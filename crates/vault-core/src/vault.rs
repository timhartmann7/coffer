//! An open database, and everything that can be done to one.

use std::io::Write;
use std::path::{Path, PathBuf};

use keepass::Database;
use keepass::config::{DatabaseVersion, InnerCipherConfig};
use keepass::db::{EntryId, GroupRef, Value};
use zeroize::Zeroizing;

use crate::error::VaultError;
use crate::history::{self, Limits};
use crate::key::MasterKey;
use crate::model::{Attachment, Entry, Field, FieldValue, Project, Timestamps, Version};
use crate::preflight;
use crate::secret::SecretValue;
use crate::storage::lock::{Lock, Outcome};
use crate::storage::watch::{Change, Stamp};
use crate::storage::{atomic, snapshot, watch};
use crate::text;

/// The largest file Coffer will read into memory to try to open.
///
/// A KeePass database is decrypted whole, so the file's own size bounds the
/// allocation. Anything past this is not a database anyone made; it is a way to
/// make Coffer ask the allocator for everything.
const MAX_DATABASE_BYTES: u64 = 1 << 30;

/// The only format Coffer writes.
const WRITTEN_VERSION: DatabaseVersion = DatabaseVersion::KDB4(1);

/// What to do about a lock file somebody else left beside the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockPolicy {
    /// Refuse to open a database another process has open.
    Respect,
    /// Open it anyway and take the lock over, because the user was shown who
    /// held it and said to go ahead.
    TakeOver,
}

/// A new value for a field.
pub enum NewValue {
    /// Stored as written. Anything that opens the database can read it.
    Open(String),
    /// Stored protected, behind the database's inner cipher.
    Protected(Zeroizing<String>),
}

/// An open database.
///
/// The vault holds the decrypted tree and the credentials that opened it, both
/// wiped when it is dropped. Keeping the master password for as long as the
/// database is unlocked is not a choice: KeePass derives a fresh key on every
/// save, so a vault that can save is a vault that still has the password.
pub struct Vault {
    database: Database,
    path: PathBuf,
    key: MasterKey,
    source: Source,
    stamp: Stamp,
    /// Held for as long as the vault is open; removed when it is dropped.
    _lock: Lock,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("path", &self.path)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

/// The format the database was read from, which decides whether it may be
/// written back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    /// KeePass 1. Read only: the format cannot hold what Coffer would put back.
    Kdb,
    /// KDBX 3 carrying attachments. Read only: the reader collapses every
    /// attachment in a KDBX 3 database onto one, so saving would write fewer
    /// attachments than were read.
    Kdbx3WithAttachments,
    /// Anything Coffer can write back as KDBX 4.1.
    Writable,
}

impl Vault {
    /// Opens the database at `path`.
    ///
    /// Symbolic links are followed to the file they name, so that a database
    /// reached through a link is saved where the link points rather than being
    /// replaced by it.
    pub fn open(path: &Path, key: MasterKey, policy: LockPolicy) -> Result<Vault, VaultError> {
        let path = path.canonicalize().map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => VaultError::DatabaseGone,
            _ => VaultError::Io(error),
        })?;

        let stamp = Stamp::of(&path)?;
        let database = read(&path, &key)?;
        let source = classify(&database);

        let lock = match policy {
            LockPolicy::TakeOver => Lock::take(&path)?,
            LockPolicy::Respect => match Lock::acquire(&path)? {
                Outcome::Taken(lock) => lock,
                Outcome::Held(holder) => return Err(VaultError::Locked(holder)),
            },
        };

        Ok(Vault {
            database,
            path,
            key,
            source,
            stamp,
            _lock: lock,
        })
    }

    /// Where the database lives, after symbolic links have been followed.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether this database can be written back at all.
    pub fn is_read_only(&self) -> bool {
        self.source != Source::Writable
    }

    /// The database's groups, as a tree.
    ///
    /// The value returned is the root group, which every KeePass database has
    /// and which holds everything else. Its sections are what Coffer calls
    /// projects. Previous versions of an entry are not in it: they belong to
    /// their entry and never appear in a tree, a list or a search.
    pub fn tree(&self) -> Project {
        project_of(self.database.root(), self.recycle_bin())
    }

    /// One entry's metadata.
    pub fn entry(&self, id: EntryId) -> Option<Entry> {
        self.database.entry(id).map(|entry| entry_of(&entry))
    }

    /// The previous versions of an entry, oldest first.
    ///
    /// Ordered by modification time rather than by position, because position
    /// is not a reliable order: a file another client wrote may list versions
    /// in any order at all. `index` stays the position in the file, so it goes
    /// on addressing the same version whatever order the list comes back in.
    pub fn versions(&self, id: EntryId) -> Vec<Version> {
        let Some(entry) = self.database.entry(id) else {
            return Vec::new();
        };
        let Some(history) = entry.history.as_ref() else {
            return Vec::new();
        };

        let mut versions: Vec<Version> = history
            .get_entries()
            .iter()
            .enumerate()
            .map(|(index, version)| Version {
                index,
                modified: version.times.last_modification,
            })
            .collect();

        versions.sort_by_key(|version| (version.modified.is_none(), version.modified));
        versions
    }

    /// Hands out one field's value.
    ///
    /// This is the only way a field value leaves the vault, and every call is a
    /// place a secret can escape.
    pub fn reveal(&self, id: EntryId, field: &str) -> Option<SecretValue> {
        let entry = self.database.entry(id)?;
        let value = entry.fields.get(field)?;
        Some(SecretValue::new(value.get().as_bytes().to_vec()))
    }

    /// Hands out one attachment's bytes.
    pub fn attachment(&self, id: EntryId, name: &str) -> Option<SecretValue> {
        let entry = self.database.entry(id)?;
        let attachment = entry.attachment_by_name(name)?;
        Some(SecretValue::new(attachment.data.get().clone()))
    }

    /// Sets a field, keeping the entry's previous state as a version.
    ///
    /// An edit that leaves the entry the same as it was writes no version and
    /// does not move the modification time.
    pub fn set_field(
        &mut self,
        id: EntryId,
        field: &str,
        value: NewValue,
    ) -> Result<(), VaultError> {
        text::writable(field)?;
        match &value {
            NewValue::Open(written) => text::writable(written)?,
            NewValue::Protected(written) => text::writable(written)?,
        }

        let field = field.to_owned();
        let edited = history::edit(&mut self.database, id, move |entry| match value {
            NewValue::Open(written) => entry.set(field, Value::unprotected(written)),
            NewValue::Protected(written) => {
                entry.set(field, Value::protected(written.to_string()));
            }
        });

        if edited {
            Ok(())
        } else {
            Err(VaultError::NoSuchEntry)
        }
    }

    /// Whether the file on disk is still the one this vault was opened from.
    pub fn external_change(&self) -> Result<Change, VaultError> {
        Ok(watch::since(&self.path, self.stamp)?)
    }

    /// Writes the database back.
    ///
    /// Nothing is written until the file on disk is confirmed to be the one the
    /// vault was opened from, a snapshot has been taken, and every entry's
    /// history has been brought inside the database's limits. The write itself
    /// goes to a temporary file beside the database and is renamed into place,
    /// so a process killed part way through leaves the original untouched.
    pub fn save(&mut self) -> Result<(), VaultError> {
        match self.source {
            Source::Kdb => return Err(VaultError::ReadOnlyKdb),
            Source::Kdbx3WithAttachments => return Err(VaultError::ReadOnlyKdbx3Attachments),
            Source::Writable => {}
        }

        match self.external_change()? {
            Change::None => {}
            Change::Modified => return Err(VaultError::ExternalChange),
            Change::Gone => return Err(VaultError::DatabaseGone),
        }

        let limits = Limits::of(&self.database);
        history::prune_all(&mut self.database, limits);
        self.database.config.version = WRITTEN_VERSION;

        // A database that declares no inner cipher stores every protected value
        // as base64 plaintext inside the encrypted body. Writing that back would
        // be writing a weaker file than the one Coffer could write, and the
        // choice of inner cipher is not a field anybody can lose.
        if self.database.config.inner_cipher_config == InnerCipherConfig::Plain {
            self.database.config.inner_cipher_config = InnerCipherConfig::ChaCha20;
        }

        // Nothing on disk is disturbed until the write is known to be possible.
        // Renaming over a file needs no write permission on that file, so a
        // read-only database would otherwise be replaced silently.
        atomic::ensure_writable(&self.path)?;

        // The whole database is encrypted into a temporary file first. Every
        // way a save can fail in practice - a full volume, an unwritable
        // directory, a process killed part way through - happens here, while
        // the database and its snapshots are still untouched. A run of failed
        // saves used to push ten copies of the same generation through the
        // snapshot chain and destroy it.
        let database = &self.database;
        let key = self.key.to_database_key()?;
        let staged = atomic::stage(&self.path, move |writer: &mut dyn Write| {
            database.save(writer, key).map_err(VaultError::from)
        })?;

        snapshot::rotate(&self.path)?;

        // The snapshot is a hard link, which changes the database's change
        // time. That change is Coffer's own, so it is recorded now: a vault
        // that mistook its own snapshot for somebody else's edit could never
        // save again, and every unsaved edit would be stranded.
        self.stamp = Stamp::of(&self.path)?;

        staged.commit()?;

        self.stamp = Stamp::of(&self.path)?;
        Ok(())
    }

    fn recycle_bin(&self) -> Option<keepass::db::GroupId> {
        self.database.recycle_bin().map(|group| group.id())
    }
}

fn read(path: &Path, key: &MasterKey) -> Result<Database, VaultError> {
    use std::io::Read;

    let file = std::fs::File::open(path)?;

    if file.metadata()?.len() > MAX_DATABASE_BYTES {
        return Err(VaultError::NotADatabase);
    }

    let mut bytes = Vec::new();
    let mut file = file;
    file.read_to_end(&mut bytes)?;

    preflight::check_key_derivation(&bytes)?;

    Ok(Database::parse(&bytes, key.to_database_key()?)?)
}

fn classify(database: &Database) -> Source {
    match database.config.version {
        DatabaseVersion::KDB(_) | DatabaseVersion::KDB2(_) => Source::Kdb,
        DatabaseVersion::KDB3(_) if database.num_attachments() > 0 => Source::Kdbx3WithAttachments,
        _ => Source::Writable,
    }
}

fn project_of(group: GroupRef<'_>, recycle_bin: Option<keepass::db::GroupId>) -> Project {
    Project {
        id: group.id(),
        name: group.name.clone(),
        notes: group.notes.clone(),
        is_recycle_bin: Some(group.id()) == recycle_bin,
        sections: group
            .groups()
            .map(|section| project_of(section, recycle_bin))
            .collect(),
        entries: group.entries().map(|entry| entry_of(&entry)).collect(),
    }
}

fn entry_of(entry: &keepass::db::EntryRef<'_>) -> Entry {
    let mut fields: Vec<Field> = entry
        .fields
        .iter()
        .map(|(name, value)| Field {
            name: name.clone(),
            value: match value {
                Value::Unprotected(text) => FieldValue::Open(text.clone()),
                Value::Protected(_) => FieldValue::Protected {
                    empty: value.get().is_empty(),
                },
            },
        })
        .collect();
    fields.sort_by(|a, b| a.name.cmp(&b.name));

    let mut attachments: Vec<Attachment> = entry
        .attachments_named()
        .map(|(name, attachment)| Attachment {
            name: name.to_owned(),
            size: attachment.data.get().len(),
        })
        .collect();
    attachments.sort_by(|a, b| a.name.cmp(&b.name));

    Entry {
        id: entry.id(),
        group: entry.parent().id(),
        fields,
        attachments,
        tags: entry.tags.clone(),
        times: Timestamps {
            created: entry.times.creation,
            modified: entry.times.last_modification,
            expires: entry
                .times
                .expiry
                .filter(|_| entry.times.expires == Some(true)),
        },
        versions: entry.history.as_ref().map_or(0, |h| h.get_entries().len()),
    }
}
