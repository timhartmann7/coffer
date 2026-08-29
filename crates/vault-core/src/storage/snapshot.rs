//! Ten rotating copies of the database, taken before every write.
//!
//! `<database>.1.bak` is the most recent and `<database>.10.bak` the oldest.
//! Together they cover roughly the last hour of active work; entry history in
//! the database itself covers the months before that.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::storage::{atomic::write_atomic, sibling};

/// Tightens a snapshot to owner-only.
///
/// A hard link shares its inode with the database, so this also tightens the
/// database. That is the same thing the next save does anyway, and the spec asks
/// for owner-only on both.
fn owner_only(path: &Path) -> Result<(), io::Error> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        path,
        std::fs::Permissions::from_mode(crate::storage::OWNER_ONLY),
    )
}

/// How many snapshots are kept. The eleventh push drops the oldest.
pub const SNAPSHOT_COUNT: u32 = 10;

/// Pushes the current contents of `database` in at slot 1 and shifts every
/// other snapshot down one.
///
/// The shift runs from the oldest slot towards the newest, so a process killed
/// part way through leaves a duplicated snapshot rather than a hole. Nothing is
/// removed except the copy that was already falling off the end.
///
/// **Slot 1 is a hard link, so the caller must replace the database by rename
/// and never in place.** A writer that truncates and rewrites the database
/// would be rewriting the snapshot with it, and one save would silently empty
/// the whole chain. That is why this is not public: the only caller is
/// [`Vault::save`][crate::Vault::save], which replaces the file by rename.
///
/// A database that does not exist yet has nothing to snapshot, which is not an
/// error.
pub(crate) fn rotate(database: &Path) -> Result<(), io::Error> {
    if !database.try_exists()? {
        return Ok(());
    }

    remove_if_present(&slot(database, SNAPSHOT_COUNT)?)?;

    for index in (1..SNAPSHOT_COUNT).rev() {
        let from = slot(database, index)?;
        if from.try_exists()? {
            std::fs::rename(&from, slot(database, index + 1)?)?;
        }
    }

    capture(database, &slot(database, 1)?)
}

/// The path of one snapshot slot, counting from 1 for the most recent.
pub fn slot(database: &Path, index: u32) -> Result<PathBuf, io::Error> {
    sibling(database, &format!(".{index}.bak"))
}

fn remove_if_present(path: &Path) -> Result<(), io::Error> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Puts the current contents of the database in slot 1.
///
/// A hard link is the whole of it on a normal filesystem: the snapshot and the
/// database name the same bytes, and the atomic save that follows gives the
/// database a new inode, leaving the snapshot holding what was there before.
/// Nothing is copied, so a hundred-megabyte database costs nothing to snapshot
/// and the database path is never absent for even a moment.
///
/// Filesystems that refuse hard links fall back to a copy through the atomic
/// writer, so that a snapshot is either whole or absent.
fn capture(database: &Path, target: &Path) -> Result<(), io::Error> {
    match std::fs::hard_link(database, target) {
        Ok(()) => return owner_only(target),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            std::fs::remove_file(target)?;
            if std::fs::hard_link(database, target).is_ok() {
                return owner_only(target);
            }
        }
        Err(_) => {}
    }

    write_atomic::<io::Error, _>(target, |writer: &mut dyn Write| {
        let mut source = File::open(database)?;
        io::copy(&mut source, writer)?;
        Ok(())
    })
}
