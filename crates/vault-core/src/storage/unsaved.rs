//! The copy a lock leaves behind when it could not save.
//!
//! Locking wipes the decrypted tree, which is the whole of what locking means.
//! A vault is dirty exactly when saving is the thing that failed - somebody else
//! wrote the file, the volume was unplugged, the folder went read only - so a
//! wipe on its own is a session's work destroyed by a timer nobody watched. What
//! could not be saved goes here instead.
//!
//! It is an ordinary KDBX 4.1 database under the same credentials, and it stays
//! where it is until the reader takes it away. Nothing in Coffer removes it on
//! its own: it holds the only copy of work the vault has not got.

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::storage::sibling;

/// Appended to the whole file name, the way `.lock` and `.1.bak` are, so that
/// the copy lands in the folder the reader chose. The `.kdbx` on the end is
/// load-bearing: every file panel in the application filters on that extension,
/// and a rescue the reader cannot pick out of "Open another database" would
/// exist only in the Finder.
const SUFFIX: &str = ".unsaved.kdbx";

/// Where the copy for `database` goes.
pub fn beside(database: &Path) -> Result<PathBuf, io::Error> {
    sibling(database, SUFFIX)
}

/// Whether a path is one of these, so that nothing makes a vault at the name.
///
/// A database created there would be overwritten without a word by the next
/// lock that had something to keep.
pub fn reserved(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.len() > SUFFIX.len() && name.ends_with(SUFFIX))
}

/// A copy sitting beside a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub path: PathBuf,
    /// When it was written, as the filesystem has it. Absent on a filesystem
    /// that does not keep the time.
    pub written: Option<SystemTime>,
}

/// The copy beside `database`, when there is one.
///
/// Asked of the filesystem rather than remembered, so that a copy left by a run
/// that has since quit is still offered on the next launch.
pub fn found(database: &Path) -> Result<Option<Kept>, io::Error> {
    let path = beside(database)?;
    match std::fs::metadata(&path) {
        Ok(about) if about.is_file() => Ok(Some(Kept {
            path,
            written: about.modified().ok(),
        })),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Takes the copy away, at the reader's word and never at Coffer's.
pub fn discard(database: &Path) -> Result<(), io::Error> {
    match std::fs::remove_file(beside(database)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
