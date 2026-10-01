//! The copy a lock leaves behind when it could not save.
//!
//! Locking wipes the decrypted tree, which is the whole of what locking means.
//! A vault is dirty exactly when saving is the thing that failed - somebody else
//! wrote the file, the volume was unplugged, the folder went read only - so a
//! wipe on its own is a session's work destroyed by a timer nobody watched. What
//! could not be saved goes here instead.
//!
//! It is an ordinary KDBX 4.1 database under the same credentials, and it stays
//! where it is until the reader takes it away: removed, or put back as the
//! vault it was taken from. Nothing in Coffer removes it on its own: it holds
//! the only copy of work the vault has not got.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::VaultError;
use crate::storage::lock::{Lock, Outcome};
use crate::storage::{atomic, sibling};

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

/// The vault a copy was taken from, read off the copy's name, or nothing when
/// the name is not a copy's.
///
/// The name is the whole of the record. Nothing about the copy is written
/// anywhere else: not inside it, where it would be a field no other client
/// knows, and not in a file beside it, which would be one more thing to lose
/// or to disagree with the name.
pub fn taken_from(copy: &Path) -> Option<PathBuf> {
    let name = copy.file_name()?.to_str()?;
    let vault = name
        .strip_suffix(SUFFIX)
        .filter(|vault| !vault.is_empty())?;
    Some(copy.with_file_name(vault))
}

/// Whether a path is one of these. Half of [`super::reserved`], which is the
/// question everything else asks.
pub(crate) fn reserved(path: &Path) -> bool {
    taken_from(path).is_some()
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

/// Moves the copy beside `database` into the vault's own name, for a vault
/// whose file has gone.
///
/// No password, because nothing is opened: the copy is a whole database under
/// the vault's credentials, and moving it is all the reader asked for. Only
/// into a name that holds nothing, though. The name is taken with an exclusive
/// create before a byte moves, so a file that arrives while this runs - the
/// vault dragged back out of the Trash, a sync client catching up - is never
/// written over, and the answer is [`VaultError::DatabaseExists`] with both
/// files as they were. A vault that is there goes into the snapshots through
/// [`crate::Vault::promote`], which has the password a write needs.
///
/// The bytes go through the staged writer into the name that was taken, so the
/// vault is born owner-only and whole, and the copy is removed only once the
/// vault's name holds every byte of it. A process killed part way leaves the
/// copy where it was.
///
/// The lock files beside both are taken for as long as this runs, so that
/// another Coffer holding either is told rather than having the file moved
/// out from under it, and neither note is left behind afterwards.
pub fn put_back(database: &Path) -> Result<(), VaultError> {
    let copy = beside(database)?;
    let _vault = claim(database)?;
    let _copy = claim(&copy)?;

    // Opened before the name is taken, so that a copy that is not there leaves
    // nothing at the vault's name either.
    let mut source = File::open(&copy).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => VaultError::DatabaseGone,
        _ => VaultError::Io(error),
    })?;

    atomic::reserve(database).map_err(|error| match error.kind() {
        io::ErrorKind::AlreadyExists => VaultError::DatabaseExists,
        _ => VaultError::Io(error),
    })?;

    // From here the file at the vault's name is this function's, and every way
    // out that is not a vault has to take it back off the disk: an empty file
    // there would be a vault that will not open, standing in the way of the
    // next attempt.
    let moved = atomic::stage::<io::Error, _>(database, |writer: &mut dyn io::Write| {
        io::copy(&mut source, writer).map(drop)
    })
    .and_then(atomic::Staged::commit);
    if let Err(error) = moved {
        let _ = std::fs::remove_file(database);
        return Err(VaultError::Io(error));
    }

    // A copy that will not go is a copy offered again beside a vault that now
    // holds the same thing, which loses nothing and is said on the next screen.
    let _ = std::fs::remove_file(&copy);
    Ok(())
}

/// Takes the lock beside a file a copy is about to be moved onto or off, for
/// as long as the move runs.
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
