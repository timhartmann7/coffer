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
    Gone,
    TooLarge,
    /// Nothing has been chosen to open, or nothing is open.
    NoVault,
    NoSuchEntry,
    /// Coffer will not do this: an address in a scheme it does not open, or a
    /// value it cannot write.
    Refused,
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

    /// An address Coffer will not hand to the system, or anything else it
    /// declines to do with a value it was given.
    pub fn refused(message: &str) -> Failure {
        Failure {
            code: Code::Refused,
            message: message.to_owned(),
        }
    }

    /// An unlock finished after the session had been pointed somewhere else.
    /// Nothing is open, which is what the code says; the message says why.
    pub fn stale() -> Failure {
        Failure {
            code: Code::NoVault,
            message: "the database changed while it was opening".to_owned(),
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
            VaultError::DatabaseGone => Code::Gone,
            VaultError::TooLarge => Code::TooLarge,
            VaultError::NoSuchEntry => Code::NoSuchEntry,
            VaultError::UnwritableText
            | VaultError::PasswordNotUtf8
            | VaultError::AbsurdKeyDerivation => Code::Refused,
            VaultError::Io(_) => Code::Io,
            _ => Code::Other,
        };

        Failure {
            code,
            message: error.to_string(),
        }
    }
}
