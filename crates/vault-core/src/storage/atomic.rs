//! Replacing a file without ever leaving a half-written database on disk.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use crate::storage::{OWNER_ONLY, parent_of, process, sibling, sync_directory};

const TEMPORARY_SUFFIX: &str = ".coffer-tmp";

/// Writes `path` by filling a temporary file beside it and renaming over the
/// target.
///
/// The sequence is fixed: create the temporary file with owner-only permissions,
/// let `fill` write it, flush it to the platter, rename, then flush the
/// directory entry. A process killed anywhere before the rename leaves the
/// original database exactly as it was, and leaves behind a temporary file that
/// the next successful save sweeps up.
///
/// `fill` is the seam the failure tests drive: a closure that stops halfway or
/// reports a full volume proves that the original survives.
pub fn write_atomic<E, F>(path: &Path, fill: F) -> Result<(), E>
where
    E: From<io::Error>,
    F: FnOnce(&mut dyn Write) -> Result<(), E>,
{
    let directory = parent_of(path)?;
    let temporary = temporary_path(path)?;

    // A temporary file from a previous run of this same process id is the only
    // one we may assume is ours to remove.
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let outcome = fill_and_rename(path, &temporary, directory, fill);

    if outcome.is_err() {
        let _ = fs::remove_file(&temporary);
        return outcome;
    }

    sweep_abandoned(path);
    outcome
}

fn fill_and_rename<E, F>(path: &Path, temporary: &Path, directory: &Path, fill: F) -> Result<(), E>
where
    E: From<io::Error>,
    F: FnOnce(&mut dyn Write) -> Result<(), E>,
{
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_ONLY)
        .open(temporary)?;

    fill(&mut file)?;

    file.flush()?;
    file.sync_all()?;
    drop(file);

    fs::rename(temporary, path)?;
    sync_directory(directory)?;

    Ok(())
}

/// Proves a write to `path` can happen, before anything on disk is disturbed.
///
/// Two separate permissions are needed and neither implies the other: the
/// directory has to accept a new file, and the database itself has to be
/// writable. `rename` needs no write permission on the file it replaces, so
/// without this check a read-only database would be quietly replaced and come
/// back writable.
pub fn ensure_writable(path: &Path) -> Result<(), io::Error> {
    OpenOptions::new().write(true).open(path)?;

    let temporary = temporary_path(path)?;
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

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
