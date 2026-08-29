//! What can go wrong, said in a way that never repeats a secret.
//!
//! None of these variants carry the underlying library's message. A parser
//! error from the XML inside a database quotes the document it failed on, and
//! that document is the user's passwords. Coffer reports what kind of thing
//! broke and nothing more, and accepts the loss of diagnostic detail as the
//! price of the rule.

use std::io;

use crate::storage::lock::Holder;

/// Everything a vault operation can fail with.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VaultError {
    /// The file does not start with the KeePass identifier.
    #[error("this file is not a KeePass database")]
    NotADatabase,

    /// A KeePass format Coffer cannot read at all, such as the KeePass 2
    /// pre-release layout.
    #[error("this database is in a KeePass format Coffer cannot read")]
    UnsupportedFormat,

    /// The password, the key file, or both are wrong. Deliberately one message
    /// with no detail: telling the user which half failed tells an attacker the
    /// same thing.
    #[error("wrong password or key file")]
    WrongCredentials,

    /// The outer header failed its own hash, so the file was truncated or
    /// altered before key derivation could even be attempted.
    #[error("the database header is damaged")]
    DamagedHeader,

    /// The encrypted body did not decrypt or decompress.
    #[error("the database body is damaged")]
    DamagedPayload,

    /// The body decrypted but the XML inside it did not parse.
    #[error("the contents of the database are damaged")]
    DamagedContent,

    /// A KeePass 1 database. Coffer reads these and never writes one, because
    /// the format cannot hold what Coffer would have to put back.
    #[error("KeePass 1 databases are read-only in Coffer")]
    ReadOnlyKdb,

    /// A KDBX 3 database holding attachments. The reader Coffer is built on
    /// collapses every attachment in a KDBX 3 database onto one, so saving it
    /// would write back fewer attachments than were read. Coffer refuses rather
    /// than lose them.
    #[error("KDBX 3 databases with attachments are read-only in Coffer")]
    ReadOnlyKdbx3Attachments,

    /// The file's key derivation parameters ask for more work or more memory
    /// than any real database needs. They sit outside everything the header
    /// signature covers, so a hostile file can put anything there and the
    /// computation happens before the file has proved it is genuine.
    #[error("the database asks for key derivation Coffer will not perform")]
    AbsurdKeyDerivation,

    /// A field name or value holds a character XML cannot carry, so writing it
    /// would produce a file no KeePass client can read.
    #[error("that value contains a character a KeePass file cannot hold")]
    UnwritableText,

    /// The master password bytes are not UTF-8, and KeePass hashes passwords as
    /// UTF-8 text.
    #[error("the master password is not valid text")]
    PasswordNotUtf8,

    /// Somebody wrote the database file after Coffer opened it. Overwriting it
    /// would discard their work, so the caller has to decide first.
    #[error("the database changed on disk after Coffer opened it")]
    ExternalChange,

    /// The database file is not where it was.
    #[error("the database file is gone")]
    DatabaseGone,

    /// Another process has the database open.
    #[error("the database is open in another process")]
    Locked(Holder),

    /// Anything the filesystem said no to.
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl From<keepass::error::DatabaseOpenError> for VaultError {
    fn from(error: keepass::error::DatabaseOpenError) -> Self {
        use keepass::db::{DatabaseFormatError, DatabaseOpenError};
        use keepass::error::{DatabaseKeyError, DatabaseVersionParseError};

        match error {
            DatabaseOpenError::Io(error) => VaultError::Io(error),
            DatabaseOpenError::UnexpectedEof => VaultError::DamagedHeader,
            DatabaseOpenError::VersionParse(DatabaseVersionParseError::InvalidKDBXIdentifier) => {
                VaultError::NotADatabase
            }
            DatabaseOpenError::VersionParse(DatabaseVersionParseError::UnexpectedEof) => {
                VaultError::NotADatabase
            }
            DatabaseOpenError::VersionParse(_) => VaultError::UnsupportedFormat,
            DatabaseOpenError::UnsupportedVersion => VaultError::UnsupportedFormat,
            DatabaseOpenError::Key(DatabaseKeyError::IncorrectKey) => VaultError::WrongCredentials,
            DatabaseOpenError::Key(_) => VaultError::WrongCredentials,
            DatabaseOpenError::Cryptography(_) => VaultError::DamagedPayload,
            DatabaseOpenError::Format(format) => match format {
                DatabaseFormatError::Kdb(_) => VaultError::DamagedContent,
                DatabaseFormatError::Kdbx3(_) => VaultError::DamagedContent,
                DatabaseFormatError::Kdbx4(error) => kdbx4_damage(&error),
                _ => VaultError::DamagedContent,
            },
            _ => VaultError::DamagedContent,
        }
    }
}

fn kdbx4_damage(error: &keepass::error::Kdbx4OpenError) -> VaultError {
    use keepass::error::Kdbx4OpenError;

    match error {
        Kdbx4OpenError::OuterHeader(_) | Kdbx4OpenError::HeaderHashMismatch => {
            VaultError::DamagedHeader
        }
        Kdbx4OpenError::BlockStream(_) | Kdbx4OpenError::InnerHeader(_) => {
            VaultError::DamagedPayload
        }
        _ => VaultError::DamagedContent,
    }
}

impl From<keepass::error::DatabaseKeyError> for VaultError {
    fn from(_: keepass::error::DatabaseKeyError) -> Self {
        VaultError::WrongCredentials
    }
}

impl From<keepass::db::DatabaseSaveError> for VaultError {
    fn from(error: keepass::db::DatabaseSaveError) -> Self {
        use keepass::db::DatabaseSaveError;

        match error {
            DatabaseSaveError::Io(error) => VaultError::Io(error),
            DatabaseSaveError::Key(_) => VaultError::WrongCredentials,
            _ => VaultError::Io(io::Error::other("the database could not be written")),
        }
    }
}
