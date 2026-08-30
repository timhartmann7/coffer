//! Replacing a file without ever leaving a half-written database on disk.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use crate::storage::{OWNER_ONLY, parent_of, process, sibling, sync_directory};

const TEMPORARY_SUFFIX: &str = ".coffer-tmp";

/// A temporary file, filled and flushed, waiting to be renamed over its target.
///
/// Splitting the write in two is what lets a caller do everything that can fail
/// before it disturbs anything on disk: filling this is where a full volume, a
/// serialisation error or a killed process shows up, and the target is still
/// exactly as it was at that point. Dropping a `Staged` without committing
/// removes the temporary file.
#[must_use = "a staged write does nothing until it is committed"]
pub struct Staged {
    temporary: PathBuf,
    target: PathBuf,
    committed: bool,
}

impl Staged {
    /// Renames the temporary file over the target and flushes the directory
    /// entry, so that the new name survives a power cut.
    pub fn commit(mut self) -> Result<(), io::Error> {
        let directory = parent_of(&self.target)?;

        fs::rename(&self.temporary, &self.target)?;
        sync_directory(directory)?;
        self.committed = true;

        sweep_abandoned(&self.target);
        Ok(())
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.temporary);
        }
    }
}

/// Fills a temporary file beside `path`, ready to be renamed over it.
///
/// The temporary file is created with owner-only permissions, filled by `fill`,
/// flushed to the platter and closed. A process killed anywhere in here leaves
/// the original exactly as it was, and leaves behind a temporary file that the
/// next successful commit sweeps up.
///
/// `fill` is the seam the failure tests drive: a closure that stops halfway or
/// reports a full volume proves that the original survives.
pub fn stage<E, F>(path: &Path, fill: F) -> Result<Staged, E>
where
    E: From<io::Error>,
    F: FnOnce(&mut dyn Write) -> Result<(), E>,
{
    let temporary = temporary_path(path)?;
    remove_if_present(&temporary)?;

    let staged = Staged {
        temporary,
        target: path.to_path_buf(),
        committed: false,
    };

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_ONLY)
        .open(&staged.temporary)?;

    fill(&mut file)?;

    file.flush()?;
    file.sync_all()?;

    Ok(staged)
}

/// Takes a name, atomically, without putting anything in it.
///
/// Asking whether a file is there and then writing one are two acts, and
/// everything between them is a window: creating a vault spends the whole of a
/// calibrated second of key derivation in it, and a database that arrived
/// meanwhile would be renamed away with no snapshot behind it. Taking the name
/// is the question and the answer at once.
///
/// The caller owns what it reserved, and has to take it back off the disk if it
/// then decides not to fill it.
pub fn reserve(path: &Path) -> Result<(), io::Error> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_ONLY)
        .open(path)
        .map(drop)
}

/// Fills and commits in one step, for writers with nothing to do in between.
pub fn write_atomic<E, F>(path: &Path, fill: F) -> Result<(), E>
where
    E: From<io::Error>,
    F: FnOnce(&mut dyn Write) -> Result<(), E>,
{
    Ok(stage(path, fill)?.commit()?)
}

/// A temporary file from a previous run of this same process id is the only one
/// Coffer may assume is its own to remove.
fn remove_if_present(path: &Path) -> Result<(), io::Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Proves a write to `path` can happen, before anything on disk is disturbed.
///
/// Two separate permissions are needed and neither implies the other: the
/// directory has to accept a new file, and the database itself has to be
/// writable. `rename` needs no write permission on the file it replaces, so
/// without this check a read-only database would be quietly replaced and come
/// back writable.
pub(crate) fn ensure_writable(path: &Path) -> Result<(), io::Error> {
    OpenOptions::new().write(true).open(path)?;

    let temporary = temporary_path(path)?;
    remove_if_present(&temporary)?;

    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_ONLY)
        .open(&temporary)?;
    fs::remove_file(&temporary)
}

fn temporary_path(database: &Path) -> Result<PathBuf, io::Error> {
    sibling(
        database,
        &format!(".{}{TEMPORARY_SUFFIX}", process::current()),
    )
}

/// Removes temporary files left beside this database by processes that are no
/// longer running. They hold an encrypted database, not a plaintext one, but
/// they are still copies of the user's data sitting where nobody asked for them.
fn sweep_abandoned(database: &Path) {
    let (Ok(directory), Some(stem)) = (parent_of(database), database.file_name()) else {
        return;
    };
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(pid) = abandoned_pid(name, &stem.to_string_lossy()) else {
            continue;
        };

        if !process::is_alive(pid) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Reads the process id out of `<database>.<pid>.coffer-tmp`, or `None` when the
/// name belongs to something else.
fn abandoned_pid(name: &str, database_name: &str) -> Option<u32> {
    let rest = name.strip_prefix(database_name)?.strip_prefix('.')?;
    rest.strip_suffix(TEMPORARY_SUFFIX)?.parse().ok()
}
