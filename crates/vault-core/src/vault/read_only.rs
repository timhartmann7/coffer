//! Why a vault Coffer opened cannot be written back, when it cannot.
//!
//! Four reasons, and the reader is told which, because each has its own way
//! on: a backup is made the vault or left, a place is left for one that takes a
//! file, and a format is not Coffer's to write at all. Only the last of those
//! is about the data rather than about where it is, so it is the one that
//! refuses a copy elsewhere as well.

use super::Vault;
use crate::error::VaultError;

/// Why a database cannot be written back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadOnly {
    /// One of Coffer's own snapshots. It opens like any other database and is
    /// not written back: the next save of the database it was taken from would
    /// rotate it away, so a change written here would be lost within ten saves.
    /// It can still be written somewhere else, and it can become the vault
    /// whole (see [`Vault::adopt`]).
    Snapshot,
    /// Kept somewhere that will not take a write: a read-only disk image, a
    /// Time Machine snapshot, a stick macOS mounted read-only, a share the
    /// reader may only read. It opens, because reading needs no write, and
    /// every change is refused because none of them could reach the file.
    Place,
    /// KeePass 1. The format cannot hold what Coffer would put back.
    Kdb,
    /// KDBX 3 carrying attachments. The reader collapses every attachment in a
    /// KDBX 3 database onto one, so saving would write fewer attachments than
    /// were read.
    Kdbx3Attachments,
}

impl ReadOnly {
    /// Whether what is open can still be written to a file somewhere else. A
    /// snapshot and a place are about where the database is, and a copy is
    /// written somewhere else; a format is why the bytes cannot be trusted,
    /// which a copy would carry with it.
    pub(super) fn copyable(self) -> bool {
        matches!(self, ReadOnly::Snapshot | ReadOnly::Place)
    }
}

impl From<ReadOnly> for VaultError {
    fn from(why: ReadOnly) -> VaultError {
        match why {
            ReadOnly::Snapshot => VaultError::ReadOnlySnapshot,
            ReadOnly::Place => VaultError::ReadOnlyPlace,
            ReadOnly::Kdb => VaultError::ReadOnlyKdb,
            ReadOnly::Kdbx3Attachments => VaultError::ReadOnlyKdbx3Attachments,
        }
    }
}

impl Vault {
    /// Why this database cannot be written back, or nothing when it can.
    pub fn read_only(&self) -> Option<ReadOnly> {
        self.source
    }

    /// Whether what is open can be written to a file somewhere else: anything
    /// but a format Coffer will not write. See [`Vault::encrypt_copy`].
    pub fn copyable(&self) -> bool {
        self.source.is_none_or(ReadOnly::copyable)
    }

    /// Whether the files the database holds can be read out of it: anything
    /// but KDBX 3 carrying attachments, whose bytes are not the ones their
    /// names promise. See [`Vault::attachment`].
    pub fn files_readable(&self) -> bool {
        self.source != Some(ReadOnly::Kdbx3Attachments)
    }
}
