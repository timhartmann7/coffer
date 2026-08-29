//! Noticing that the database changed underneath us.
//!
//! Coffer never overwrites a file another client has touched. Every open and
//! every save records what the file looked like, and the next save checks that
//! the file still looks that way.

use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::SystemTime;

/// What the database looked like the last time Coffer and the disk agreed.
///
/// Modification time alone is not enough. Its granularity is a second on some
/// filesystems, so a change inside the same tick is invisible; and every client
/// that saves the way Coffer does replaces the file by rename, which changes the
/// inode without necessarily moving the clock. Size, inode and device close
/// those gaps between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    modified: Option<SystemTime>,
    /// Change time, which a writer cannot set backwards the way it can set
    /// modification time.
    changed: i64,
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
            changed: metadata.ctime_nsec().wrapping_add(metadata.ctime()),
            len: metadata.len(),
            inode: metadata.ino(),
            device: metadata.dev(),
        })
    }
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

/// Compares `path` against a stamp taken earlier.
pub fn since(path: &Path, stamp: Stamp) -> Result<Change, io::Error> {
    match Stamp::of(path) {
        Ok(current) if current == stamp => Ok(Change::None),
        Ok(_) => Ok(Change::Modified),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Change::Gone),
        Err(error) => Err(error),
    }
}
