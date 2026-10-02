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

use crate::error::VaultError;
use crate::storage::{process, refuses_writes, sibling};

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
            .field("time", &self.time)
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

    /// Who holds this lock, in a sentence.
    ///
    /// Every field is optional in practice: a lock file is written by whoever
    /// can write the directory beside the database, and a client that records
    /// no account or no machine is a client that records none. A sentence with
    /// the gaps left in it reads as a bug, so each shape has its own wording.
    ///
    /// The values are shown and never acted on, and they are clipped: a lock
    /// file naming a megabyte of account is not something to put on a screen.
    pub fn describe(&self) -> String {
        /// Long enough for any real account or machine name.
        const SHOWN: usize = 60;

        fn short(text: &str) -> &str {
            let text = text.trim();
            match text.char_indices().nth(SHOWN) {
                Some((at, _)) => text.get(..at).unwrap_or_default(),
                None => text,
            }
        }

        let (user, host, time) = (short(&self.user), short(&self.host), short(&self.time));

        let who = match (user.is_empty(), host.is_empty()) {
            (false, false) => format!("{user} has this vault open on {host}"),
            (false, true) => format!("{user} has this vault open"),
            (true, false) => format!("this vault is open on {host}"),
            (true, true) => "another process has this vault open".to_owned(),
        };

        if time.is_empty() {
            who
        } else {
            format!("{who}, since {time}")
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
    /// The place beside the database will not take a file, so there is no lock
    /// to write and nothing for one to guard: nobody can write the database
    /// through this path either. The caller opens it to be read.
    Unwritable,
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

        match create(&path) {
            Ok(Some(lock)) => Ok(Outcome::Taken(lock)),
            // Reading a database needs no write, so a place that will not take
            // the note beside it is not a reason to refuse the database.
            Err(error) if refuses_writes(&error) => Ok(Outcome::Unwritable),
            Err(error) => Err(error),
            Ok(None) => {
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

/// Takes the lock beside a file that is about to be written over, or moved
/// onto or off, by something other than the vault open at it: a copy a lock
/// left going back, or a copy or a backup becoming the vault. Held for as long
/// as that runs.
///
/// Stricter than opening, on purpose. A lock somebody else holds is refused
/// rather than offered to be taken over, because this is not a reader asking
/// to open a vault they were shown is held; and a place that will not take the
/// note beside a file will not take the file either.
pub(crate) fn claim(path: &Path) -> Result<Lock, VaultError> {
    match Lock::acquire(path)? {
        Outcome::Taken(lock) => Ok(lock),
        Outcome::Held(holder) => Err(VaultError::Locked(holder)),
        Outcome::Unwritable => Err(VaultError::ReadOnlyPlace),
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

/// Reads a lock file, or says there is none.
///
/// A file that is there and will not be read counts as a lock held by somebody
/// unknown. That happens on a share where the far end owns the file, and the
/// answer has to be the cautious one: a lock nobody can read is still a lock,
/// and the reader is shown the same offer to open anyway that any other holder
/// gets.
fn read(path: &Path) -> Result<Option<Holder>, io::Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(Holder::parse(&text))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) if refuses_writes(&error) => Ok(Some(placeholder())),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holder(user: &str, host: &str, time: &str) -> Holder {
        Holder {
            time: time.to_owned(),
            user: user.to_owned(),
            host: host.to_owned(),
            pid: None,
            token: 0,
        }
    }

    /// A lock file is written by whoever can write the directory beside the
    /// database, so every field in it is somebody else's text and any of them
    /// may be missing. None of these may come out as a sentence with a hole in
    /// it, and none of them may come out a kilometre long.
    #[test]
    fn a_lock_file_that_says_little_still_reads_as_a_sentence() {
        assert_eq!(
            holder("someone", "a-mac", "2026-08-30T10:00:00Z").describe(),
            "someone has this vault open on a-mac, since 2026-08-30T10:00:00Z"
        );
        assert_eq!(
            holder("someone", "", "").describe(),
            "someone has this vault open"
        );
        assert_eq!(
            holder("", "a-mac", "").describe(),
            "this vault is open on a-mac"
        );
        assert_eq!(
            holder("", "", "").describe(),
            "another process has this vault open"
        );
        assert_eq!(
            holder(" ", "\t", " ").describe(),
            "another process has this vault open"
        );
    }

    /// The fields are shown, so a lock file naming a kilometre of account is a
    /// lock file that would fill the screen with it. Cut on a character and
    /// never inside one.
    #[test]
    fn a_lock_file_that_says_far_too_much_is_cut_short() {
        let long = holder(&"n".repeat(4096), &"h".repeat(4096), &"t".repeat(4096));
        assert!(long.describe().len() < 250, "{}", long.describe().len());

        let wide = holder(&"e\u{301}".repeat(500), "", "");
        let said = wide.describe();
        assert!(said.len() < 250, "{}", said.len());
        assert!(said.is_char_boundary(said.len()));
    }
}
