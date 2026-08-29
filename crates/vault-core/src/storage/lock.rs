//! The advisory lock file that sits beside an open database.
//!
//! The file is `<database>.lock` and carries the INI shape KeePass 2.x uses, so
//! that a KeePass 2.x client recognises it. KeePassXC 2.7 neither writes nor
//! reads a per-database lock file, so in practice this guards Coffer against
//! Coffer. It is advisory in every case: it says who has the database open, and
//! nothing enforces it.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::storage::{process, sibling};

const LOCK_SUFFIX: &str = ".lock";

/// Who is holding a database open, as far as their lock file admits.
#[derive(Clone, PartialEq, Eq)]
pub struct Holder {
    /// When the lock was taken, RFC 3339.
    pub time: String,
    /// The account that took it.
    pub user: String,
    /// The machine it was taken on. A lock from another machine is never
    /// treated as stale, because its process ids mean nothing here.
    pub host: String,
    /// The process that took it, absent in a lock written by a client that does
    /// not record one.
    pub pid: Option<u32>,
    /// Which lock inside that process took it. A process may hold locks on
    /// several databases, and dropping one must not remove another's file.
    token: u64,
}

impl fmt::Debug for Holder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Holder")
            .field("user", &self.user)
            .field("host", &self.host)
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

impl Holder {
    fn current(token: u64) -> Self {
        Holder {
            time: chrono::Utc::now().to_rfc3339(),
            user: process::username(),
            host: process::hostname(),
            pid: Some(process::current()),
            token,
        }
    }

    /// Whether this lock is the one a given `Lock` value wrote.
    fn is(&self, token: u64) -> bool {
        self.token == token
            && self.pid == Some(process::current())
            && self.host == process::hostname()
    }

    /// A lock is stale when it was taken on this machine by a process that has
    /// since gone. A lock from another machine, or one without a process id,
    /// has to be believed.
    fn is_stale(&self) -> bool {
        match self.pid {
            Some(pid) if self.host == process::hostname() => !process::is_alive(pid),
            _ => false,
        }
    }

    fn render(&self) -> String {
        let pid = self.pid.map(|p| p.to_string()).unwrap_or_default();
        format!(
            "[Lock]\nTime={}\nUserName={}\nMachine={}\nPID={}\nToken={}\n",
            self.time, self.user, self.host, pid, self.token
        )
    }

    fn parse(text: &str) -> Holder {
        let mut holder = Holder {
            time: String::new(),
            user: String::new(),
            host: String::new(),
            pid: None,
            token: 0,
        };

        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "Time" => holder.time = value.trim().to_owned(),
                "UserName" => holder.user = value.trim().to_owned(),
                "Machine" => holder.host = value.trim().to_owned(),
                "PID" => holder.pid = value.trim().parse().ok(),
                "Token" => holder.token = value.trim().parse().unwrap_or_default(),
                _ => {}
            }
        }

        holder
    }
}

/// The outcome of asking for a database's lock.
#[derive(Debug)]
pub enum Outcome {
    /// The lock is ours. Dropping it removes the file.
    Taken(Lock),
    /// Somebody else has the database open and their lock was left alone. The
    /// caller decides whether to open anyway.
    Held(Holder),
}

/// A held lock file. Removed when this value is dropped.
pub struct Lock {
    path: PathBuf,
    token: u64,
}

impl fmt::Debug for Lock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lock").field("path", &self.path).finish()
    }
}

impl Lock {
    /// Takes the lock beside `database`.
    ///
    /// The file is created exclusively, so two processes racing for the same
    /// database cannot both be told they took it. A lock left by a process on
    /// this machine that is no longer running is removed and the attempt is
    /// made again: it describes a state of the world that ended when that
    /// process died. Any other existing lock is reported and left exactly as it
    /// was found.
    pub fn acquire(database: &Path) -> Result<Outcome, io::Error> {
        let path = path_for(database)?;

        match create(&path)? {
            Some(lock) => Ok(Outcome::Taken(lock)),
            None => {
                let Some(existing) = read(&path)? else {
                    // It went between the failed create and the read. One more
                    // attempt, and no more: a caller stuck in a loop here is
                    // worse than a caller told somebody else has the database.
                    return match create(&path)? {
                        Some(lock) => Ok(Outcome::Taken(lock)),
                        None => Ok(Outcome::Held(read(&path)?.unwrap_or_else(placeholder))),
                    };
                };

                if !existing.is_stale() {
                    return Ok(Outcome::Held(existing));
                }

                std::fs::remove_file(&path)?;
                match create(&path)? {
                    Some(lock) => Ok(Outcome::Taken(lock)),
                    None => Ok(Outcome::Held(read(&path)?.unwrap_or_else(placeholder))),
                }
            }
        }
    }

    /// Takes the lock whether or not somebody else holds it.
    ///
    /// The only caller is a user who has been shown who holds it and said to
    /// open anyway.
    pub fn take(database: &Path) -> Result<Lock, io::Error> {
        let path = path_for(database)?;

        loop {
            if let Some(lock) = create(&path)? {
                return Ok(lock);
            }
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
    }
}

/// Writes a lock file, or reports that one is already there.
///
/// `create_new` is what makes this safe against another Coffer doing the same
/// thing at the same moment: the file appears or it does not, and only one
/// caller can be the one that made it.
fn create(path: &Path) -> Result<Option<Lock>, io::Error> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;

    let token = next_token();
    let holder = Holder::current(token);

    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(crate::storage::OWNER_ONLY)
        .open(path)
    {
        Ok(mut file) => {
            file.write_all(holder.render().as_bytes())?;
            Ok(Some(Lock {
                path: path.to_path_buf(),
                token,
            }))
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(None),
        Err(error) => Err(error),
    }
}

/// A distinct number for every lock this process takes.
fn next_token() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// What to report when a lock file exists but cannot be read back, which means
/// somebody is holding it and has not finished writing it.
fn placeholder() -> Holder {
    Holder {
        time: String::new(),
        user: String::new(),
        host: String::new(),
        pid: None,
        token: 0,
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // Only remove the lock this value wrote. Between taking it and dropping
        // it somebody may have cleared it and taken their own - another process,
        // or another Lock in this one - and deleting theirs would be worse than
        // leaving ours behind.
        if let Ok(Some(holder)) = read(&self.path)
            && holder.is(self.token)
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Reads the lock beside `database` without taking it.
pub fn inspect(database: &Path) -> Result<Option<Holder>, io::Error> {
    read(&path_for(database)?)
}

fn path_for(database: &Path) -> Result<PathBuf, io::Error> {
    sibling(database, LOCK_SUFFIX)
}

fn read(path: &Path) -> Result<Option<Holder>, io::Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(Holder::parse(&text))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
