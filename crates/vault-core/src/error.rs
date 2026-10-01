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
    #[error("this file is not a KDBX database")]
    NotADatabase,

    /// A KeePass format Coffer cannot read at all, such as the KeePass 2
    /// pre-release layout.
    #[error("this database is in a format Coffer cannot read")]
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
    #[error("KDB databases are read-only in Coffer")]
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

    /// The file is larger than any database anybody made. It is read into
    /// memory whole to be decrypted, so its own size is the ceiling on what
    /// opening it costs.
    #[error("this file is too large to be a KDBX database")]
    TooLarge,

    /// The entry asked for is not in this database.
    #[error("there is no such entry in this database")]
    NoSuchEntry,

    /// The group asked for is not in this database.
    #[error("there is no such folder in this database")]
    NoSuchGroup,

    /// The entry has no file of that name.
    #[error("that entry has no such file")]
    NoSuchAttachment,

    /// The entry has no version at that position.
    #[error("that entry has no such version")]
    NoSuchVersion,

    /// The file, or one whose number would have to change for it to go, is held
    /// by previous versions of an entry. The format keeps those inside the entry
    /// and the library cannot rewrite what one points at, so the versions have
    /// to go first.
    ///
    /// No count: the versions holding a file are not always the ones on the
    /// entry the reader is looking at, and a number that is sometimes about one
    /// entry and sometimes about two says less than a plain sentence.
    #[error("earlier versions of an entry still hold that file in place")]
    AttachmentInHistory,

    /// Removing the file would renumber another one that a previous version, or
    /// a second name, points at.
    #[error("that file cannot be removed without disturbing another entry")]
    AttachmentPinned,

    /// The files in the database no longer number from zero without a gap, so
    /// writing it would hand some of them to the wrong entries. Nothing in
    /// Coffer should be able to reach this; it is the last check before a write.
    #[error("the files in this database have lost their order")]
    AttachmentOrder,

    /// The root group holds everything else and is not something a user has.
    #[error("the top of the vault cannot be deleted or moved")]
    CannotMoveRoot,

    /// A folder cannot be put inside itself, or inside one of its own folders.
    #[error("a folder cannot be moved inside itself")]
    CannotMoveIntoItself,

    /// Only what is in the recycle bin can be put back out of it. The bin
    /// itself is not in the bin, and neither is anything already put back.
    #[error("that is not in the recycle bin")]
    NotInRecycleBin,

    /// What is open is one of Coffer's own snapshots. It opens like any other
    /// database and is not written back: the next save of the database it was
    /// taken from would rotate it away.
    /// The folder the database sits in will not take a file: a read-only disk
    /// image, a Time Machine snapshot, a stick macOS mounted read-only, a share
    /// the reader may only read. Coffer opens the database anyway, because
    /// reading one needs no write, and answers every change with this.
    #[error("this folder is read only, so nothing can be written to it")]
    ReadOnlyPlace,

    #[error("a snapshot is opened to read, not to write")]
    ReadOnlySnapshot,

    /// The entry has no field of that name.
    #[error("that entry has no such field")]
    NoSuchField,

    /// Part of a value was asked for that the value does not have: an empty
    /// or backwards range, an end past the last character, or an end between
    /// the two halves of a character outside the basic plane.
    #[error("that part of the value is not there")]
    NoSuchPart,

    /// The file offered is larger than Coffer will put into a database.
    #[error("that file is too large to keep in a vault")]
    AttachmentTooLarge,

    /// The database is a KDBX 3 file, whose attachments the reader Coffer is
    /// built on collapses onto one. The names are the file's; the bytes behind
    /// them are not, so they are not handed out.
    #[error("the files in a KDBX 3 database cannot be read reliably")]
    UnreadableAttachments,

    /// A copy has to go somewhere other than the database it is a copy of.
    #[error("a copy cannot be written over the database it came from")]
    CopyOntoItself,

    /// A password with no characters to choose from is not a password.
    #[error("a password needs at least one kind of character")]
    NothingToGenerateFrom,

    /// The operating system would not give us randomness, so there is no
    /// password to hand back. Never quietly substituted with anything weaker.
    #[error("this Mac would not provide the randomness a password needs")]
    RandomnessUnavailable,

    /// A field name or value holds a character XML cannot carry, so writing it
    /// would produce a file no KeePass client can read.
    #[error("that value contains a character a KDBX file cannot hold")]
    UnwritableText,

    /// A vault is being made with no master password at all. The library would
    /// write one happily; a file anybody can open is not what the reader asked
    /// for. Opening a database that already has one is untouched.
    #[error("a new vault needs a master password")]
    EmptyMasterPassword,

    /// There is already a file where the new vault would go. Nothing is written
    /// over: a creation takes no snapshot, so what was there would be gone.
    #[error("there is already a file with that name")]
    DatabaseExists,

    /// The new vault would take a name Coffer gives a file of its own beside a
    /// database: a snapshot, which every save is refused for, or the copy a
    /// lock leaves when it could not save, which the next such lock overwrites.
    #[error("that name belongs to a file Coffer keeps beside a vault")]
    ReservedName,

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
    ///
    /// The message names who, because the reader has to decide whether that is
    /// still true: a lock left behind by a Mac that lost power looks exactly
    /// like one held by a Coffer running right now, and the only thing that
    /// tells them apart is a person recognising the machine and the hour. A
    /// `Holder` carries a time, an account, a machine and a process id, and
    /// none of those is a secret out of the database.
    #[error("{}", .0.describe())]
    Locked(Holder),

    /// Anything the filesystem said no to.
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl From<keepass::error::DatabaseOpenError> for VaultError {
    fn from(error: keepass::error::DatabaseOpenError) -> Self {
        use keepass::db::{DatabaseFormatError, DatabaseOpenError};
        use keepass::error::DatabaseVersionParseError;

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
            // Every key error is one message: telling the user which half of a
            // password-and-key-file pair failed tells an attacker the same
            // thing.
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
            // The one failure that is about the machine rather than the file.
            // Without this arm it lands in the catch-all below and the reader
            // is told the database could not be written, which says nothing.
            DatabaseSaveError::Random(_) => VaultError::RandomnessUnavailable,
            _ => VaultError::Io(io::Error::other("the database could not be written")),
        }
    }
}
