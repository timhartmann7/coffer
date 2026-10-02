//! Giving a vault a new master password, and taking away the snapshots that
//! still open with the old one.
//!
//! A change is a save under another key, with the old password held until the
//! write has answered so that a change the file did not take can be put back.
//! The snapshots the change leaves are known by what each file is rather than
//! by its slot, so a removal asked for after later saves still takes exactly
//! those.

use chrono::NaiveDateTime;
use keepass::db::Times;
use zeroize::Zeroizing;

use super::{Guard, Vault, read};
use crate::error::VaultError;
use crate::key::Former;
use crate::storage::watch::{Change, Content, Stamp};
use crate::storage::{snapshot, unsaved};

impl Vault {
    /// Gives the vault a new master password, and answers with how many
    /// snapshots beside it still open with the old one.
    ///
    /// `current` has to be the password the vault holds, compared by
    /// [`crate::MasterKey`] itself; the file is not asked. A key file the vault
    /// was opened with stays part of the key, and only the password beside it
    /// changes. Refused, with nothing written: a vault Coffer does not write
    /// back, the copy a lock left (made the vault first, it can be given one),
    /// a vault with such a copy beside it (made the vault or removed first), a
    /// wrong current password, an empty new one, one that is not text, and the
    /// one the vault already has.
    ///
    /// The new key is written the way every save writes: the file checked
    /// first, a snapshot taken, the database's own key derivation parameters
    /// with a fresh salt, and `MasterKeyChanged` set to now. A write that does
    /// not go through puts the old password and the old date back. A vault that
    /// kept a key its file was never written with would encrypt the next save
    /// under a password the reader was told did not take. The one exception is
    /// a write that failed after its rename had put the file in place: that
    /// file is under the new key, so the change stands (see
    /// `Vault::settle_failed_change`).
    pub fn change_master_password(
        &mut self,
        current: &[u8],
        new: Zeroizing<Vec<u8>>,
    ) -> Result<usize, VaultError> {
        self.writable()?;
        // Both asked before the password, so that neither is a place to try
        // one, and so that the reader is told before typing anything else.
        if unsaved::taken_from(&self.path).is_some() {
            return Err(VaultError::PasswordOfACopy);
        }
        // The copy opens with the password the vault has now. Changed beside
        // it, every sentence about the copy that says it opens with the
        // vault's password turns false, and making it the vault would quietly
        // bring the old password back.
        if unsaved::found(&self.path)?.is_some() {
            return Err(VaultError::PasswordBesideACopy);
        }
        if !self.key.password_is(current) {
            return Err(VaultError::NotTheCurrentPassword);
        }
        if new.is_empty() {
            return Err(VaultError::EmptyMasterPassword);
        }
        // A view of the bytes, not a copy. The key would refuse them anyway,
        // but only once the write had begun.
        std::str::from_utf8(&new).map_err(|_| VaultError::PasswordNotUtf8)?;
        if self.key.password_is(&new) {
            return Err(VaultError::SamePassword);
        }

        let former = self.key.replace_password(new);
        let dated = self.database.meta.master_key_changed.replace(Times::now());

        match self.write(Guard::Refuse) {
            Ok(()) => drop(former),
            // The guard found the file is not the one the vault agreed with,
            // before anything was written. It is somebody else's whatever key
            // opens it - another client may have given it this very password -
            // so it is the conflict's to settle and never this change's to
            // take for its own.
            Err(refused @ (VaultError::ExternalChange | VaultError::DatabaseGone)) => {
                self.put_back(former, dated);
                return Err(refused);
            }
            Err(failed) => self.settle_failed_change(failed, former, dated)?,
        }

        // Every slot that can be asked about is counted, one at a time. The
        // chain as a whole fails only for a name no suffix can be put on, which
        // a vault that opened does not have; a change that landed is still one.
        self.superseded = snapshot::Superseded::now(&self.path).unwrap_or_default();
        Ok(self.superseded.count())
    }

    /// Puts back the password and the date a change took, for a change the
    /// file did not take. The password the change gave is wiped.
    fn put_back(&mut self, former: Former, dated: Option<NaiveDateTime>) {
        self.key.restore(former);
        self.database.meta.master_key_changed = dated;
    }

    /// Decides what a change whose write answered with `failed`, after its
    /// guard had let it through, came to.
    ///
    /// A file at the vault's name that opens with the new key is the change's
    /// own, left by a write that failed after its rename: the change stands,
    /// and the vault records that file as the one it agrees with, so the next
    /// save is not refused as somebody else's write. Anything else is the
    /// failure it says it is, and the old key goes back. A refusal by the guard
    /// never comes here: see [`Vault::change_master_password`].
    fn settle_failed_change(
        &mut self,
        failed: VaultError,
        former: Former,
        dated: Option<NaiveDateTime>,
    ) -> Result<(), VaultError> {
        match self.written_anyway() {
            Some((stamp, content)) => {
                drop(former);
                self.stamp = stamp;
                self.content = content;
                self.changed = false;
                Ok(())
            }
            None => {
                self.put_back(former, dated);
                Err(failed)
            }
        }
    }

    /// Removes the snapshots that open with the password the vault had before
    /// its last change, wherever later saves have moved them, and answers with
    /// what that came to. Nothing else beside the vault is touched: not the
    /// vault, its lock, nor the copy a lock left.
    pub fn remove_old_snapshots(&mut self) -> snapshot::Removal {
        self.superseded.remove(&self.path)
    }

    /// The file at the vault's name, when it is one this vault wrote under the
    /// key it holds now, asked after a write that passed its guard and then
    /// answered with a failure.
    ///
    /// The guard is what tells another client's write apart, not the key: a
    /// file somebody else wrote may open with the new password as well, if they
    /// gave it the same one. Past the guard, the file was the one the vault
    /// agreed with a moment ago. Still that file, it is the old one, told
    /// without a key derivation. Anything else at the name is opened with the
    /// key, and a file that opens is this write's own, put in place before
    /// something after the rename failed - a directory that would not flush, a
    /// file that could not be read back. A file that does not open is somebody
    /// else's.
    fn written_anyway(&self) -> Option<(Stamp, Content)> {
        if matches!(self.external_change(), Ok(Change::None)) {
            return None;
        }
        let stamp = Stamp::of(&self.path).ok()?;
        let (_, content) = read(&self.path, &self.key).ok()?;
        Some((stamp, content))
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::Path;

    use keepass::config::KdfConfig;
    use keepass::{Database, DatabaseKey};

    use super::*;
    use crate::MasterKey;
    use crate::storage::{atomic, watch};
    use crate::vault::{LockPolicy, encrypt};

    const PASSWORD: &str = "built";
    const NEW: &str = "the new one";

    /// A date nobody would write today, so that the one a change sets cannot
    /// be mistaken for it.
    const LONG_AGO: NaiveDateTime = chrono::DateTime::UNIX_EPOCH.naive_utc();

    /// A database nobody has opened, with a key derivation that costs
    /// nothing.
    fn built(at: &Path) -> Result<(), VaultError> {
        let mut database = Database::new();
        database.config.kdf_config = KdfConfig::Aes { rounds: 16 };
        let mut file = std::fs::File::create(at)?;
        database.save(&mut file, DatabaseKey::new().with_password(PASSWORD))?;
        Ok(())
    }

    fn opened(directory: &Path) -> Result<Vault, VaultError> {
        let path = directory.join("vault.kdbx");
        built(&path)?;
        Vault::open(
            &path,
            MasterKey::from_password(Zeroizing::new(PASSWORD.as_bytes().to_vec())),
            LockPolicy::Respect,
        )
    }

    fn opens(path: &Path, password: &str) -> bool {
        std::fs::read(path).is_ok_and(|read| {
            Database::parse(&read, DatabaseKey::new().with_password(password)).is_ok()
        })
    }

    /// What a write under the new key leaves when everything after its rename
    /// fails: the file at the vault's name, encrypted under that key.
    fn written_under(vault: &Vault) -> Result<(), VaultError> {
        atomic::write_atomic::<VaultError, _>(&vault.path, |writer: &mut dyn Write| {
            encrypt(&vault.database, &vault.key, writer)?;
            Ok(())
        })
    }

    /// No fault from outside can land after the rename and before the write
    /// answers, so the file such a write leaves is put there by hand. It is the
    /// vault's own, under the key the vault holds now, and putting the old key
    /// back over it would leave a vault that writes under one password a file
    /// that opens with another.
    #[test]
    fn a_file_written_under_the_new_key_is_taken_as_the_vault_s_own_after_a_failed_write()
    -> Result<(), VaultError> {
        let directory = tempfile::tempdir()?;
        let mut vault = opened(directory.path())?;

        let _former = vault
            .key
            .replace_password(Zeroizing::new(NEW.as_bytes().to_vec()));
        written_under(&vault)?;

        assert_eq!(
            vault.written_anyway(),
            Some((
                Stamp::of(&vault.path)?,
                watch::digest(&std::fs::read(&vault.path)?)
            ))
        );
        Ok(())
    }

    /// A file at the name that does not open with the new key is not this
    /// write's, and the old key goes back for the conflict that follows.
    #[test]
    fn a_file_that_does_not_open_with_the_new_key_is_not_taken_for_it() -> Result<(), VaultError> {
        let directory = tempfile::tempdir()?;
        let mut vault = opened(directory.path())?;

        let theirs = directory.path().join("theirs.kdbx");
        built(&theirs)?;
        std::fs::copy(&theirs, &vault.path)?;
        let _former = vault
            .key
            .replace_password(Zeroizing::new(NEW.as_bytes().to_vec()));

        assert!(vault.written_anyway().is_none());
        Ok(())
    }

    /// The file the vault last agreed with is the old one, and is told without
    /// being opened: opened with the key the vault holds, which here is the key
    /// it is under, it would be taken for a write that landed.
    #[test]
    fn an_untouched_file_is_the_old_one_without_a_derivation() -> Result<(), VaultError> {
        let directory = tempfile::tempdir()?;
        let vault = opened(directory.path())?;

        assert!(vault.written_anyway().is_none());
        Ok(())
    }

    /// A vault the way a change leaves it once its write has answered with a
    /// failure: the new password in the key and the key dated now, with the
    /// old password and the old date held to be put back. It holds something
    /// the file had not got, as a vault being changed may.
    fn changing(directory: &Path) -> Result<(Vault, Former, Option<NaiveDateTime>), VaultError> {
        let mut vault = opened(directory)?;
        vault.database.meta.master_key_changed = Some(LONG_AGO);
        vault.touched();
        let former = vault
            .key
            .replace_password(Zeroizing::new(NEW.as_bytes().to_vec()));
        let dated = vault.database.meta.master_key_changed.replace(Times::now());
        Ok((vault, former, dated))
    }

    fn failed() -> VaultError {
        VaultError::Io(std::io::Error::other("the directory would not flush"))
    }

    /// A write that put its file in place and then failed - a directory that
    /// would not flush - leaves the change standing. The vault agrees with the
    /// file it wrote, so the next save goes through rather than being refused
    /// as somebody else's write, and that save is under the new key too: the
    /// file opens with the password the reader was told it has, and with no
    /// other.
    #[test]
    fn a_change_whose_file_landed_before_its_write_failed_stands() -> Result<(), VaultError> {
        let directory = tempfile::tempdir()?;
        let (mut vault, former, dated) = changing(directory.path())?;
        written_under(&vault)?;

        vault.settle_failed_change(failed(), former, dated)?;

        assert!(
            matches!(vault.external_change()?, Change::None),
            "the vault took its own file for somebody else's"
        );
        assert!(!vault.changed, "what the file holds was still owed to it");
        assert!(vault.key.password_is(NEW.as_bytes()));
        assert!(
            vault.database.meta.master_key_changed > Some(LONG_AGO),
            "the key's date went back with the change standing"
        );

        vault.save()?;
        assert!(opens(&vault.path, NEW));
        assert!(
            !opens(&vault.path, PASSWORD),
            "the save after a change that stood went back to the old password"
        );
        Ok(())
    }

    /// A write that failed with the vault's file untouched is the failure it
    /// says it is. The old password and its date go back, and the save after it
    /// writes the file under the password it already had.
    #[test]
    fn a_change_whose_file_did_not_land_puts_the_old_password_back() -> Result<(), VaultError> {
        let directory = tempfile::tempdir()?;
        let (mut vault, former, dated) = changing(directory.path())?;

        let refused = vault.settle_failed_change(failed(), former, dated);

        assert!(
            matches!(refused, Err(VaultError::Io(_))),
            "a change the file never took stood: {refused:?}"
        );
        assert!(vault.key.password_is(PASSWORD.as_bytes()));
        assert!(!vault.key.password_is(NEW.as_bytes()));
        assert_eq!(vault.database.meta.master_key_changed, Some(LONG_AGO));
        assert!(
            vault.changed,
            "what the file had not got was taken for written"
        );

        vault.save()?;
        assert!(opens(&vault.path, PASSWORD));
        assert!(
            !opens(&vault.path, NEW),
            "the save after a failed change wrote the new password"
        );
        Ok(())
    }
}
