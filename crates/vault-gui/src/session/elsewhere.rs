//! The open vault written to a file of its own somewhere else: the one door
//! every copy goes through, from the conflict dialog, a backup's strip, the
//! read-only note, and the copy kept on another disk.

use std::path::Path;

use super::Session;
use crate::error::Failure;

impl Session {
    /// Writes the open vault to `target`, while it is still the vault at
    /// `vault`: see [`vault_core::Vault::encrypt_copy`].
    ///
    /// `vault` is the one the window asked about before a panel opened. The
    /// panel is a sheet and the clock that locks keeps running behind it, so
    /// by the time a place is picked the vault may have locked - `noVault` -
    /// or another may be open after the lock, and that one is not written
    /// under a name offered for the first.
    ///
    /// Held for the encryption and its key derivation, as a save is, and let
    /// go for the write. The write goes to whatever disk the reader picked: a
    /// stick pulled out half way, or a share whose server went to sleep, holds
    /// it until the system gives up on that disk, and every lock, sleep and
    /// Quit would wait behind it with the vault open. The bytes are encrypted
    /// by then, and a lock that lands meanwhile takes the vault and leaves
    /// them. Copies are still written one at a time: two at one name would
    /// stage into one temporary file.
    ///
    /// A copy settles every entry's history the way a save does, so the
    /// revision moves and a list of versions read before it is read again,
    /// whether or not the write then goes through. What is being typed and not
    /// yet written into the vault is not in the copy; it stays a draft, for
    /// the next lock to write into the vault.
    pub fn copy_to(&self, vault: &Path, target: &Path) -> Result<(), Failure> {
        let encrypted = self.with_mut(|open| {
            if open.path() != vault {
                return Err(Failure::refused(
                    "another vault was opened while the panel was up, so nothing was copied",
                ));
            }
            Ok(open.encrypt_copy(target)?)
        })??;
        let _one_at_a_time = self
            .copying
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Ok(encrypted.write()?)
    }
}
