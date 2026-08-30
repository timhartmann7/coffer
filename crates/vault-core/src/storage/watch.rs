//! Noticing that the database changed underneath us.
//!
//! Coffer never overwrites a file another client has touched. Every open and
//! every save records what the file looked like, and the next save checks that
//! the file still looks that way.

use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::SystemTime;

use sha2::{Digest, Sha256};

/// What the database looked like the last time Coffer and the disk agreed.
///
/// Modification time alone is not enough. Its granularity is a second on some
/// filesystems, so a change inside the same tick is invisible; and every client
/// that saves the way Coffer does replaces the file by rename, which changes the
/// inode without necessarily moving the clock. Size, inode and device close
/// those gaps between them.
///
/// Change time is the one field a writer cannot set for itself, which is why it
/// is here - and why it cannot be trusted on its own. macOS writes
/// `com.apple.macl` and `com.apple.provenance` onto a file after an application
/// has touched it through a save panel, and an extended attribute moves change
/// time and nothing else: not the size, not the modification time, not the
/// inode. A vault that read that as another client's edit refused to save its
/// own file and asked the reader to resolve a conflict with themselves, on every
/// write they made.
///
/// So when change time is the only thing that moved, the bytes decide. That is
/// the one case where the metadata cannot: an extended attribute the system
/// wrote and a client that rewrote the file in place and put the modification
/// time back look exactly alike from the outside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    modified: Option<SystemTime>,
    /// Seconds and nanoseconds kept apart, because `SystemTime` cannot hold a
    /// change time from before 1970 and a file can carry one.
    changed: (i64, i64),
    len: u64,
    inode: u64,
    device: u64,
}

impl Stamp {
    /// Reads the current state of `path`.
    pub fn of(path: &Path) -> Result<Stamp, io::Error> {
        let metadata = std::fs::metadata(path)?;
        Ok(Stamp {
            modified: metadata.modified().ok(),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
            len: metadata.len(),
            inode: metadata.ino(),
            device: metadata.dev(),
        })
    }

    /// Whether these two differ in nothing but change time.
    fn same_but_for_change_time(self, other: Stamp) -> bool {
        Stamp {
            changed: other.changed,
            ..self
        } == other
    }
}

/// The bytes of the database, hashed.
///
/// Only ever compared against another of these. It is not a signature and it
/// protects nothing: a writer who can rewrite the vault can rewrite this too.
/// What it answers is narrower - whether the file holds what Coffer put there -
/// and for that a plain digest is enough.
pub type Content = [u8; 32];

pub fn digest(bytes: &[u8]) -> Content {
    Sha256::digest(bytes).into()
}

/// How the file on disk relates to a stamp taken earlier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// The file is the one we last agreed with.
    None,
    /// Somebody wrote it.
    Modified,
    /// It is not there any more.
    Gone,
}

/// Compares `path` against a stamp taken earlier, and against the bytes that
/// went with it.
pub fn since(path: &Path, stamp: Stamp, content: Content) -> Result<Change, io::Error> {
    let current = match Stamp::of(path) {
        Ok(current) => current,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Change::Gone),
        Err(error) => return Err(error),
    };

    if current == stamp {
        return Ok(Change::None);
    }

    // The file is read only here, and only when the metadata cannot answer: an
    // attribute the system wrote and a client that put the modification time
    // back are the same shape from the outside.
    if current.same_but_for_change_time(stamp) {
        let found = digest(&std::fs::read(path)?);
        return Ok(if found == content {
            Change::None
        } else {
            Change::Modified
        });
    }

    Ok(Change::Modified)
}
