//! An open database, and everything that can be done to one.

use std::io::Write;
use std::path::{Path, PathBuf};

use keepass::Database;
use keepass::config::{DatabaseVersion, InnerCipherConfig};
use keepass::db::{EntryId, GroupId, GroupRef, Times, Value};
use zeroize::Zeroizing;

use crate::attachment;
use crate::error::VaultError;
use crate::history::{self, Limits};
use crate::key::MasterKey;
use crate::model::{Attachment, Entry, Field, FieldValue, Project, Timestamps, Version, fields};
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

/// The largest file Coffer will put into a database.
///
/// The inner header records a file's length in four bytes, so anything past 4
/// GiB is written truncated and the database it lands in cannot be opened
/// again. The ceiling is far below that because the whole database is read into
/// memory to be opened, and [`MAX_DATABASE_BYTES`] is the ceiling on that.
const MAX_ATTACHMENT_BYTES: usize = 256 * 1024 * 1024;

/// What Coffer writes into `Meta/Generator`, the way every KeePass client
/// writes its own name there.
const GENERATOR: &str = "Coffer";

/// What Coffer calls the group it makes when a database asks for a recycle bin
/// and has none. The name KeePassXC uses, so the two agree on sight.
const RECYCLE_BIN: &str = "Recycle Bin";

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
    /// Whether this vault holds a change the file on disk does not.
    changed: bool,
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

/// What the file on disk holds, when it is not what this vault holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rival {
    /// When it was last written, as the filesystem has it.
    pub modified: Option<std::time::SystemTime>,
    /// How many entries it holds, or nothing when it will not open with the
    /// credentials this vault was opened with. A file somebody else changed is
    /// a file somebody else may have changed the password of.
    pub entries: Option<usize>,
}

/// Whether a write may go over a file somebody else has changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Guard {
    /// Stop and let the caller decide. Every ordinary save is this one.
    Refuse,
    /// Go over it. The snapshot taken on the way keeps what was there.
    Ignore,
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
    /// One of Coffer's own snapshots. It opens like any other database and is
    /// not written back: the next save of the database it was taken from would
    /// rotate it away, so a change written here would be lost within ten saves.
    /// It can still be written somewhere else, which is what the offer to keep
    /// a copy is for.
    Snapshot,
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
        let source = classify(&database, &path);

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
            changed: false,
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
        let entry = self.database.entry(id)?;
        let group = entry.parent().id();
        Some(entry_of(&entry, group))
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
        self.writable()?;
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
            self.changed = true;
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
        self.write(Guard::Refuse)
    }

    fn write(&mut self, guard: Guard) -> Result<(), VaultError> {
        self.writable()?;

        let standing = self.external_change()?;
        if guard == Guard::Refuse {
            match standing {
                Change::None => {}
                Change::Modified => return Err(VaultError::ExternalChange),
                Change::Gone => return Err(VaultError::DatabaseGone),
            }
        }

        self.prepare()?;

        // Nothing on disk is disturbed until the write is known to be possible.
        // Renaming over a file needs no write permission on that file, so a
        // read-only database would otherwise be replaced silently. A file that
        // is not there any more has no permissions to prove and is written back
        // by the rename itself.
        if standing != Change::Gone {
            atomic::ensure_writable(&self.path)?;
        }

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
        self.changed = false;
        Ok(())
    }

    /// Everything a write settles in the database before any bytes leave it.
    fn prepare(&mut self) -> Result<(), VaultError> {
        let limits = Limits::of(&self.database);
        history::prune_all(&mut self.database, limits);
        self.database.config.version = WRITTEN_VERSION;

        // Every KeePass client writes its own name here, so a file Coffer wrote
        // says so rather than going on claiming to be the work of whatever
        // wrote it last.
        self.database.meta.generator = Some(GENERATOR.to_owned());

        // A database that declares no inner cipher stores every protected value
        // as base64 plaintext inside the encrypted body. Writing that back would
        // be writing a weaker file than the one Coffer could write, and the
        // choice of inner cipher is not a field anybody can lose.
        if self.database.config.inner_cipher_config == InnerCipherConfig::Plain {
            self.database.config.inner_cipher_config = InnerCipherConfig::ChaCha20;
        }

        // The last check before the bytes go. A file the writer cannot place
        // where the reader will look for it is the one way this application
        // hands somebody else's password to the wrong entry, so the write does
        // not happen at all.
        if !attachment::unbroken(&self.database) {
            return Err(VaultError::AttachmentOrder);
        }

        Ok(())
    }

    /// Whether this vault holds a change the file on disk does not.
    pub fn is_dirty(&self) -> bool {
        self.changed
    }

    /// How many entries the vault holds. Previous versions are not entries and
    /// are not counted; the recycle bin's contents are, because they are still
    /// in the file.
    pub fn count(&self) -> usize {
        self.database.num_entries()
    }

    /// One previous version of an entry, read the same way the entry itself is.
    pub fn version(&self, id: EntryId, index: usize) -> Option<Entry> {
        let entry = self.database.entry(id)?;
        let group = entry.parent().id();
        let version = entry.historical(index)?;
        Some(entry_of(&version, group))
    }

    /// Hands out one field's value from a previous version.
    pub fn reveal_version(&self, id: EntryId, index: usize, field: &str) -> Option<SecretValue> {
        let entry = self.database.entry(id)?;
        let version = entry.historical(index)?;
        let value = version.fields.get(field)?;
        Some(SecretValue::new(value.get().as_bytes().to_vec()))
    }

    /// Makes a folder inside another one.
    pub fn create_group(&mut self, parent: GroupId, name: &str) -> Result<GroupId, VaultError> {
        self.writable()?;
        text::writable(name)?;

        let made = {
            let mut group = self
                .database
                .group_mut(parent)
                .ok_or(VaultError::NoSuchGroup)?;
            let mut made = group.add_group();
            made.name = name.to_owned();
            made.id()
        };

        self.changed = true;
        Ok(made)
    }

    /// Renames a folder.
    pub fn rename_group(&mut self, id: GroupId, name: &str) -> Result<(), VaultError> {
        self.writable()?;
        text::writable(name)?;

        let mut group = self.database.group_mut(id).ok_or(VaultError::NoSuchGroup)?;
        if group.name == name {
            return Ok(());
        }
        group.name = name.to_owned();
        group.times.last_modification = Some(Times::now());

        self.changed = true;
        Ok(())
    }

    /// Deletes a folder and everything in it.
    ///
    /// A database with a recycle bin gets the folder moved into it, and a
    /// folder that is already in the bin, or a database with no bin, gets it
    /// removed and recorded in `DeletedObjects` so that other clients do not
    /// bring it back.
    pub fn delete_group(&mut self, id: GroupId) -> Result<(), VaultError> {
        self.writable()?;
        if id == self.database.root().id() {
            return Err(VaultError::CannotMoveRoot);
        }
        if self.database.group(id).is_none() {
            return Err(VaultError::NoSuchGroup);
        }

        match self.bin_for(id)? {
            Some(bin) => self.move_group(id, bin),
            None => self.erase_group(id),
        }
    }

    /// Empties the recycle bin, if the database has one with anything in it.
    pub fn empty_recycle_bin(&mut self) -> Result<(), VaultError> {
        self.writable()?;
        let Some(bin) = self.recycle_bin() else {
            return Ok(());
        };

        let sections: Vec<GroupId> = self.sections_of(bin);
        let entries: Vec<EntryId> = self.entries_of(bin);

        for section in sections {
            self.erase_group(section)?;
        }
        for entry in entries {
            self.erase_entry(entry)?;
        }
        Ok(())
    }

    /// Makes an entry in a folder.
    ///
    /// The five fields the format names are written empty, protected as the
    /// database asks for them to be, so that an entry Coffer made looks like an
    /// entry KeePassXC made and the screen has every row to edit.
    pub fn create_entry(&mut self, group: GroupId) -> Result<EntryId, VaultError> {
        self.writable()?;
        let protection = self.protection();

        let made = {
            let mut group = self
                .database
                .group_mut(group)
                .ok_or(VaultError::NoSuchGroup)?;
            let mut entry = group.add_entry();
            for (name, protect) in protection {
                if protect {
                    entry.set_protected(name, "");
                } else {
                    entry.set_unprotected(name, "");
                }
            }
            entry.id()
        };

        self.changed = true;
        Ok(made)
    }

    /// Takes a field off an entry.
    pub fn remove_field(&mut self, id: EntryId, field: &str) -> Result<(), VaultError> {
        self.writable()?;
        if !self
            .database
            .entry(id)
            .ok_or(VaultError::NoSuchEntry)?
            .fields
            .contains_key(field)
        {
            return Err(VaultError::NoSuchField);
        }

        let field = field.to_owned();
        history::edit(&mut self.database, id, move |entry| {
            entry.fields.remove(&field);
            entry.times.last_modification = Some(Times::now());
        });

        self.changed = true;
        Ok(())
    }

    /// Replaces an entry's tags.
    ///
    /// The format keeps them as one string with semicolons between, and reads
    /// a comma and a tab as separators too, so a tag holding one of those would
    /// come back as two tags. A tag padded with spaces comes back trimmed. Both
    /// are refused rather than written and silently changed.
    pub fn set_tags(&mut self, id: EntryId, tags: Vec<String>) -> Result<(), VaultError> {
        self.writable()?;
        for tag in &tags {
            text::writable(tag)?;
            if tag.is_empty() || tag.trim() != tag || tag.contains([';', ',', '\t']) {
                return Err(VaultError::UnwritableText);
            }
        }
        if self.database.entry(id).is_none() {
            return Err(VaultError::NoSuchEntry);
        }

        history::edit(&mut self.database, id, move |entry| {
            entry.tags = tags;
            entry.times.last_modification = Some(Times::now());
        });

        self.changed = true;
        Ok(())
    }

    /// Deletes an entry.
    ///
    /// As with a folder: to the recycle bin when the database has one, and out
    /// of the file and into `DeletedObjects` when it is already there.
    pub fn delete_entry(&mut self, id: EntryId) -> Result<(), VaultError> {
        self.writable()?;
        let group = self
            .database
            .entry(id)
            .ok_or(VaultError::NoSuchEntry)?
            .parent()
            .id();

        match self.bin_for(group)? {
            Some(bin) => {
                let mut entry = self.database.entry_mut(id).ok_or(VaultError::NoSuchEntry)?;
                entry.move_to(bin).map_err(|_| VaultError::NoSuchGroup)?;
                entry.times.location_changed = Some(Times::now());
                self.changed = true;
                Ok(())
            }
            None => self.erase_entry(id),
        }
    }

    /// Puts a file on an entry, replacing one of the same name.
    ///
    /// No previous version is written; see [`Vault::remove_attachment`] for why
    /// the files on an entry are not part of its history.
    pub fn add_attachment(
        &mut self,
        id: EntryId,
        name: &str,
        data: Zeroizing<Vec<u8>>,
    ) -> Result<(), VaultError> {
        self.writable()?;
        if data.len() > MAX_ATTACHMENT_BYTES {
            return Err(VaultError::AttachmentTooLarge);
        }
        if name.is_empty() {
            return Err(VaultError::UnwritableText);
        }
        text::writable(name)?;
        if self.database.entry(id).is_none() {
            return Err(VaultError::NoSuchEntry);
        }

        // Replacing a file means removing the old one first. The library's own
        // replacement takes the bytes of the file it replaces out from under
        // every previous version that still names them, and leaves a hole in
        // the pool behind it.
        if attachment::held(&self.database, id, name) {
            attachment::detach(&mut self.database, id, name)?;
        }

        // Added as a protected value: the flag is what KeePassXC writes, and it
        // is what keeps the bytes in a buffer that wipes itself and prints
        // `[REDACTED]` rather than the file.
        let value = Value::protected(data.to_vec());
        let mut entry = self.database.entry_mut(id).ok_or(VaultError::NoSuchEntry)?;
        entry.add_attachment(name.to_owned(), value);
        entry.times.last_modification = Some(Times::now());

        self.changed = true;
        Ok(())
    }

    /// Takes a file off an entry.
    ///
    /// No previous version is written, and none is written when a file is added
    /// either: **the files on an entry are not part of its history.** A version
    /// records the entry as it was, files included, and the library gives no way
    /// to change what a version points at, so a version written across a change
    /// to the pool would name bytes that have moved or gone. Writing one on the
    /// way in would also be the surest way to make the file impossible to take
    /// off again.
    pub fn remove_attachment(&mut self, id: EntryId, name: &str) -> Result<(), VaultError> {
        self.writable()?;
        attachment::detach(&mut self.database, id, name)?;

        if let Some(mut entry) = self.database.entry_mut(id) {
            entry.times.last_modification = Some(Times::now());
        }

        self.changed = true;
        Ok(())
    }

    /// Makes a previous version the current state of the entry, keeping what it
    /// replaces as a version of its own.
    pub fn restore_version(&mut self, id: EntryId, index: usize) -> Result<(), VaultError> {
        self.writable()?;
        history::restore(&mut self.database, id, index)?;
        self.changed = true;
        Ok(())
    }

    /// Drops one previous version.
    pub fn delete_version(&mut self, id: EntryId, index: usize) -> Result<(), VaultError> {
        self.writable()?;
        history::forget(&mut self.database, id, index)?;
        self.changed = true;
        Ok(())
    }

    /// Drops every previous version of an entry, leaving it as it is now.
    ///
    /// A file that only a version named stays in the database. Coffer removes a
    /// file when it is asked to remove that file and at no other time: bytes
    /// kept are never a lost field, and the pool has no room for bytes that
    /// nothing names.
    pub fn clear_history(&mut self, id: EntryId) -> Result<(), VaultError> {
        self.writable()?;
        history::clear(&mut self.database, id)?;
        self.changed = true;
        Ok(())
    }

    /// Writes the database back over the file it was opened from, whatever has
    /// happened to that file since.
    ///
    /// The snapshot taken before the write is of the file as it is now, so what
    /// this replaces is in `<database>.1.bak` and can be opened again. That is
    /// what makes this offer safe to put in front of somebody.
    pub fn save_over(&mut self) -> Result<(), VaultError> {
        self.write(Guard::Ignore)
    }

    /// Writes the database somewhere else, leaving the file it came from alone.
    ///
    /// No snapshot is rotated and nothing about this vault changes: the copy is
    /// a copy, and the database is still the one this vault has open.
    pub fn save_copy(&mut self, path: &Path) -> Result<(), VaultError> {
        match self.source {
            Source::Kdb => return Err(VaultError::ReadOnlyKdb),
            Source::Kdbx3WithAttachments => return Err(VaultError::ReadOnlyKdbx3Attachments),
            Source::Snapshot | Source::Writable => {}
        }
        if path.canonicalize().is_ok_and(|target| target == self.path) {
            return Err(VaultError::CopyOntoItself);
        }

        self.prepare()?;

        let database = &self.database;
        let key = self.key.to_database_key()?;
        atomic::write_atomic(path, move |writer: &mut dyn Write| {
            database.save(writer, key).map_err(VaultError::from)
        })
    }

    /// What the file on disk holds now.
    ///
    /// Reading it means decrypting it, which is another key derivation, so this
    /// is asked for when there is a decision to make and never on a timer.
    pub fn rival(&self) -> Rival {
        Rival {
            modified: std::fs::metadata(&self.path)
                .ok()
                .and_then(|about| about.modified().ok()),
            // A file that will not open is still an answer: the dialog says how
            // many entries are in this window and that it cannot say what is in
            // the other one.
            entries: read(&self.path, &self.key)
                .ok()
                .map(|database| database.num_entries()),
        }
    }

    /// Throws away everything in this vault and reads the file again.
    pub fn reload(&mut self) -> Result<(), VaultError> {
        let stamp = Stamp::of(&self.path)?;
        let database = read(&self.path, &self.key)?;

        self.source = classify(&database, &self.path);
        self.database = database;
        self.stamp = stamp;
        self.changed = false;
        Ok(())
    }

    /// Whether this database may be written back at all, and why not when it
    /// may not.
    fn writable(&self) -> Result<(), VaultError> {
        match self.source {
            Source::Kdb => Err(VaultError::ReadOnlyKdb),
            Source::Kdbx3WithAttachments => Err(VaultError::ReadOnlyKdbx3Attachments),
            Source::Snapshot => Err(VaultError::ReadOnlySnapshot),
            Source::Writable => Ok(()),
        }
    }

    /// Which fields the database asks to be kept protected.
    ///
    /// The library never reads these on its own: protection is a property of
    /// each value, so a database that asks for a protected password gets one
    /// only because Coffer builds it that way.
    fn protection(&self) -> [(&'static str, bool); 5] {
        let asked = self.database.meta.memory_protection.as_ref();
        [
            (
                fields::TITLE,
                asked.is_some_and(|memory| memory.protect_title),
            ),
            (
                fields::USERNAME,
                asked.is_some_and(|memory| memory.protect_username),
            ),
            // Absent memory protection is KeePass's own default, which protects
            // the password and nothing else.
            (
                fields::PASSWORD,
                asked.is_none_or(|memory| memory.protect_password),
            ),
            (fields::URL, asked.is_some_and(|memory| memory.protect_url)),
            (
                fields::NOTES,
                asked.is_some_and(|memory| memory.protect_notes),
            ),
        ]
    }

    /// The recycle bin a deletion out of `group` should go to, or nothing when
    /// the deletion is a removal: the database says it keeps no bin, or the
    /// group is the bin or inside it.
    ///
    /// The bin is made here when the database asks for one and has none, which
    /// is what KeePassXC does on the first deletion.
    fn bin_for(&mut self, group: GroupId) -> Result<Option<GroupId>, VaultError> {
        if self.database.meta.recyclebin_enabled == Some(false) {
            return Ok(None);
        }

        if let Some(bin) = self.recycle_bin() {
            if bin == group || self.sections_of(bin).contains(&group) {
                return Ok(None);
            }
            return Ok(Some(bin));
        }

        let made = {
            let mut root = self.database.root_mut();
            let mut made = root.add_group();
            made.name = RECYCLE_BIN.to_owned();
            // The bin is not part of the vault a search runs over, which is the
            // flag KeePassXC writes on the group it makes.
            made.enable_searching = Some(false);
            made.id()
        };

        self.database.meta.recyclebin_enabled = Some(true);
        self.database.meta.recyclebin_uuid = Some(made.uuid());
        self.database.meta.recyclebin_changed = Some(Times::now());
        self.changed = true;
        Ok(Some(made))
    }

    fn move_group(&mut self, id: GroupId, into: GroupId) -> Result<(), VaultError> {
        use keepass::db::MoveGroupError;

        let mut group = self.database.group_mut(id).ok_or(VaultError::NoSuchGroup)?;
        group.move_to(into).map_err(|error| match error {
            MoveGroupError::CannotMoveRoot => VaultError::CannotMoveRoot,
            MoveGroupError::WouldCreateCycle => VaultError::CannotMoveIntoItself,
            _ => VaultError::NoSuchGroup,
        })?;
        group.times.location_changed = Some(Times::now());

        self.changed = true;
        Ok(())
    }

    /// Takes a folder out of the file for good.
    fn erase_group(&mut self, id: GroupId) -> Result<(), VaultError> {
        let mut gone = self.sections_of(id);
        gone.push(id);
        let doomed: Vec<EntryId> = gone
            .iter()
            .flat_map(|section| self.entries_of(*section))
            .collect();

        // The files first, by Coffer's rules. The library's own removal takes
        // the bytes of everything these entries name, whether or not somebody
        // else names them too, and leaves the pool full of holes.
        attachment::detach_entries(&mut self.database, &doomed)?;

        self.database
            .group_mut(id)
            .ok_or(VaultError::NoSuchGroup)?
            .track_changes()
            .remove()
            .map_err(|_| VaultError::CannotMoveRoot)?;

        self.forget_groups(&gone);
        self.changed = true;
        Ok(())
    }

    /// Takes an entry out of the file for good.
    fn erase_entry(&mut self, id: EntryId) -> Result<(), VaultError> {
        attachment::detach_entries(&mut self.database, &[id])?;

        self.database
            .entry_mut(id)
            .ok_or(VaultError::NoSuchEntry)?
            .track_changes()
            .remove();

        self.changed = true;
        Ok(())
    }

    /// Clears the `Meta` fields that name a group that has just gone.
    ///
    /// The library does this for its untracked removal and not for the tracked
    /// one, which is the one Coffer uses because it is the one that records a
    /// deletion. A `RecycleBinUUID` naming a group that is not there is how a
    /// vault ends up with two recycle bins.
    fn forget_groups(&mut self, gone: &[GroupId]) {
        let meta = &mut self.database.meta;
        for field in [
            &mut meta.recyclebin_uuid,
            &mut meta.entry_templates_group,
            &mut meta.last_selected_group,
            &mut meta.last_top_visible_group,
        ] {
            if let Some(named) = *field
                && gone.iter().any(|group| group.uuid() == named)
            {
                *field = None;
            }
        }
    }

    /// Every folder inside this one, however deep.
    fn sections_of(&self, id: GroupId) -> Vec<GroupId> {
        fn walk(group: &GroupRef<'_>, into: &mut Vec<GroupId>) {
            for section in group.groups() {
                into.push(section.id());
                walk(&section, into);
            }
        }

        let mut found = Vec::new();
        if let Some(group) = self.database.group(id) {
            walk(&group, &mut found);
        }
        found
    }

    /// The entries this folder holds directly.
    fn entries_of(&self, id: GroupId) -> Vec<EntryId> {
        self.database
            .group(id)
            .map(|group| group.entries().map(|entry| entry.id()).collect())
            .unwrap_or_default()
    }

    fn recycle_bin(&self) -> Option<keepass::db::GroupId> {
        self.database.recycle_bin().map(|group| group.id())
    }
}

fn read(path: &Path, key: &MasterKey) -> Result<Database, VaultError> {
    use std::io::Read;

    let file = std::fs::File::open(path)?;

    if file.metadata()?.len() > MAX_DATABASE_BYTES {
        return Err(VaultError::TooLarge);
    }

    let mut bytes = Vec::new();
    let mut file = file;
    file.read_to_end(&mut bytes)?;

    preflight::check(&bytes)?;

    let database_key = key.to_database_key()?;
    parse_without_dying(&bytes, database_key)
}

/// Parses a database, turning a panic inside the parser into an error.
///
/// The pre-flight stops everything it can reach before the file has been
/// authenticated, but the parser has reachable panics inside the decrypted body
/// as well - a timestamp shorter than eight bytes is one - and a vault that
/// dies on a corrupt file is a vault that cannot offer the user their backup.
/// Nothing has been written at this point, so there is no half-finished state
/// to unwind into.
fn parse_without_dying(bytes: &[u8], key: keepass::DatabaseKey) -> Result<Database, VaultError> {
    // A panic inside the parser quotes the document it failed on, and that
    // document is the user's passwords. The default hook writes it to standard
    // error, where a terminal, a log or a crash reporter can pick it up. The
    // hook is silenced for exactly the length of this call and put back
    // afterwards, so that nothing else in the process loses its diagnostics.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let outcome =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Database::parse(bytes, key)));

    std::panic::set_hook(previous);

    outcome
        .map_err(|_| VaultError::DamagedContent)?
        .map_err(VaultError::from)
}

fn classify(database: &Database, path: &Path) -> Source {
    match database.config.version {
        DatabaseVersion::KDB(_) | DatabaseVersion::KDB2(_) => Source::Kdb,
        DatabaseVersion::KDB3(_) if database.num_attachments() > 0 => Source::Kdbx3WithAttachments,
        _ if snapshot::slot_of(path).is_some() => Source::Snapshot,
        _ => Source::Writable,
    }
}

fn project_of(group: GroupRef<'_>, recycle_bin: Option<keepass::db::GroupId>) -> Project {
    let group_id = group.id();
    Project {
        id: group_id,
        name: group.name.clone(),
        notes: group.notes.clone(),
        is_recycle_bin: Some(group.id()) == recycle_bin,
        sections: group
            .groups()
            .map(|section| project_of(section, recycle_bin))
            .collect(),
        entries: group
            .entries()
            .map(|entry| entry_of(&entry, group_id).summary())
            .collect(),
    }
}

/// Reads an entry, or one previous version of one.
///
/// The group is passed in rather than read off the entry, because a previous
/// version carries the group it was in when it was written and that group may
/// be gone; asking a version for its parent is a way to bring the window down.
fn entry_of(entry: &keepass::db::EntryRef<'_>, group: GroupId) -> Entry {
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
        group,
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
