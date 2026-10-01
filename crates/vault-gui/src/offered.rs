//! A file the reader chose for an entry under a name the entry already gives
//! another, waiting on their word about which of the two to keep.
//!
//! Picking the file and answering the question are two messages, and a great
//! deal can happen between them: another file picked, the entry deleted, the
//! vault read again or locked. What is here is the rule for each of those, so
//! that a file only ever goes on the entry it was chosen for, and only on the
//! reader's answer about it.

use vault_core::model::EntryId;
use vault_core::{Attached, Vault, VaultError};
use zeroize::Zeroizing;

use crate::error::Failure;

/// A file offered to an entry, held until the reader answers.
///
/// The bytes rather than the path they were read from. A path is a question
/// asked again later of a disk that has moved on: the file can be renamed,
/// rewritten by the scanner that made it, or on a stick that has been pulled
/// out by the time the reader answers, and what went on the entry would not be
/// what they were asked about, or would be nothing at all. The bytes are the
/// file they chose, as it was when they chose it. They cost what the offer had
/// already paid to read them - no more than the largest file a vault takes -
/// and they are held the way the vault holds everything else: wiped when they
/// go, and gone whenever the vault is.
///
/// Tied to the entry the file was picked for. An answer about any other entry
/// does not reach it, so a file is never put on an entry it was not chosen for.
struct Offered {
    entry: EntryId,
    name: String,
    data: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for Offered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Offered")
            .field("entry", &self.entry)
            .field("file", &"[redacted]")
            .finish()
    }
}

/// The one file waiting on an answer, if any is. Held beside the vault it is
/// for, and dropped with it.
#[derive(Default)]
pub struct Waiting {
    offered: Option<Offered>,
}

impl Waiting {
    /// Offers a file to an entry, and holds on to it when the entry already
    /// gives its name to another.
    ///
    /// Whatever was waiting before is let go first, whichever entry it was for:
    /// a file picked is a new question, and the last one is no longer being
    /// asked.
    pub fn offer(
        &mut self,
        vault: &mut Vault,
        entry: EntryId,
        name: String,
        data: Zeroizing<Vec<u8>>,
    ) -> Result<Attached, VaultError> {
        self.offered = None;

        let attached = vault.add_attachment(entry, &name, &data)?;
        if matches!(attached, Attached::Taken(_)) {
            self.offered = Some(Offered { entry, name, data });
        }
        Ok(attached)
    }

    /// Puts the file waiting on `entry` where the reader said: `answer` is
    /// [`Vault::keep_both`] or [`Vault::replace_attachment`].
    ///
    /// Only the file picked for this entry answers to it. One waiting on any
    /// other entry is refused and left where it is, and none waiting at all -
    /// the reader said no, or picked again - is refused as well, so an answer
    /// that arrives late can never put anything anywhere.
    ///
    /// The file is let go once the answer is in, and kept when the answer is
    /// refused: a replacement the versions stand in the way of leaves keeping
    /// both still open to the reader, without choosing the file again.
    pub fn answer(
        &mut self,
        vault: &mut Vault,
        entry: EntryId,
        answer: impl FnOnce(&mut Vault, EntryId, &str, &[u8]) -> Result<(), VaultError>,
    ) -> Result<(), Failure> {
        let offered = self
            .offered
            .as_ref()
            .filter(|offered| offered.entry == entry)
            .ok_or_else(|| Failure::refused("no file is waiting to go on that entry"))?;

        answer(vault, entry, &offered.name, &offered.data)?;
        self.offered = None;
        Ok(())
    }

    /// Lets go of the file waiting on `entry`, when there is one.
    ///
    /// Only that entry's. The window says this about the entry it is leaving,
    /// and the message can arrive after a file has been picked for the next
    /// one, which is not the window's to throw away.
    pub fn withdraw(&mut self, entry: EntryId) {
        if self
            .offered
            .as_ref()
            .is_some_and(|offered| offered.entry == entry)
        {
            self.offered = None;
        }
    }

    /// Lets go of whatever is waiting. The file read again keeps an entry's id
    /// and may not keep the file the reader was asked about.
    pub fn clear(&mut self) {
        self.offered = None;
    }

    /// Lets go of the file waiting on an entry the reader can no longer be
    /// looking at: one deleted, emptied out of the bin, or in the bin, where
    /// nothing goes on an entry. Called under the lock of the change that took
    /// the entry away, so that no answer can land between the two.
    pub fn settle(&mut self, vault: &Vault) {
        let gone = self.offered.as_ref().is_some_and(|offered| {
            vault
                .entry(offered.entry)
                .is_none_or(|entry| entry.binned.is_some())
        });
        if gone {
            self.offered = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file waiting is the reader's file, and printing it prints neither
    /// its bytes nor its name.
    #[test]
    fn the_file_waiting_prints_nothing_of_itself() {
        let offered = Offered {
            entry: EntryId::from_uuid(uuid::Uuid::nil()),
            name: "passport scan.pdf".to_owned(),
            data: Zeroizing::new(b"-----BEGIN OPENSSH PRIVATE KEY-----".to_vec()),
        };
        let printed = format!("{offered:?}");
        assert!(printed.contains("[redacted]"), "{printed}");
        assert!(!printed.contains("passport"), "{printed}");
        assert!(!printed.contains("OPENSSH"), "{printed}");
    }
}
