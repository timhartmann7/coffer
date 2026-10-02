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
use crate::storage::lock::claim;
use crate::storage::{atomic, no_links, sibling, snapshot};

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

/// Takes the copy away, at the reader's word and never at Coffer's, and its
/// snapshots with it (see [`retire`]).
pub fn discard(database: &Path) -> Result<(), io::Error> {
    let copy = beside(database)?;
    match std::fs::remove_file(&copy) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    retire(&copy);
    Ok(())
}

/// Moves the copy beside `database` into the vault's own name, for a vault
/// whose file has gone.
///
/// No password, because nothing is opened: the copy is a whole database under
/// the vault's credentials, and moving it is all the reader asked for. Only
/// into a name that holds nothing, though, and nothing is ever written at that
/// name before it is whole. The copy's bytes go into a temporary file of
/// Coffer's own beside it, owner-only and flushed, and that file is then
/// published at the vault's name with [`atomic::Staged::publish`], which is
/// refused if anything at all is there by then. So a file that arrives while
/// this runs - the vault dragged back out of the Trash, a sync client catching
/// up - is never written over and never removed, and the answer is
/// [`VaultError::DatabaseExists`] with both files as they were. A vault that is
/// there goes into the snapshots through [`crate::Vault::promote`], which has
/// the password a write needs.
///
/// The copy is removed only once the vault's name holds every byte of it and
/// the folder has been flushed with that name in it. A process killed part
/// way leaves the copy where it was, nothing at the vault's name, and at most a
/// temporary file the next write beside it sweeps. A folder that will not be
/// flushed after the link leaves the move made and the copy beside it, and is
/// not answered as a refusal: the vault's name already holds the copy.
///
/// A filesystem that keeps no second name for a file cannot publish without
/// replacing, so the move is refused there with [`VaultError::NoExclusiveMove`]
/// and the copy has to be opened and made the vault from inside.
///
/// The lock files beside both are taken for as long as this runs, so that
/// another Coffer holding either is told rather than having the file moved
/// out from under it, and neither note is left behind afterwards.
pub fn put_back(database: &Path) -> Result<(), VaultError> {
    let copy = beside(database)?;
    let _vault = claim(database)?;
    let _copy = claim(&copy)?;

    let mut source = File::open(&copy).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => VaultError::DatabaseGone,
        _ => VaultError::Io(error),
    })?;

    // Advice, asked before a copy that can be large is read: the answer that
    // counts is the publish's own.
    if atomic::taken(database) {
        return Err(VaultError::DatabaseExists);
    }

    let flushed = atomic::stage::<io::Error, _>(database, |writer: &mut dyn io::Write| {
        io::copy(&mut source, writer).map(drop)
    })?
    .publish()
    .map_err(unpublished)?;

    // The vault's name holds the copy from here on, so nothing below is a
    // refusal. Until the folder is known to remember that name, the copy is
    // the only name the work certainly has on the disk, and it stays.
    if flushed.is_err() {
        return Ok(());
    }
    // A copy that will not go is a copy offered again beside a vault that now
    // holds the same thing, which loses nothing and is said on the next screen.
    let _ = std::fs::remove_file(&copy);
    retire(&copy);
    Ok(())
}

/// What a refused publish means for a move into the vault's name.
fn unpublished(error: io::Error) -> VaultError {
    match error.kind() {
        io::ErrorKind::AlreadyExists => VaultError::DatabaseExists,
        // The temporary file was just made in the same folder, so the
        // filesystem is what refused: it keeps no second name for a file.
        _ if no_links(&error) => VaultError::NoExclusiveMove,
        _ => VaultError::Io(error),
    }
}

/// Takes away the snapshots a copy rotated beside its own name, once the copy
/// has gone: into the vault's name, over the vault, or at the reader's word.
///
/// Every save made inside an open copy pushes a chain beside the copy, as any
/// save does. Once the copy is not there to be chosen, nothing in Coffer lists
/// that chain or offers it, and it would sit in the folder as earlier states of
/// the vault that open with whatever password the vault had then - long after
/// the reader changed it because they thought it had leaked. The next copy a
/// lock left would then rotate into it, and two rescues' generations would
/// share one chain. The vault's own chain is untouched: what the copy replaced
/// is in it.
///
/// Best effort: the move it follows has happened, and a snapshot that will not
/// go is no reason to say it did not.
pub(crate) fn retire(copy: &Path) {
    let _ = snapshot::clear(copy);
}

#[cfg(test)]
mod tests {
    use super::unpublished;
    use crate::error::VaultError;
    use std::io;

    /// A disk without hard links cannot be made inside a test, so this is
    /// where the answer to one is pinned: the errnos FAT, exFAT and the shares
    /// that lack links give for a link are a refusal that sends the reader to
    /// open the copy, a name already taken is the one answer about the vault,
    /// and anything else is a fault said as one.
    #[test]
    fn a_publish_refused_is_read_for_what_it_says_about_the_move() {
        assert!(matches!(
            unpublished(io::Error::from_raw_os_error(libc::EEXIST)),
            VaultError::DatabaseExists
        ));
        for errno in [libc::EPERM, libc::EACCES, libc::ENOTSUP, libc::EOPNOTSUPP] {
            assert!(
                matches!(
                    unpublished(io::Error::from_raw_os_error(errno)),
                    VaultError::NoExclusiveMove
                ),
                "errno {errno}"
            );
        }
        for errno in [libc::ENOSPC, libc::EIO, libc::ENOENT] {
            assert!(
                matches!(
                    unpublished(io::Error::from_raw_os_error(errno)),
                    VaultError::Io(_)
                ),
                "errno {errno}"
            );
        }
    }
}
