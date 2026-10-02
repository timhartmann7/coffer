//! The open vault written to a file of its own somewhere else, leaving the file
//! it came from alone.
//!
//! In two steps, because only the first needs the vault. Encrypting takes the
//! tree and the key, derives the key as a save does, and settles every history
//! on the way, so it is done by whoever holds the vault. Writing the bytes out
//! takes neither, and it is aimed at a disk that is not the vault's: a stick
//! pulled out half way, a share whose server went to sleep, a slow port. Whoever
//! holds the vault through that holds every lock waiting behind it, and a Mac
//! that goes to sleep meanwhile goes with the vault open.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::Vault;
use crate::error::VaultError;
use crate::storage::{self, atomic};

/// A copy of the vault, encrypted whole and waiting to be written at the name it
/// was made for. Encrypted under the vault's own credentials, so nothing in it
/// opens without them, and it needs nothing of the vault to be written.
#[must_use = "a copy is written nowhere until it is written"]
pub struct EncryptedCopy {
    bytes: Vec<u8>,
    target: PathBuf,
}

impl Vault {
    /// The vault encrypted for a copy at `path`, which [`EncryptedCopy::write`]
    /// then writes there. Nothing about this vault changes: the copy is a copy,
    /// and the database is still the one this vault has open.
    ///
    /// Encrypting settles every entry's history the way a save does, so it
    /// counts as an edit (see [`Vault::edits`]) whether or not the copy is
    /// written afterwards. What a copy refuses before then is refused here,
    /// before any key is derived: a format Coffer will not write anywhere, the
    /// vault's own file, and one of the names Coffer keeps beside a vault
    /// ([`storage::reserved`]), where the copy would be taken for that vault's
    /// snapshot or its unsaved work.
    pub fn encrypt_copy(&mut self, path: &Path) -> Result<EncryptedCopy, VaultError> {
        // A snapshot and a read-only place are both about where the database
        // is, and a copy is written somewhere else. That is the whole point of
        // the offer: it is how the reader gets their work off a medium that
        // will not take it.
        if let Some(why) = self.source.filter(|why| !why.copyable()) {
            return Err(why.into());
        }
        if path.canonicalize().is_ok_and(|target| target == self.path) {
            return Err(VaultError::CopyOntoItself);
        }
        if storage::reserved(path) {
            return Err(VaultError::ReservedName);
        }

        let mut bytes = Vec::new();
        self.encrypt_whole(&mut bytes)?;
        Ok(EncryptedCopy {
            bytes,
            target: path.to_path_buf(),
        })
    }
}

impl EncryptedCopy {
    /// Writes the copy at the name it was made for, and only while that name
    /// holds nothing.
    ///
    /// A copy takes no snapshot of what it would replace, and the panel it is
    /// aimed from may open in the vault's own folder, where every file that
    /// must not go ends in `.kdbx`: the vault a backup was taken of, the copy a
    /// lock left, a file a backup made the vault kept aside, anybody's vault.
    /// Written over, any of them would be gone with nothing behind it, so a
    /// name that is taken - the vault's own included, by now - is refused with
    /// [`VaultError::DatabaseExists`] rather than confirmed in the panel, as a
    /// creation's is. The copy is staged beside the name first, whole and
    /// flushed, and the name is taken only then, so what is at it is never
    /// part of one.
    pub fn write(self) -> Result<(), VaultError> {
        let staged = atomic::stage::<VaultError, _>(&self.target, |writer: &mut dyn Write| {
            Ok(writer.write_all(&self.bytes)?)
        })?;
        atomic::reserve(&self.target).map_err(|error| match error.kind() {
            std::io::ErrorKind::AlreadyExists => VaultError::DatabaseExists,
            _ => VaultError::Io(error),
        })?;
        // The name holds Coffer's own empty file from here, and the rename puts
        // the whole copy over it. One that did not go through takes it back.
        staged.commit().map_err(|error| {
            let _ = std::fs::remove_file(&self.target);
            VaultError::Io(error)
        })
    }
}
