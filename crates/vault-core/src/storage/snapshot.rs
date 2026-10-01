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

/// Which snapshot slot a path names, when it names one.
///
/// The name is Coffer's own convention, so a file that carries it is treated as
/// one of Coffer's snapshots: it opens like any other database and is not
/// written back, because the next save of the database beside it would rotate
/// it away.
pub fn slot_of(path: &Path) -> Option<u32> {
    let name = path.file_name()?.to_str()?;
    let (database, index) = name.strip_suffix(".bak")?.rsplit_once('.')?;
    if database.is_empty() {
        return None;
    }

    let index: u32 = index.parse().ok()?;
    (1..=SNAPSHOT_COUNT).contains(&index).then_some(index)
}

/// A snapshot that exists on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taken {
    /// Which slot it is in, counting from 1 for the most recent.
    pub index: u32,
    pub path: PathBuf,
    /// When it was taken, as far as the filesystem knows. Absent on a
    /// filesystem that does not keep the time.
    pub taken: Option<std::time::SystemTime>,
}

/// The snapshots beside `database`, most recent first.
///
/// A slot that is not there is skipped rather than reported: the chain is a
/// history of saves, and a database saved three times has three of them. This
/// is what the screen offers when a database will not open, so it asks about
/// the file the user chose rather than about a database Coffer managed to read.
pub fn taken(database: &Path) -> Result<Vec<Taken>, io::Error> {
    let mut found = Vec::new();

    for index in 1..=SNAPSHOT_COUNT {
        let path = slot(database, index)?;
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };

        if metadata.is_file() {
            found.push(Taken {
                index,
                path,
                taken: metadata.modified().ok(),
            });
        }
    }

    Ok(found)
}

/// Takes every snapshot beside `database` off the disk. Only for a database
/// that has gone for good: see [`crate::storage::unsaved::retire`].
pub(crate) fn clear(database: &Path) -> Result<(), io::Error> {
    for index in 1..=SNAPSHOT_COUNT {
        remove_if_present(&slot(database, index)?)?;
    }
    Ok(())
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
