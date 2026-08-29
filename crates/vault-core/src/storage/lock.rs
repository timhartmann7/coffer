//! The advisory lock file that sits beside an open database.
//!
//! The file is `<database>.lock` and carries the INI shape KeePass 2.x uses, so
//! that a KeePass 2.x client recognises it. KeePassXC 2.7 neither writes nor
//! reads a per-database lock file, so in practice this guards Coffer against
//! Coffer. It is advisory in every case: it says who has the database open, and
//! nothing enforces it.

use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::storage::{atomic::write_atomic, process, sibling};

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
    fn current() -> Self {
        Holder {
            time: chrono::Utc::now().to_rfc3339(),
            user: process::username(),
            host: process::hostname(),
            pid: Some(process::current()),
        }
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
            "[Lock]\nTime={}\nUserName={}\nMachine={}\nPID={}\n",
            self.time, self.user, self.host, pid
        )
    }

    fn parse(text: &str) -> Holder {
        let mut holder = Holder {
            time: String::new(),
            user: String::new(),
            host: String::new(),
            pid: None,
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
}

impl fmt::Debug for Lock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lock").field("path", &self.path).finish()
    }
}

impl Lock {
    /// Takes the lock beside `database`.
    ///
    /// A lock left by a process on this machine that is no longer running is
    /// replaced without comment: it describes a state of the world that ended
    /// when that process died. Any other existing lock is reported and left
    /// exactly as it was found.
    pub fn acquire(database: &Path) -> Result<Outcome, io::Error> {
        let path = path_for(database)?;

        if let Some(existing) = read(&path)?
            && !existing.is_stale()
        {
            return Ok(Outcome::Held(existing));
        }

        let holder = Holder::current();
        write_atomic::<io::Error, _>(&path, |writer: &mut dyn Write| {
            writer.write_all(holder.render().as_bytes())
        })?;

        Ok(Outcome::Taken(Lock { path }))
    }

    /// Takes the lock whether or not somebody else holds it.
    ///
    /// The only caller is a user who has been shown who holds it and said to
    /// open anyway.
    pub fn take(database: &Path) -> Result<Lock, io::Error> {
        let path = path_for(database)?;
        let holder = Holder::current();

        write_atomic::<io::Error, _>(&path, |writer: &mut dyn Write| {
            writer.write_all(holder.render().as_bytes())
        })?;

        Ok(Lock { path })
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // Only remove a lock that is still ours. Between taking it and dropping
        // it somebody may have cleared it and taken their own, and deleting
        // theirs would be worse than leaving ours behind.
        if let Ok(Some(holder)) = read(&self.path)
            && holder.pid == Some(process::current())
            && holder.host == process::hostname()
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
