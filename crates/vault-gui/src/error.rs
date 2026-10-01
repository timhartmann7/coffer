//! What a command says when it fails.
//!
//! Every message here comes from `vault-core`, which writes them so that they
//! never repeat a secret, never name which half of a credential was wrong, and
//! never quote the document a parser choked on. The code beside the message is
//! what the screen branches on, so that the wording can change without the
//! screen noticing.

use serde::Serialize;
use vault_core::VaultError;

/// `Debug` is derived on purpose: every message here is written to be shown to
/// the user and to carry no part of the database, so a failure is one of the few
/// things in Coffer that is safe to print.
#[derive(Serialize, Debug)]
pub struct Failure {
    code: Code,
    message: String,
}

#[derive(Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
enum Code {
    /// The password, the key file, or both. One code, one message, no detail.
    WrongCredentials,
    NotADatabase,
    UnsupportedFormat,
    /// The header, the body or the XML inside it. The offer the screen makes is
    /// the same for all three: open the most recent snapshot instead.
    Damaged,
    /// Another process has this database open.
    HeldByAnother,
    /// The file changed on disk after Coffer opened it, and the screen has to
    /// ask which version to keep before anything is written. Also the vault's
    /// file changing after the copy's banner said how it stood: the banner
    /// reads it again before the copy goes over it.
    ExternalChange,
    /// The database cannot be written back at all: a snapshot, or a format
    /// Coffer reads and does not write.
    ReadOnly,
    Gone,
    TooLarge,
    /// Nothing has been chosen to open, or nothing is open.
    NoVault,
    NoSuchEntry,
    /// Coffer will not do this: an address in a scheme it does not open, or a
    /// value it cannot write.
    Refused,
    /// The file cannot go while previous versions of the entry still hold it.
    /// A code of its own, because the screen has something to offer here that
    /// it has nowhere else: clearing those versions.
    AttachmentInHistory,
    /// Something already sits where a new vault would go. A code of its own
    /// for the same reason: the screen offers to open that file instead, and
    /// keeps the passwords the reader typed for somewhere else.
    Taken,
    /// A previous version was named by a position read before the vault last
    /// changed, where it may name another version now. Nothing was done, and
    /// the screen reads the list again rather than showing a failure.
    VersionsChanged,
    /// Removing the field would be for good: the vault keeps no version to
    /// bring it back from. Nothing was done, and the screen asks the reader
    /// whether they mean it.
    ForGood,
    /// A removal the reader asked to take back is no longer the last thing
    /// that happened to its entry. Nothing was done, and the screen says the
    /// removal can no longer be undone.
    Superseded,
    /// The copy a lock left cannot be put back without a password on this
    /// disk. Nothing was done, and the screen offers to open the copy, which
    /// is the way it can still become the vault.
    NeedsOpening,
    Io,
    Other,
}

impl Failure {
    /// Coffer has no database open, or none chosen.
    pub fn no_vault() -> Failure {
        Failure {
            code: Code::NoVault,
            message: "no database is open".to_owned(),
        }
    }

    pub fn no_such_entry() -> Failure {
        Failure::from(VaultError::NoSuchEntry)
    }

    pub fn no_such_group() -> Failure {
        Failure::from(VaultError::NoSuchGroup)
    }

    /// An address Coffer will not hand to the system, or anything else it
    /// declines to do with a value it was given.
    pub fn refused(message: &str) -> Failure {
        Failure {
            code: Code::Refused,
            message: message.to_owned(),
        }
    }

    /// Two databases at once is not something Coffer does, so every offer to
    /// open another file is refused while a vault is open. One sentence, three
    /// places that make the offer.
    pub fn lock_first() -> Failure {
        Failure::refused("lock the vault before opening another")
    }

    /// A new vault was asked for before anywhere had been settled for it. One
    /// sentence, because making one and opening what is at the place both ask.
    pub fn nowhere_chosen() -> Failure {
        Failure::refused("nowhere has been chosen for the new vault")
    }

    /// An unlock finished after the session had been pointed somewhere else.
    /// Nothing is open, which is what the code says; the message says why.
    pub fn stale() -> Failure {
        Failure {
            code: Code::NoVault,
            message: "the database changed while it was opening".to_owned(),
        }
    }

    /// A version named by a position from a list the vault has changed since.
    /// See [`crate::session::Session::at`].
    pub fn versions_changed() -> Failure {
        Failure {
            code: Code::VersionsChanged,
            message: "the versions changed after they were listed".to_owned(),
        }
    }

    /// The database file is not where it was.
    pub fn gone() -> Failure {
        Failure::from(VaultError::DatabaseGone)
    }

    /// Something inside Coffer went wrong in a way the screen cannot act on.
    pub fn internal(message: &str) -> Failure {
        Failure {
            code: Code::Other,
            message: message.to_owned(),
        }
    }

    pub fn io(error: std::io::Error) -> Failure {
        Failure::from(VaultError::Io(error))
    }
}

impl From<VaultError> for Failure {
    fn from(error: VaultError) -> Failure {
        let code = match error {
            VaultError::WrongCredentials => Code::WrongCredentials,
            VaultError::NotADatabase => Code::NotADatabase,
            VaultError::UnsupportedFormat => Code::UnsupportedFormat,
            VaultError::DamagedHeader | VaultError::DamagedPayload | VaultError::DamagedContent => {
                Code::Damaged
            }
            VaultError::Locked(_) => Code::HeldByAnother,
            VaultError::AttachmentInHistory => Code::AttachmentInHistory,
            VaultError::DatabaseExists => Code::Taken,
            VaultError::ExternalChange | VaultError::VaultFileChanged => Code::ExternalChange,
            VaultError::NoExclusiveMove => Code::NeedsOpening,
            VaultError::ReadOnlyKdb
            | VaultError::ReadOnlyKdbx3Attachments
            | VaultError::ReadOnlySnapshot
            | VaultError::ReadOnlyPlace => Code::ReadOnly,
            VaultError::DatabaseGone => Code::Gone,
            VaultError::TooLarge => Code::TooLarge,
            VaultError::NoSuchEntry
            | VaultError::NoSuchGroup
            | VaultError::NoSuchField
            | VaultError::NoSuchAttachment
            | VaultError::NoSuchVersion => Code::NoSuchEntry,
            VaultError::UnwritableText
            | VaultError::EmptyMasterPassword
            | VaultError::ReservedName
            | VaultError::PasswordNotUtf8
            | VaultError::AbsurdKeyDerivation
            | VaultError::AttachmentPinned
            | VaultError::AttachmentTooLarge
            | VaultError::AttachmentOrder
            | VaultError::UnreadableAttachments
            | VaultError::CannotMoveRoot
            | VaultError::CannotMoveIntoItself
            | VaultError::NotInRecycleBin
            | VaultError::NoSuchPart
            | VaultError::CopyOntoItself
            | VaultError::NotACopy
            | VaultError::NothingToGenerateFrom
            | VaultError::RandomnessUnavailable => Code::Refused,
            VaultError::RemovalForGood => Code::ForGood,
            VaultError::RemovalSuperseded => Code::Superseded,
            VaultError::Io(_) => Code::Io,
            _ => Code::Other,
        };

        Failure {
            code,
            message: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code_of(error: VaultError) -> String {
        let payload = serde_json::to_value(Failure::from(error)).expect("a failure serialises");
        payload["code"]
            .as_str()
            .expect("a code is a string")
            .to_owned()
    }

    /// The screen decides what to offer from the code alone: a snapshot for a
    /// file that will not open, a red field for a password that did not fit.
    /// Nothing else pins these strings, and moving one is silent.
    #[test]
    fn every_failure_keeps_the_code_the_screen_branches_on() {
        use std::io;

        for (error, expected) in [
            (VaultError::WrongCredentials, "wrongCredentials"),
            (VaultError::NotADatabase, "notADatabase"),
            (VaultError::UnsupportedFormat, "unsupportedFormat"),
            (VaultError::DamagedHeader, "damaged"),
            (VaultError::DamagedPayload, "damaged"),
            (VaultError::DamagedContent, "damaged"),
            (VaultError::DatabaseGone, "gone"),
            (VaultError::TooLarge, "tooLarge"),
            (VaultError::NoSuchEntry, "noSuchEntry"),
            (VaultError::UnwritableText, "refused"),
            (VaultError::EmptyMasterPassword, "refused"),
            (VaultError::DatabaseExists, "taken"),
            (VaultError::ReservedName, "refused"),
            (VaultError::RandomnessUnavailable, "refused"),
            (VaultError::PasswordNotUtf8, "refused"),
            (VaultError::AbsurdKeyDerivation, "refused"),
            (VaultError::ReadOnlyKdb, "readOnly"),
            (VaultError::ReadOnlyKdbx3Attachments, "readOnly"),
            (VaultError::ReadOnlySnapshot, "readOnly"),
            (VaultError::ReadOnlyPlace, "readOnly"),
            (VaultError::ExternalChange, "externalChange"),
            (VaultError::VaultFileChanged, "externalChange"),
            (VaultError::NoExclusiveMove, "needsOpening"),
            (VaultError::NoSuchGroup, "noSuchEntry"),
            (VaultError::NoSuchVersion, "noSuchEntry"),
            (VaultError::AttachmentInHistory, "attachmentInHistory"),
            (VaultError::AttachmentPinned, "refused"),
            (VaultError::AttachmentTooLarge, "refused"),
            (VaultError::CannotMoveRoot, "refused"),
            (VaultError::NotInRecycleBin, "refused"),
            (VaultError::NoSuchPart, "refused"),
            (VaultError::NotACopy, "refused"),
            (VaultError::NothingToGenerateFrom, "refused"),
            (VaultError::RemovalForGood, "forGood"),
            (VaultError::RemovalSuperseded, "superseded"),
            (VaultError::Io(io::Error::other("a disk")), "io"),
        ] {
            let message = error.to_string();
            assert_eq!(code_of(error), expected, "{message}");
        }
    }

    /// Every message the screen shows comes from `vault-core`, and none of them
    /// repeats what was in the database.
    #[test]
    fn a_failure_says_what_broke_and_nothing_about_the_contents() {
        let payload = serde_json::to_string(&Failure::from(VaultError::WrongCredentials))
            .expect("a failure serialises");
        assert_eq!(
            payload,
            r#"{"code":"wrongCredentials","message":"wrong password or key file"}"#
        );

        let stale = serde_json::to_string(&Failure::stale()).expect("a failure serialises");
        assert!(stale.contains(r#""code":"noVault""#), "{stale}");

        let moved =
            serde_json::to_string(&Failure::versions_changed()).expect("a failure serialises");
        assert!(moved.contains(r#""code":"versionsChanged""#), "{moved}");
    }
}
