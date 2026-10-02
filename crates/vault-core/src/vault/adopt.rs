//! Making a file Coffer keeps beside a vault the vault: a backup, and the copy
//! a lock left, which share the one write that does it.
//!
//! A choice of one whole file over another, never a merge (spec section 3).
//! What is open goes over the vault's file the way a save goes over one
//! somebody else wrote, and what was there is kept: in the snapshots, like
//! everything a save replaces, and for a backup under a name of its own as
//! well when nothing in Coffer could open it again.

use std::path::{Path, PathBuf};

use super::{Guard, ReadOnly, Vault, classify, read};
use crate::error::VaultError;
use crate::storage::lock::{Lock, claim};
use crate::storage::{self, OnDisk, Seen, aside, snapshot};

/// What became of the vault's file when a backup went in its place.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Adopted {
    /// It opened with this vault's password, which makes it an older or newer
    /// state of the same vault. It is the newest snapshot, at this path, until
    /// later saves push it out like any other.
    Kept(PathBuf),
    /// It would not open with this vault's password - damaged, under another
    /// one, not a vault at all - and it is kept beside the vault at this path,
    /// which no save touches and nothing in Coffer removes. It is the newest
    /// snapshot as well, for as long as that lasts.
    SetAside(PathBuf),
    /// Nothing was at the vault's name, and the backup took it.
    Empty,
}

impl Vault {
    /// Makes this vault, opened from one of the snapshots Coffer keeps beside a
    /// vault, the vault it was taken beside.
    ///
    /// No password is asked for: the key is the one the backup opened with, so
    /// from then on the vault opens with the password it had when the backup
    /// was taken, which need not be the one it has now. What is in the backup
    /// is written over the vault's file by the write [`Vault::promote`] makes,
    /// so the file as it stands is the newest snapshot afterwards, whatever it
    /// holds. A file that will not open with this key is first given a name of
    /// its own beside the vault, `<vault>.replaced-<day>.kdbx`, numbered past
    /// any name already taken (see [`aside::keep`]): in the snapshots alone it
    /// would be gone after ten saves, and Coffer could never have said what it
    /// held. A vault whose file has gone takes the backup at its name.
    ///
    /// `seen` is how the vault's file stood when the reader was told, and the
    /// press is held to it exactly as [`Vault::promote`] is: refused with
    /// [`VaultError::VaultFileChanged`] and nothing touched when it stands
    /// otherwise, asked once the vault's lock is held.
    ///
    /// A press that does not go through leaves the vault's file and its other
    /// names as they were, and this vault still the backup, still read only.
    /// A write that failed after the snapshots moved on has moved the backup a
    /// slot on with them, and this vault follows it there; one the move pushed
    /// out of the chain stays open from memory under the name it had.
    ///
    /// A backup of the copy a lock left is refused with
    /// [`VaultError::NotASnapshot`]: it would go over that copy, which holds
    /// the only version of work its vault has not got, and which nothing but
    /// its own saves writes over.
    pub fn adopt(&mut self, seen: Option<Seen>) -> Result<Adopted, VaultError> {
        let vault = snapshot::taken_from(&self.path)
            .filter(|vault| !storage::reserved(vault))
            .ok_or(VaultError::NotASnapshot)?;
        // A file at a snapshot's name in a format Coffer does not write is
        // refused for its format, which is what the reader has to hear.
        if let Some(why) = self.source.filter(|why| *why != ReadOnly::Snapshot) {
            return Err(why.into());
        }
        let lock = claim(&vault)?;
        let Some(seen) = seen.filter(|seen| *seen == Seen::of(&vault)) else {
            return Err(VaultError::VaultFileChanged);
        };

        let newest = snapshot::slot(&vault, 1)?;
        let own = snapshot::taken(&vault)?
            .into_iter()
            .find(|each| each.path == self.path);

        // Opening the file is a key derivation, and only a file that is there
        // is worth one. The tree it opens to is wiped as it goes.
        let kept = match seen.on_disk {
            OnDisk::Gone => None,
            OnDisk::Written(_) if read(&vault, &self.key).is_ok() => None,
            OnDisk::Written(_) => Some(
                aside::keep(&vault, chrono::Local::now().date_naive()).map_err(|error| {
                    // A file that went or changed between the look and the
                    // second name no longer stands as the reader was told.
                    // Anything else is the disk's own answer.
                    if Seen::of(&vault) == seen {
                        VaultError::Io(error)
                    } else {
                        VaultError::VaultFileChanged
                    }
                })?,
            ),
        };

        if let Err(error) = self.take_over(vault.clone(), lock) {
            self.settle_failed_adoption(&vault, kept.as_deref(), own.as_ref());
            return Err(error);
        }

        Ok(match (kept, seen.on_disk) {
            (Some(kept), _) => Adopted::SetAside(kept),
            (None, OnDisk::Gone) => Adopted::Empty,
            (None, OnDisk::Written(_)) => Adopted::Kept(newest),
        })
    }

    /// Puts back what a press whose write did not go through can put back.
    ///
    /// The second name the vault's file was just given goes again, while that
    /// file is still at the vault's name (see [`aside::withdraw`]). A write
    /// that failed after the snapshots moved on has moved this backup a slot
    /// on with them, and this vault follows `own` - the backup as it was
    /// listed before the write - to wherever it is now, so that the next
    /// unlock opens the backup the strip described and not the neighbour that
    /// took its slot. One the move pushed out of the chain is left open from
    /// memory under the name it had.
    fn settle_failed_adoption(
        &mut self,
        vault: &Path,
        kept: Option<&Path>,
        own: Option<&snapshot::Taken>,
    ) {
        if let Some(kept) = kept {
            aside::withdraw(vault, kept);
        }
        if let Some(moved) = own.and_then(|own| snapshot::find(vault, own).ok().flatten()) {
            self.path = moved.path;
        }
    }

    /// Points this vault at `vault`'s name, with the lock beside it, and writes
    /// it there, answering with the name it leaves. For what was opened from a
    /// file Coffer keeps beside that vault, becoming it.
    ///
    /// The write is the ordinary one aimed at the vault's name, so that it
    /// proves the place will take it, snapshots what is there and records what
    /// it leaves exactly as every save does, over whatever is there by then
    /// (the callers have held the reader to how it stood). What is open is
    /// judged for the new name before the write, because a snapshot's own name
    /// refuses every write. All of it is put back if the write does not go
    /// through, and the lock with the vault's name goes.
    pub(super) fn take_over(&mut self, vault: PathBuf, lock: Lock) -> Result<PathBuf, VaultError> {
        let left = std::mem::replace(&mut self.path, vault);
        let agreed = (self.stamp, self.content, self.source);
        self.source = classify(&self.database, &self.path, Some(&lock));

        if let Err(error) = self.write(Guard::Ignore) {
            self.path = left;
            (self.stamp, self.content, self.source) = agreed;
            return Err(error);
        }

        self._lock = Some(lock);
        Ok(left)
    }
}

#[cfg(test)]
mod tests {
    use keepass::config::KdfConfig;
    use keepass::{Database, DatabaseKey};
    use zeroize::Zeroizing;

    use super::*;
    use crate::MasterKey;
    use crate::vault::LockPolicy;

    const PASSWORD: &str = "built";

    fn key() -> MasterKey {
        MasterKey::from_password(Zeroizing::new(PASSWORD.as_bytes().to_vec()))
    }

    /// A vault saved three times, with a key derivation that costs nothing,
    /// and the path its snapshots are beside.
    fn saved_three_times(directory: &Path) -> Result<PathBuf, VaultError> {
        let path = directory.join("vault.kdbx");
        let mut database = Database::new();
        database.config.kdf_config = KdfConfig::Aes { rounds: 16 };
        let mut file = std::fs::File::create(&path)?;
        database.save(&mut file, DatabaseKey::new().with_password(PASSWORD))?;

        let mut vault = Vault::open(&path, key(), LockPolicy::Respect)?;
        for _ in 0..3 {
            vault.save()?;
        }
        Ok(vault.path().to_path_buf())
    }

    /// No fault from outside lands between the write's rotation and its
    /// rename, so the rotation is made by hand and the failure handed over.
    /// The backup the strip described is two slots down by then: the vault
    /// follows it there, with its bytes, and the name the vault's file was
    /// given goes again while that file is still the vault's.
    #[test]
    fn a_backup_whose_write_failed_after_the_chain_moved_is_still_the_one_open()
    -> Result<(), VaultError> {
        let directory = tempfile::tempdir()?;
        let vault = saved_three_times(directory.path())?;
        let shown = snapshot::slot(&vault, 2)?;
        let bytes = std::fs::read(&shown)?;
        let mut backup = Vault::open(&shown, key(), LockPolicy::Respect)?;
        let own = snapshot::taken(&vault)?
            .into_iter()
            .find(|each| each.path == backup.path);
        let kept = aside::keep(&vault, chrono::Local::now().date_naive())?;

        snapshot::rotate(&vault)?;
        backup.settle_failed_adoption(&vault, Some(&kept), own.as_ref());

        assert_eq!(backup.path(), snapshot::slot(&vault, 3)?);
        assert_eq!(std::fs::read(backup.path())?, bytes);
        assert_ne!(
            std::fs::read(&shown)?,
            bytes,
            "the slot the backup was shown at still holds it, so this proves nothing"
        );
        assert!(
            !kept.exists(),
            "the second name outlived a press that did not go through"
        );
        assert_eq!(backup.read_only(), Some(ReadOnly::Snapshot));
        Ok(())
    }
}
