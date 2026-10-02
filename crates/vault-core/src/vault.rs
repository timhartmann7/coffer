//! An open database, and everything that can be done to one.

use std::io::Write;
use std::path::{Path, PathBuf};

use keepass::Database;
use keepass::config::{DatabaseVersion, InnerCipherConfig};
use keepass::db::{EntryId, EntryRef, GroupId, GroupRef, History, Times, Value};
use zeroize::Zeroizing;

use crate::attachment;
use crate::bin::{Bin, Standing};
use crate::blank;
use crate::clash::{self, Clash};
use crate::error::VaultError;
use crate::history::{self, Limits};
use crate::kdf::Work;
use crate::key::MasterKey;
use crate::model::{
    self, Attachment, Binned, Deletion, Entry, Field, FieldValue, Project, Timestamps, Version,
    fields,
};
use crate::preflight;
use crate::secret::SecretValue;
use crate::storage::lock::{Lock, Outcome, claim};
use crate::storage::watch::{Change, Content, Stamp};
use crate::storage::{self, Seen, atomic, snapshot, unsaved, watch};
use crate::templates;
use crate::text;
use crate::wipe;

mod adopt;
mod batch;
mod began;
mod copy;
mod making;
mod moves;
mod read_only;
mod rekey;

pub use adopt::Adopted;
pub use copy::EncryptedCopy;
pub use read_only::ReadOnly;

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
///
/// Public because whoever reads a file off the disk has to know the size before
/// it reads it. Handing this function twenty gigabytes and letting it refuse
/// afterwards means twenty gigabytes were already in memory.
pub const MAX_ATTACHMENT_BYTES: usize = 256 * 1024 * 1024;

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

/// What a new database is made with.
pub struct Recipe<'a> {
    /// What the top group and the database's own name are called. The file's
    /// stem, so that the screen has one field fewer and nothing the window
    /// sends names a file.
    pub name: &'a str,
    /// How hard its key derivation is. Given rather than measured here, so that
    /// a suite can make a thousand databases without paying a calibrated second
    /// for each of them.
    pub work: Work,
}

/// A new value for a field. Which kind it is decides how a field the entry
/// does not have yet is stored; a field it has keeps its own protection, which
/// only [`Vault::set_protection`] changes.
pub enum NewValue {
    /// Stored open, when it makes the field. Anything that opens the database
    /// can read it.
    Open(String),
    /// Stored protected, behind the database's inner cipher, when it makes the
    /// field.
    Protected(Zeroizing<String>),
}

impl NewValue {
    /// The text, whichever way it was to be stored, moved rather than copied.
    fn into_text(self) -> Zeroizing<String> {
        match self {
            NewValue::Open(text) => Zeroizing::new(text),
            NewValue::Protected(text) => text,
        }
    }
}

/// How text the reader was typing when the vault had to go stands to the field
/// it was typed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typing {
    /// Typed into the field where it stands. What is in the field is what the
    /// reader wants there, and it is written into the field.
    InPlace,
    /// Typed as a new value for the field, in a field of its own under it,
    /// while the value there stays until the reader saves the new one. A new
    /// password half typed, or the wrong one pasted, is not the reader's word
    /// that the old one may go. It is kept beside the value rather than over
    /// it (see [`Vault::set_typed`]).
    Beside,
}

/// Where text typed before a lock went, as [`Vault::set_typed`] answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    /// Nowhere: it was nothing new.
    Nothing,
    /// Into the field it was typed for.
    Into,
    /// Into a field of its own beside the one it was typed for, whose value
    /// stays as it was. The reader who comes back has to be told: the field
    /// they typed into still holds the old value.
    Beside,
}

/// What the field a new value is kept in when it cannot go over the old one is
/// called, after the old one's name: a string field like any the reader makes,
/// so every KeePass client shows it, and named so the reader knows it on sight.
const TYPED_BEFORE_LOCKING: &str = "(typed before locking)";

/// An open database.
///
/// The vault holds the decrypted tree and the credentials that opened it, both
/// wiped when it is dropped. Keeping the master password for as long as the
/// database is unlocked is not a choice: KeePass derives a fresh key on every
/// save, so a vault that can save is a vault that still has the password.
pub struct Vault {
    /// Declared first so that it is dropped first: the tree is emptied, and
    /// only then is the lock file beside the database let go.
    database: Held,
    path: PathBuf,
    key: MasterKey,
    /// Why the database may not be written back, decided by what it was read
    /// from - its format and its place - or nothing when it may be.
    source: Option<ReadOnly>,
    stamp: Stamp,
    /// The bytes the stamp went with. Read only when the stamp cannot decide on
    /// its own, which is when nothing but the change time moved.
    content: Content,
    /// Whether this vault holds a change the file on disk does not.
    changed: bool,
    /// How many times the tree has changed since the vault opened, counting
    /// what a save settles and what a reload replaces. See [`Vault::edits`].
    edits: u64,
    /// The snapshots beside the database that open with a master password
    /// this vault no longer has: every one there was when the password last
    /// changed. Empty until it does.
    superseded: snapshot::Superseded,
    /// Held for as long as the vault is open; removed when it is dropped.
    ///
    /// Absent where the place beside the database would not take the file.
    /// Nothing can be written there, so there is nothing for a lock to guard,
    /// and a vault that refused to open over a note it could not leave would be
    /// refusing to read a file that reads perfectly well.
    _lock: Option<Lock>,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("path", &self.path)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

/// A decrypted database, emptied when it goes.
///
/// The wrapper is what carries the destructor, and it carries it here rather
/// than on [`Vault`] for three reasons. Reloading assigns into this field, which
/// drops a `Database` and not a `Vault`. Reading the rival file builds a whole
/// second database and throws it away without a vault ever existing. And a type
/// with a `Drop` cannot have a field moved out of it, which would make `Vault`
/// harder to work with for nothing.
struct Held(Database);

impl Drop for Held {
    fn drop(&mut self) {
        wipe::database(&mut self.0);
    }
}

impl std::ops::Deref for Held {
    type Target = Database;

    fn deref(&self) -> &Database {
        &self.0
    }
}

impl std::ops::DerefMut for Held {
    fn deref_mut(&mut self) -> &mut Database {
        &mut self.0
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

/// What came of a file offered to an entry.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attached {
    /// It is on the entry, under the name it was offered with.
    Added,
    /// The entry already gives that name to a file, and nothing changed. The
    /// answer is the reader's, through [`Vault::keep_both`] or
    /// [`Vault::replace_attachment`].
    Taken(Clash),
}

/// What a lock did with changes the file has not got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rescue {
    /// The vault held nothing the file did not already have.
    Nothing,
    /// The ordinary save went through, and the work is where the reader
    /// believes it is.
    Saved,
    /// The save was refused, and everything that was in the window is in the
    /// file beside the database instead.
    Kept,
    /// Neither could be written. The vault is wiped anyway.
    Lost,
}

/// Whether a write may go over a file somebody else has changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Guard {
    /// Stop and let the caller decide. Every ordinary save is this one.
    Refuse,
    /// Go over it. The snapshot taken on the way keeps what was there.
    Ignore,
}

impl Vault {
    /// Opens the database at `path`.
    ///
    /// Symbolic links are followed to the file they name, so that a database
    /// reached through a link is saved where the link points rather than being
    /// replaced by it.
    pub fn open(path: &Path, mut key: MasterKey, policy: LockPolicy) -> Result<Vault, VaultError> {
        let path = path.canonicalize().map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => VaultError::DatabaseGone,
            _ => VaultError::Io(error),
        })?;

        let stamp = Stamp::of(&path)?;
        let (database, content) = unlock(&path, &mut key)?;

        let lock = match policy {
            LockPolicy::TakeOver => Some(Lock::take(&path)?),
            LockPolicy::Respect => match Lock::acquire(&path)? {
                Outcome::Taken(lock) => Some(lock),
                Outcome::Held(holder) => return Err(VaultError::Locked(holder)),
                Outcome::Unwritable => None,
            },
        };
        let source = classify(&database, &path, lock.as_ref());

        Ok(Vault {
            database,
            path,
            key,
            source,
            stamp,
            content,
            changed: false,
            edits: 0,
            superseded: snapshot::Superseded::default(),
            _lock: lock,
        })
    }

    /// Makes a database where there is none, and opens it.
    ///
    /// Every refusal happens before a byte is written. There is no snapshot to
    /// fall back on at a path that has nothing at it yet, and the staged write
    /// underneath renames over whatever is at the target without a word, so a
    /// creation aimed at somebody's vault is the one way this application could
    /// destroy one. It is refused here rather than confirmed in a dialog.
    ///
    /// The vault is built from what was written rather than by opening the file
    /// again, because opening it means a second one-second derivation for a
    /// database this process already has in its hands.
    pub fn create(path: &Path, key: MasterKey, recipe: &Recipe<'_>) -> Result<Vault, VaultError> {
        if key.is_empty() {
            return Err(VaultError::EmptyMasterPassword);
        }
        preflight::acceptable(recipe.work)?;

        // A snapshot's name would open like any other database and refuse
        // every save for good, and a rescue copy's would be written over by the
        // next lock that had something to keep.
        if storage::reserved(path) {
            return Err(VaultError::ReservedName);
        }
        // The parent has to be there: a staged write puts its temporary file
        // beside the target, and a directory that is not there is a clearer
        // answer than a failed rename.
        let parent = crate::storage::parent_of(path)?;
        if !parent.is_dir() {
            return Err(VaultError::DatabaseGone);
        }
        // A copy a lock left at this name is all there is of the vault it was
        // taken from, and it is put back from that name's unlock screen. A
        // vault made over its name would adopt it as its own unsaved work.
        if unsaved::found(path)?.is_some() {
            return Err(VaultError::CopyBeside);
        }

        // The name is taken rather than asked about. Asking and then writing
        // leaves the whole of key derivation between the two, and a database
        // that arrived in that second would be renamed away with nothing behind
        // it - which is the one way this application can destroy a vault.
        atomic::reserve(path).map_err(|error| match error.kind() {
            std::io::ErrorKind::AlreadyExists => VaultError::DatabaseExists,
            _ => VaultError::Io(error),
        })?;

        // From here the empty file at `path` is Coffer's, and every way out
        // that is not a vault has to take it back off the disk. A creation that
        // failed and left a file behind would refuse every retry as a database
        // that is already there.
        match fill(path, key, recipe) {
            Ok(vault) => Ok(vault),
            Err(error) => {
                let _ = std::fs::remove_file(path);
                Err(error)
            }
        }
    }

    /// Where the database lives, after symbolic links have been followed.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The database's groups, as a tree.
    ///
    /// The value returned is the root group, which every KeePass database has
    /// and which holds everything else. Its sections are what Coffer calls
    /// projects. Previous versions of an entry are not in it: they belong to
    /// their entry and never appear in a tree, a list or a search.
    pub fn tree(&self) -> Project {
        let bin = Bin::of(&self.database);
        project_of(
            &self.database,
            &bin,
            templates::group(&self.database),
            self.database.root(),
            Standing::Outside,
        )
    }

    /// One entry's metadata.
    pub fn entry(&self, id: EntryId) -> Option<Entry> {
        let entry = self.database.entry(id)?;
        let group = entry.parent().id();
        let (binned, deletion) = self.placed(&entry, group);
        Some(entry_of(&entry, group, binned, deletion))
    }

    /// Whether an entry held by `group` is in the recycle bin, and what
    /// deleting it would do.
    fn placed(&self, entry: &EntryRef<'_>, group: GroupId) -> (Option<Binned>, Deletion) {
        let bin = Bin::of(&self.database);
        let holder = bin.standing(&self.database, group);
        (
            bin.binned(
                &self.database,
                holder,
                &entry.times,
                entry.previous_parent().map(|previous| previous.id()),
            ),
            bin.deletion(holder),
        )
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

        versions.sort_by_key(|version| history::age(version.modified));
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
    ///
    /// A KDBX 3 database is refused. The reader Coffer is built on collapses
    /// every attachment in one of those onto a single file, so the names are
    /// the file's and the bytes behind them are somebody else's: an entry whose
    /// row says `id_ed25519` would hand over another entry's private key.
    /// Refusing to save one was never enough - it has to refuse to read one out
    /// as well.
    pub fn attachment(&self, id: EntryId, name: &str) -> Result<SecretValue, VaultError> {
        if !self.files_readable() {
            return Err(VaultError::UnreadableAttachments);
        }

        let entry = self.database.entry(id).ok_or(VaultError::NoSuchEntry)?;
        let attachment = entry
            .attachment_by_name(name)
            .ok_or(VaultError::NoSuchAttachment)?;
        Ok(SecretValue::new(attachment.data.get().clone()))
    }

    /// Sets a field, keeping the entry's previous state as a version.
    ///
    /// An edit that leaves the entry the same as it was writes no version and
    /// does not move the modification time.
    ///
    /// Whether the value is stored protected is the field's own when the entry
    /// already has it: [`NewValue`] decides only for a field this makes. The
    /// screen says how a field is protected with every value it writes, and
    /// what it says is what it read before the reader pressed anything - so a
    /// value written on the way out of a field, landing after the press that
    /// hid that field (see [`Vault::set_protection`]), would otherwise put the
    /// value back into the file as plain text. Protection changes in one place.
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

        let held = self
            .database
            .entry(id)
            .and_then(|entry| entry.fields.get(field).map(Value::is_protected));
        let protect = held.unwrap_or(matches!(value, NewValue::Protected(_)));

        let field = field.to_owned();
        let edited = history::edit(&mut self.database, id, move |entry| {
            let written = value.into_text();
            if protect {
                entry.set(field, Value::protected(written.to_string()));
            } else {
                entry.set(field, Value::unprotected(written.to_string()));
            }
        });

        if edited {
            self.touched();
            Ok(())
        } else {
            Err(VaultError::NoSuchEntry)
        }
    }

    /// Writes text the reader was still typing into a field when the vault had
    /// to go, and says where it went, if anywhere.
    ///
    /// On the terms of [`Vault::set_field`], so the entry's previous state is
    /// kept as a version, and narrower, because nobody is looking when it
    /// happens. The field has to be one the entry still has, or one of the five
    /// every entry is drawn with: a field of the reader's own that was removed
    /// while its text was on the way is not made again under their feet. And
    /// text that is what the field already holds writes nothing at all,
    /// whatever protection the draft names - a field the entry has keeps its
    /// own (see [`Vault::set_field`]) - so a vault that heard only that has
    /// nothing to save.
    ///
    /// A new value typed [`Typing::Beside`] never goes over a value. The
    /// reader had not saved it, and a lock that wrote half a new password over
    /// the real one - or the wrong one pasted while the right one was being
    /// fetched - handed them a password that opens nothing. It goes into the
    /// field only when the field holds nothing; otherwise into a new protected
    /// field of the entry, named after the one it was typed for with
    /// "(typed before locking)" after it and numbered past any the entry
    /// already has, and the value it was typed for stays as it was. Nothing
    /// typed there is no new value at all, and writes nothing.
    pub fn set_typed(
        &mut self,
        id: EntryId,
        field: &str,
        value: NewValue,
        typing: Typing,
    ) -> Result<Written, VaultError> {
        let text = match &value {
            NewValue::Open(written) => written.as_str(),
            NewValue::Protected(written) => written.as_str(),
        };

        let entry = self.database.entry(id).ok_or(VaultError::NoSuchEntry)?;
        let held = entry.fields.get(field);
        // The text alone: a field the entry has keeps its own protection
        // whatever the draft says (see `set_field`).
        let unchanged = match held {
            Some(held) => held.get() == text,
            None if fields::Standard::of(field).is_some() => text.is_empty(),
            None => return Err(VaultError::NoSuchField),
        };
        let beside = typing == Typing::Beside;
        let nothing_new =
            beside && (text.is_empty() || held.is_some_and(|held| held.get() == text));
        if unchanged || nothing_new {
            return Ok(Written::Nothing);
        }

        if beside && held.is_some_and(|held| !held.get().is_empty()) {
            let names: Vec<&str> = entry.fields.keys().map(String::as_str).collect();
            let kept = clash::beside(&format!("{field} {TYPED_BEFORE_LOCKING}"), &names);
            self.set_field(id, &kept, NewValue::Protected(value.into_text()))?;
            Ok(Written::Beside)
        } else {
            self.set_field(id, field, value)?;
            Ok(Written::Into)
        }
    }

    /// Whether the file on disk is still the one this vault was opened from.
    pub fn external_change(&self) -> Result<Change, VaultError> {
        Ok(watch::since(&self.path, self.stamp, self.content)?)
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
        let mut written = 0;
        let staged = atomic::stage::<VaultError, _>(&self.path, |writer: &mut dyn Write| {
            written = encrypt(&self.database, &self.key, writer)?;
            Ok(())
        })?;

        // The whole database is read into memory to be opened, so a file past
        // the ceiling is a file nobody can open again - not Coffer, and not the
        // reader who put a hundred files in it one at a time. It is measured
        // here, where the database it would replace is still untouched and the
        // temporary file goes when this returns.
        if written > MAX_DATABASE_BYTES {
            return Err(VaultError::TooLarge);
        }

        snapshot::rotate(&self.path)?;

        // The snapshot is a hard link, which changes the database's change
        // time. That change is Coffer's own, so it is recorded now: a vault
        // that mistook its own snapshot for somebody else's edit could never
        // save again, and every unsaved edit would be stranded.
        //
        // There is nothing to record when the file is not there, which is the
        // one case that reaches here: somebody deleted the database and the
        // reader asked to write it back anyway. The rename below puts it back,
        // and the stamp taken after it is the one that counts.
        if let Ok(stamp) = Stamp::of(&self.path) {
            self.stamp = stamp;
            self.content = self.on_disk();
        }

        staged.commit()?;

        self.stamp = Stamp::of(&self.path)?;
        self.content = self.on_disk();
        self.changed = false;
        Ok(())
    }

    /// Everything a write settles in the database before any bytes leave it.
    ///
    /// Settling prunes and re-sorts histories, which moves the position of
    /// every previous version it touches, so it counts as an edit whether or
    /// not the write that follows goes through.
    fn prepare(&mut self) -> Result<(), VaultError> {
        self.edits += 1;
        settle(&mut self.database)
    }

    /// Marks the tree as holding a change the file has not got.
    fn touched(&mut self) {
        self.changed = true;
        self.edits += 1;
    }

    /// A number that moves every time the tree changes and at no other time:
    /// on every change, every save, which settles the histories, and every
    /// reload.
    ///
    /// What a caller compares to tell whether something read from the tree,
    /// such as the position of a previous version, still means what it did. A
    /// change that was refused, or that found nothing to do, leaves it alone.
    pub fn edits(&self) -> u64 {
        self.edits
    }

    /// How many entries the vault holds. Previous versions are not entries and
    /// are not counted; the recycle bin's contents are, because they are still
    /// in the file.
    pub fn count(&self) -> usize {
        self.database.num_entries()
    }

    /// One previous version of an entry, read the same way the entry itself is.
    ///
    /// It says where the entry it belongs to stands. A version goes wherever
    /// its entry goes, into the bin and out of the file alike.
    pub fn version(&self, id: EntryId, index: usize) -> Option<Entry> {
        let entry = self.database.entry(id)?;
        let group = entry.parent().id();
        let (binned, deletion) = self.placed(&entry, group);
        let version = entry.historical(index)?;
        Some(entry_of(&version, group, binned, deletion))
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
            dated(&mut made.times);
            made.id()
        };

        self.touched();
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

        self.touched();
        Ok(())
    }

    /// Deletes a folder and everything in it.
    ///
    /// A database with a recycle bin gets the folder moved into it, and a
    /// folder that is already in the bin, one the bin is inside, or a database
    /// with no bin, gets it removed and recorded in `DeletedObjects` so that
    /// other clients do not bring it back. Which of the two happens is what
    /// [`Project::deletion`] said it would.
    ///
    /// `shown` is the deletion the reader was shown and agreed to. When the
    /// folder stands somewhere else by now - a folder around it went into the
    /// bin first, or a reload brought in a vault that keeps no bin - this one
    /// would do something they never chose, and is refused with
    /// [`VaultError::DeletionChanged`], changing nothing. A move to the bin
    /// must never turn into an erasure on the way.
    pub fn delete_group(&mut self, id: GroupId, shown: Deletion) -> Result<(), VaultError> {
        self.writable()?;
        if id == self.database.root().id() {
            return Err(VaultError::CannotMoveRoot);
        }
        if self.database.group(id).is_none() {
            return Err(VaultError::NoSuchGroup);
        }

        let bin = Bin::of(&self.database);
        match bin.group_deletion(id, bin.standing(&self.database, id)) {
            deletion if deletion != shown => Err(VaultError::DeletionChanged),
            Deletion::Forever => self.erase_group(id),
            Deletion::Bin => {
                let into = self.bin(bin.id());
                self.relocate_group(id, into)
            }
        }
    }

    /// Empties the recycle bin, if the database has one with anything in it.
    pub fn empty_recycle_bin(&mut self) -> Result<(), VaultError> {
        self.writable()?;
        let Some(bin) = Bin::of(&self.database).id() else {
            return Ok(());
        };

        // The folders directly inside it, not every folder under it: erasing one
        // takes everything below it, and coming back for a folder that has
        // already gone is an error about a folder nobody asked to delete.
        let sections: Vec<GroupId> = self.children_of(bin);
        let entries: Vec<EntryId> = self.entries_of(bin);

        // Everything that can go, goes, and the first thing that could not is
        // what the reader is told about. Each erasure is all or nothing on its
        // own, so what is left behind is whole entries rather than pieces.
        //
        // Stopping at the first refusal used to leave the bin half emptied with
        // nothing saying which half, and one entry anywhere in the vault holding
        // a file in place was enough to make the bin refuse for good.
        let mut refused: Option<VaultError> = None;
        for section in sections {
            if let Err(error) = self.erase_group(section) {
                refused.get_or_insert(error);
            }
        }
        for entry in entries {
            if let Err(error) = self.erase_entries(&[entry]) {
                refused.get_or_insert(error);
            }
        }

        match refused {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Takes a field off an entry.
    ///
    /// The version the removal writes is the only way the field comes back
    /// (see [`Vault::undo_removal`]). When the database's own limits would drop
    /// that version at the next save - no versions kept at all, or a size limit
    /// it does not fit - the removal is for good, and it is refused with
    /// [`VaultError::RemovalForGood`] unless `forever` says the reader agreed
    /// to that. Which it is comes from the rule the save prunes by.
    pub fn remove_field(
        &mut self,
        id: EntryId,
        field: &str,
        forever: bool,
    ) -> Result<(), VaultError> {
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
        if !forever && !history::outlasts_save(&self.database, id) {
            return Err(VaultError::RemovalForGood);
        }

        let field = field.to_owned();
        history::edit(&mut self.database, id, move |entry| {
            entry.fields.remove(&field);
            entry.times.last_modification = Some(Times::now());
        });

        self.touched();
        Ok(())
    }

    /// Hides a field of the reader's own behind the database's protection, or
    /// stops hiding it.
    ///
    /// The value moves from one kind of storage to the other here, inside the
    /// vault: the window holds no protected value to write back, and asking it
    /// for one would be a reveal nobody asked for. The entry's previous state
    /// is kept as a version, the way any edit's is. A field already stored the
    /// way that is asked for writes nothing.
    ///
    /// The five fields every entry has are refused. Their protection is the
    /// database's, set for every entry at once, and the password's is what
    /// keeps it out of the window.
    pub fn set_protection(
        &mut self,
        id: EntryId,
        field: &str,
        protect: bool,
    ) -> Result<(), VaultError> {
        self.writable()?;
        if fields::Standard::of(field).is_some() {
            return Err(VaultError::StandardField);
        }
        let entry = self.database.entry(id).ok_or(VaultError::NoSuchEntry)?;
        let held = entry.fields.get(field).ok_or(VaultError::NoSuchField)?;
        if held.is_protected() == protect {
            return Ok(());
        }

        let field = field.to_owned();
        history::edit(&mut self.database, id, move |entry| {
            if let Some(value) = entry.fields.remove(&field) {
                entry.fields.insert(field, rewrapped(value, protect));
            }
            entry.times.last_modification = Some(Times::now());
        });

        self.touched();
        Ok(())
    }

    /// Gives a field of the reader's own another name.
    ///
    /// The value goes with the name, moved inside the vault under the
    /// protection it had, so a protected value never crosses to the window to
    /// be written again under the new one. One version keeps the entry as it
    /// was, and putting that version back puts the old name back.
    ///
    /// A name the entry already gives a field is refused rather than written
    /// over, and so is one of the five every entry keeps, whether or not this
    /// entry has it yet: a field renamed `Password` would be the password. So
    /// is no name at all. The same name again writes nothing.
    pub fn rename_field(&mut self, id: EntryId, from: &str, to: &str) -> Result<(), VaultError> {
        self.writable()?;
        if fields::Standard::of(from).is_some() {
            return Err(VaultError::StandardField);
        }
        if to.is_empty() {
            return Err(VaultError::UnnamedField);
        }
        text::writable(to)?;

        let entry = self.database.entry(id).ok_or(VaultError::NoSuchEntry)?;
        if !entry.fields.contains_key(from) {
            return Err(VaultError::NoSuchField);
        }
        if from == to {
            return Ok(());
        }
        if fields::Standard::of(to).is_some() || entry.fields.contains_key(to) {
            return Err(VaultError::FieldNameTaken);
        }

        let (from, to) = (from.to_owned(), to.to_owned());
        history::edit(&mut self.database, id, move |entry| {
            if let Some(value) = entry.fields.remove(&from) {
                entry.fields.insert(to, value);
            }
            entry.times.last_modification = Some(Times::now());
        });

        self.touched();
        Ok(())
    }

    /// Replaces an entry's tags.
    ///
    /// A tag the format would split in two, trim or drop is refused rather
    /// than written and silently changed (see `text::tag`).
    pub fn set_tags(&mut self, id: EntryId, tags: Vec<String>) -> Result<(), VaultError> {
        self.writable()?;
        for tag in &tags {
            text::tag(tag)?;
        }
        if self.database.entry(id).is_none() {
            return Err(VaultError::NoSuchEntry);
        }

        history::edit(&mut self.database, id, move |entry| {
            entry.tags = tags;
            entry.times.last_modification = Some(Times::now());
        });

        self.touched();
        Ok(())
    }

    /// Takes a folder out of the recycle bin with everything in it, on the
    /// same terms as [`Vault::put_back_entries`] takes an entry. The bin itself
    /// is not in the bin, and cannot be put back.
    pub fn put_back_group(&mut self, id: GroupId) -> Result<(), VaultError> {
        self.writable()?;
        let into = {
            let group = self.database.group(id).ok_or(VaultError::NoSuchGroup)?;
            let bin = Bin::of(&self.database);
            let holder = group.parent().map_or(Standing::Outside, |parent| {
                bin.standing(&self.database, parent.id())
            });
            self.back(bin.binned(
                &self.database,
                holder,
                &group.times,
                group.previous_parent().map(|previous| previous.id()),
            ))?
        };
        self.relocate_group(id, into)
    }

    /// Where putting something back takes it: the folder it came from, or the
    /// top of the vault when that is nowhere to go.
    fn back(&self, binned: Option<Binned>) -> Result<GroupId, VaultError> {
        let binned = binned.ok_or(VaultError::NotInRecycleBin)?;
        Ok(binned.from.unwrap_or_else(|| self.database.root().id()))
    }

    /// Whether something may be put into `into` by any way but deleting it: a
    /// folder that is there, and neither the recycle bin nor anything inside
    /// it. Deleting is the one way into the bin, because it is the one that
    /// says first whether it can be undone.
    fn destination(&self, bin: &Bin, into: GroupId) -> Result<(), VaultError> {
        if self.database.group(into).is_none() {
            return Err(VaultError::NoSuchGroup);
        }
        if bin.standing(&self.database, into).binned() {
            return Err(VaultError::IntoRecycleBin);
        }
        Ok(())
    }

    /// Moves an entry into another folder, which is not an edit: no version,
    /// and the modification time stays where it was.
    ///
    /// No question about where: every caller has decided that already, the
    /// deletion by the bin's rule, putting back by where the thing came from,
    /// a move between folders by [`Vault::move_entries`]'s checks, and taking
    /// one back by [`Vault::move_entries_back`]'s, which go by what the file
    /// says of the move rather than by [`Vault::destination`].
    fn relocate_entry(&mut self, id: EntryId, into: GroupId) -> Result<(), VaultError> {
        let mut entry = self.database.entry_mut(id).ok_or(VaultError::NoSuchEntry)?;
        entry.move_to(into).map_err(|_| VaultError::NoSuchGroup)?;
        entry.times.location_changed = Some(Times::now());
        self.touched();
        Ok(())
    }

    /// Puts a file on an entry, unless the entry already gives its name to
    /// another.
    ///
    /// A taken name changes nothing. The answer says how large the file there
    /// is and what the new one could be called beside it, and the way on is
    /// the reader's to choose, through [`Vault::keep_both`] or
    /// [`Vault::replace_attachment`]: a file replaced here is gone for good,
    /// and the name every phone gives every scan is not a reason to think
    /// somebody meant it.
    ///
    /// The bytes are borrowed, so that a caller told the name is taken still
    /// has them to offer again with the answer.
    ///
    /// No previous version is written; see [`Vault::remove_attachment`] for why
    /// the files on an entry are not part of its history.
    pub fn add_attachment(
        &mut self,
        id: EntryId,
        name: &str,
        data: &[u8],
    ) -> Result<Attached, VaultError> {
        self.offerable(id, name, data)?;
        if let Some(clash) = clash::clash(&self.database, id, name) {
            return Ok(Attached::Taken(clash));
        }
        self.put(id, name, data)?;
        Ok(Attached::Added)
    }

    /// Puts a file on an entry beside the one that already has its name.
    ///
    /// It goes under the name [`Attached::Taken`] offered, worked out again
    /// from the entry as it is now: a name that has come free since the
    /// question was asked is simply the file's own, and one taken since is
    /// passed over for the next.
    pub fn keep_both(&mut self, id: EntryId, name: &str, data: &[u8]) -> Result<(), VaultError> {
        self.offerable(id, name, data)?;
        let free = clash::free(&self.database, id, name);
        self.put(id, &free, data)
    }

    /// Puts a file on an entry in place of the one that has its name.
    ///
    /// The one there goes the way a removal takes it, and is refused for the
    /// same reasons: a file previous versions hold in place stays, and so does
    /// everything else, until the reader clears those versions.
    pub fn replace_attachment(
        &mut self,
        id: EntryId,
        name: &str,
        data: &[u8],
    ) -> Result<(), VaultError> {
        self.offerable(id, name, data)?;

        // Replacing a file means removing the old one first. The library's own
        // replacement takes the bytes of the file it replaces out from under
        // every previous version that still names them, and leaves a hole in
        // the pool behind it.
        if attachment::held(&self.database, id, name) {
            attachment::detach(&mut self.database, id, name)?;
        }
        self.put(id, name, data)
    }

    /// Whether a file can go on an entry at all, whatever its name turns out
    /// to be. Asked before anything changes, by every way a file goes on.
    fn offerable(&self, id: EntryId, name: &str, data: &[u8]) -> Result<(), VaultError> {
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
        Ok(())
    }

    /// Adds a file under a name the entry does not use. Only ever that: the
    /// library's answer to a name the entry does use is to drop the file there.
    fn put(&mut self, id: EntryId, name: &str, data: &[u8]) -> Result<(), VaultError> {
        // Added as a protected value: the flag is what KeePassXC writes, and it
        // is what keeps the bytes in a buffer that wipes itself and prints
        // `[REDACTED]` rather than the file.
        let value = Value::protected(data.to_vec());
        let mut entry = self.database.entry_mut(id).ok_or(VaultError::NoSuchEntry)?;
        entry.add_attachment(name.to_owned(), value);
        entry.times.last_modification = Some(Times::now());

        self.touched();
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

        self.touched();
        Ok(())
    }

    /// Takes a file off an entry, dropping the previous versions that are
    /// holding it back.
    ///
    /// The versions go only if the file then goes. A removal can be refused for
    /// more than one reason, and a reader who asked to be rid of a file must
    /// not be left having lost the history of the entry and still having the
    /// file.
    pub fn remove_attachment_and_versions(
        &mut self,
        id: EntryId,
        name: &str,
    ) -> Result<(), VaultError> {
        self.writable()?;
        if self.database.entry(id).is_none() {
            return Err(VaultError::NoSuchEntry);
        }

        // Every entry whose versions stand in the way, not only this one. A file
        // is held in place by the numbers around it as much as by its own, so
        // clearing this entry's history and no other answered a reader who asked
        // to be rid of a file with the same refusal and one less history than
        // they started with.
        //
        // Round by round, because a refusal names the first thing in the way and
        // there can be another behind it. Each round clears an entry no round
        // before it cleared, so there are at most as many rounds as there are
        // entries.
        let mut kept: Vec<(EntryId, Option<History>)> = Vec::new();
        let outcome = self.clear_the_way(id, name, &mut kept);

        match outcome {
            Ok(()) => {
                if let Some(mut entry) = self.database.entry_mut(id) {
                    entry.times.last_modification = Some(Times::now());
                }
                self.touched();
                Ok(())
            }
            Err(refused) => {
                // Whatever went wrong and however far it got, every history goes
                // back exactly as it was: a removal that was refused must cost
                // nothing at all.
                for (entry, history) in kept {
                    if let Some(mut found) = self.database.entry_mut(entry) {
                        found.history = history;
                    }
                }
                Err(refused)
            }
        }
    }

    /// Drops the previous versions standing between a file and going, then takes
    /// the file off.
    ///
    /// Only the versions that are actually holding something: the entry's whole
    /// history is not cleared, and neither is anybody else's. A file is held in
    /// place by the numbers around it as much as by its own, so the versions in
    /// the way can belong to an entry the reader is not looking at, and taking
    /// months of their history to move one file is not a trade anybody would
    /// make knowingly.
    ///
    /// Round by round, because a refusal names the first thing in the way and
    /// there can be another behind it. Every round drops at least one version,
    /// so it ends.
    ///
    /// Each entry's history is written down in `kept` before the first version
    /// is taken out of it, so that the caller can put back whatever this got
    /// through before it failed.
    fn clear_the_way(
        &mut self,
        id: EntryId,
        name: &str,
        kept: &mut Vec<(EntryId, Option<History>)>,
    ) -> Result<(), VaultError> {
        loop {
            let mut holding = attachment::blocking(&self.database, id, name);
            if holding.is_empty() {
                return attachment::detach(&mut self.database, id, name);
            }

            for (entry, _) in &holding {
                if kept.iter().any(|(seen, _)| seen == entry) {
                    continue;
                }
                let history = self
                    .database
                    .entry(*entry)
                    .and_then(|found| found.history.clone());
                kept.push((*entry, history));
            }

            // Highest position first, so that dropping one does not move the
            // next one out from under its own index.
            holding.sort_by_key(|(entry, index)| (entry.to_string(), std::cmp::Reverse(*index)));
            for (entry, index) in holding {
                history::forget(&mut self.database, entry, index)?;
            }
        }
    }

    /// Makes a previous version the current state of the entry, keeping what it
    /// replaces as a version of its own.
    pub fn restore_version(&mut self, id: EntryId, index: usize) -> Result<(), VaultError> {
        self.writable()?;
        history::restore(&mut self.database, id, index)?;
        self.touched();
        Ok(())
    }

    /// Takes back the removal of a field: restores the version the removal
    /// wrote, which puts back that field and nothing else.
    ///
    /// Only while the removal is the last thing that happened to the entry.
    /// Otherwise - a change made since, a save that pruned the version, a
    /// version another client dated later - whatever is newest would take back
    /// more than the field, and the undo is refused with
    /// [`VaultError::RemovalSuperseded`] and does nothing. The question and the
    /// restore are one call, so that nothing can land between them.
    pub fn undo_removal(&mut self, id: EntryId, field: &str) -> Result<(), VaultError> {
        self.writable()?;
        if self.database.entry(id).is_none() {
            return Err(VaultError::NoSuchEntry);
        }
        let index = history::before_removal(&self.database, id, field)
            .ok_or(VaultError::RemovalSuperseded)?;
        history::restore(&mut self.database, id, index)?;
        self.touched();
        Ok(())
    }

    /// Drops one previous version.
    pub fn delete_version(&mut self, id: EntryId, index: usize) -> Result<(), VaultError> {
        self.writable()?;
        history::forget(&mut self.database, id, index)?;
        self.touched();
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
        self.touched();
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

    /// Settles the database and encrypts the whole of it into `writer`, for a
    /// write that is not a save of this vault's own file: a copy elsewhere, or
    /// the one a lock leaves beside the vault. One past the ceiling is refused,
    /// because nobody could open it again.
    fn encrypt_whole(&mut self, writer: &mut dyn Write) -> Result<(), VaultError> {
        self.prepare()?;
        if encrypt(&self.database, &self.key, writer)? > MAX_DATABASE_BYTES {
            return Err(VaultError::TooLarge);
        }
        Ok(())
    }

    /// Writes out whatever the file has not got, on the way to being wiped.
    ///
    /// Called by locking and by nothing else. Locking destroys the decrypted
    /// tree, and a vault is dirty exactly when saving is the thing that failed,
    /// so without this an idle timer or a closed lid is a whole session's work
    /// gone with no message anywhere.
    ///
    /// The ordinary save is tried first, because a save that goes through
    /// leaves the work in the file the reader thinks it is in, and because it is
    /// the cheap order: an external change and a file that has gone are both
    /// decided before any key is derived. What it will not take goes beside the
    /// database, under the same credentials, through the same staged write.
    ///
    /// Nothing here can stop the lock. A rescue that fails answers [`Rescue::Lost`]
    /// and the caller wipes the tree regardless: a vault left unlocked because
    /// it had unsaved work would be the whole of the locking gone.
    pub fn rescue(&mut self) -> Rescue {
        if !self.changed {
            return Rescue::Nothing;
        }

        if self.save().is_ok() {
            return Rescue::Saved;
        }

        let Ok(beside) = unsaved::beside(&self.path) else {
            return Rescue::Lost;
        };

        // Over the copy an earlier lock with the same trouble left, which is
        // the one name a copy is written over: there is one copy, the newest.
        let kept = atomic::write_atomic::<VaultError, _>(&beside, |writer: &mut dyn Write| {
            self.encrypt_whole(writer)
        });
        match kept {
            Ok(()) => Rescue::Kept,
            Err(_) => Rescue::Lost,
        }
    }

    /// Makes this vault, opened from the copy a lock left, the vault the copy
    /// was taken from.
    ///
    /// What is in it goes over the vault's file the way [`Vault::save_over`]
    /// goes over a file somebody else wrote: the file as it stands is pushed
    /// into the snapshots first, so what this replaces is `<vault>.1.bak` and
    /// opens again with the same password. The copy is taken off the disk only
    /// once the vault's file holds this; a refusal or a failure anywhere before
    /// that leaves both files as they were, and this vault still the copy.
    ///
    /// `seen` is how the vault's file stood when the reader was told, which is
    /// what they decided on. A file that stands otherwise now - another
    /// client's save, a vault that came back or went - is
    /// [`VaultError::VaultFileChanged`] with nothing written, because the write
    /// would push a change nobody showed them into the snapshots; so is a
    /// reader who was never told, with nothing to have decided on. Asked once
    /// the vault's lock is held, so another Coffer cannot write it in between.
    ///
    /// From then on this is the vault: its path, the lock beside it, and every
    /// save after this one. The copy's own lock goes with the copy, and so do
    /// the snapshots its saves rotated beside it (see [`unsaved::retire`]).
    pub fn promote(&mut self, seen: Option<Seen>) -> Result<(), VaultError> {
        let vault = unsaved::taken_from(&self.path).ok_or(VaultError::NotACopy)?;
        self.writable()?;
        let lock = claim(&vault)?;
        if seen != Some(Seen::of(&vault)) {
            return Err(VaultError::VaultFileChanged);
        }

        let copy = self.take_over(vault, lock)?;

        // A copy that will not go is offered again beside a vault that now
        // holds the same thing, which loses nothing and is said on the unlock
        // screen the next time it is drawn.
        let _ = std::fs::remove_file(&copy);
        unsaved::retire(&copy);
        Ok(())
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
                .map(|(database, _)| database.num_entries()),
        }
    }

    /// The bytes on disk, hashed, or nothing at all when they cannot be read.
    ///
    /// A digest that could not be taken is a digest that matches nothing, which
    /// makes the next save ask rather than assume - the same answer the stamp
    /// gives when it cannot read the file either.
    fn on_disk(&self) -> Content {
        std::fs::read(&self.path)
            .map(|bytes| watch::digest(&bytes))
            .unwrap_or_default()
    }

    /// Throws away everything in this vault and reads the file again.
    pub fn reload(&mut self) -> Result<(), VaultError> {
        let stamp = Stamp::of(&self.path)?;
        let (database, content) = read(&self.path, &self.key)?;

        self.source = classify(&database, &self.path, self._lock.as_ref());
        self.database = database;
        self.stamp = stamp;
        self.content = content;
        self.changed = false;
        self.edits += 1;
        Ok(())
    }

    /// Whether this database may be written back at all, and why not when it
    /// may not.
    fn writable(&self) -> Result<(), VaultError> {
        match self.source {
            Some(why) => Err(why.into()),
            None => Ok(()),
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

    /// The recycle bin a deletion goes to: the one the database has, or one
    /// made now when it asks for one and has none, which is what KeePassXC does
    /// on the first deletion.
    fn bin(&mut self, found: Option<GroupId>) -> GroupId {
        if let Some(bin) = found {
            return bin;
        }

        let made = {
            let mut root = self.database.root_mut();
            let mut made = root.add_group();
            made.name = RECYCLE_BIN.to_owned();
            dated(&mut made.times);
            // The bin is not part of the vault a search runs over, which is the
            // flag KeePassXC writes on the group it makes.
            made.enable_searching = Some(false);
            made.id()
        };

        self.database.meta.recyclebin_enabled = Some(true);
        self.database.meta.recyclebin_uuid = Some(made.uuid());
        self.database.meta.recyclebin_changed = Some(Times::now());
        self.touched();
        made
    }

    /// Moves a folder, with everything in it, into another folder, on the
    /// terms [`Vault::relocate_entry`] moves an entry. The library's own walk
    /// refuses the top of the vault and a folder going inside itself.
    fn relocate_group(&mut self, id: GroupId, into: GroupId) -> Result<(), VaultError> {
        use keepass::db::MoveGroupError;

        let mut group = self.database.group_mut(id).ok_or(VaultError::NoSuchGroup)?;
        group.move_to(into).map_err(|error| match error {
            MoveGroupError::CannotMoveRoot => VaultError::CannotMoveRoot,
            MoveGroupError::WouldCreateCycle => VaultError::CannotMoveIntoItself,
            _ => VaultError::NoSuchGroup,
        })?;
        group.times.location_changed = Some(Times::now());

        self.touched();
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
        self.touched();
        Ok(())
    }

    /// Takes entries out of the file for good, every one of them or none.
    ///
    /// Their files go first, by the pool's rules and for all of them at once:
    /// [`attachment::detach_entries`] works out where every file goes before it
    /// writes anything, so a version standing in the way of one entry's file
    /// refuses them all with the pool as it was. Nothing after it can refuse:
    /// every caller found each entry first, and names each one once.
    fn erase_entries(&mut self, ids: &[EntryId]) -> Result<(), VaultError> {
        attachment::detach_entries(&mut self.database, ids)?;

        for &id in ids {
            self.database
                .entry_mut(id)
                .ok_or(VaultError::NoSuchEntry)?
                .track_changes()
                .remove();
        }

        self.touched();
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

    /// The folders directly inside this one.
    fn children_of(&self, id: GroupId) -> Vec<GroupId> {
        self.database
            .group(id)
            .map(|group| group.groups().map(|section| section.id()).collect())
            .unwrap_or_default()
    }

    /// The entries this folder holds directly.
    fn entries_of(&self, id: GroupId) -> Vec<EntryId> {
        self.database
            .group(id)
            .map(|group| group.entries().map(|entry| entry.id()).collect())
            .unwrap_or_default()
    }
}

/// Fills a name a creation has already taken.
///
/// Split out so that the one caller can take the reserved name back off the
/// disk however this ends. Every failure in here happens with an empty file at
/// `path` and nothing else disturbed.
fn fill(path: &Path, key: MasterKey, recipe: &Recipe<'_>) -> Result<Vault, VaultError> {
    let mut database = Held(blank::database(recipe.name, recipe.work)?);
    settle(&mut database)?;

    // The name exists now, so it can be resolved before anything is written -
    // and the lock is taken before the contents, so a creation that cannot have
    // the database never wrote one.
    let path = path.canonicalize()?;
    let lock = match Lock::acquire(&path)? {
        Outcome::Taken(lock) => lock,
        Outcome::Held(holder) => return Err(VaultError::Locked(holder)),
        // A vault is made where the reader can keep it. Somewhere that will not
        // take the lock file will not take the database either, and the write
        // below would be the one to say so - after a calibrated second of key
        // derivation, and in an errno.
        Outcome::Unwritable => return Err(VaultError::ReadOnlyPlace),
    };

    let mut written = 0;
    atomic::write_atomic::<VaultError, _>(&path, |writer: &mut dyn Write| {
        written = encrypt(&database, &key, writer)?;
        if written > MAX_DATABASE_BYTES {
            return Err(VaultError::TooLarge);
        }
        Ok(())
    })?;

    let stamp = Stamp::of(&path)?;
    let content = std::fs::read(&path)
        .map(|bytes| watch::digest(&bytes))
        .unwrap_or_default();
    let source = classify(&database, &path, Some(&lock));

    Ok(Vault {
        database,
        path,
        key,
        source,
        stamp,
        content,
        changed: false,
        edits: 0,
        superseded: snapshot::Superseded::default(),
        _lock: Some(lock),
    })
}

/// Gives something newly made a date to go with the fact that it never expires.
///
/// The library leaves the date out when nothing expires, which the format
/// allows and no other client does: KeePass and KeePassXC both write one, and a
/// reader that finds none invents its own. A file that leaves a field for
/// somebody else to fill in says a different thing to every client that opens
/// it, and says a different thing twice to the same one.
fn dated(times: &mut Times) {
    if times.expiry.is_none() {
        times.expiry = times.creation.or_else(|| Some(Times::now()));
    }
}

/// A value moved into the other kind of storage with no copy of it left
/// behind. The text an open value held becomes the protected value's own, and
/// the box a protected value was held in wipes it as it goes.
fn rewrapped(value: Value<String>, protect: bool) -> Value<String> {
    match (protect, value) {
        (true, Value::Unprotected(text)) => Value::protected(text),
        (false, protected @ Value::Protected(_)) => Value::unprotected(protected.get().clone()),
        (_, unchanged) => unchanged,
    }
}

/// Everything a write settles in a database before any bytes leave it.
///
/// A free function rather than a method, because the database a creation writes
/// has no vault around it yet and has to go through exactly the same door: the
/// format version, the generator, the inner cipher and the history limits are
/// one rule each, and a second copy of any of them would drift the first time
/// one changed.
fn settle(database: &mut Database) -> Result<(), VaultError> {
    let limits = Limits::of(database);
    history::prune_all(database, limits);
    database.config.version = WRITTEN_VERSION;

    // Every KeePass client writes its own name here, so a file Coffer wrote
    // says so rather than going on claiming to be the work of whatever wrote
    // it last.
    database.meta.generator = Some(GENERATOR.to_owned());

    // A database that declares no inner cipher stores every protected value as
    // base64 plaintext inside the encrypted body. Writing that back would be
    // writing a weaker file than the one Coffer could write, and the choice of
    // inner cipher is not a field anybody can lose.
    if database.config.inner_cipher_config == InnerCipherConfig::Plain {
        database.config.inner_cipher_config = InnerCipherConfig::ChaCha20;
    }

    // The last check before the bytes go. A file the writer cannot place where
    // the reader will look for it is the one way this application hands
    // somebody else's password to the wrong entry, so the write does not
    // happen at all.
    if !attachment::unbroken(database) {
        return Err(VaultError::AttachmentOrder);
    }

    Ok(())
}

/// Writes the encrypted database out, and says how many bytes that took.
///
/// The count is the only way to know what a save would cost before it costs it:
/// the payload is compressed, so nothing about the tree in memory predicts the
/// size of the file.
fn encrypt(
    database: &Database,
    key: &MasterKey,
    writer: &mut dyn Write,
) -> Result<u64, VaultError> {
    struct Counted<'a> {
        inner: &'a mut dyn Write,
        written: u64,
    }

    impl Write for Counted<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let taken = self.inner.write(bytes)?;
            self.written += taken as u64;
            Ok(taken)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.inner.flush()
        }
    }

    let mut counted = Counted {
        inner: writer,
        written: 0,
    };
    database.save(&mut counted, key.to_database_key()?)?;
    counted.flush()?;
    Ok(counted.written)
}

/// Reads the database, trying the one other way its credentials could be put
/// together before giving up.
///
/// A vault whose owner chose a key file and no password at all is opened by the
/// file alone. Nothing in the header says so, and folding an empty password in
/// anyway fails inside the cipher, which is where a corrupt file fails too - so
/// the reader was told their vault was damaged and offered a snapshot, for a
/// file with nothing wrong with it.
///
/// The refusal that is reported is the first one. The second attempt is a guess
/// about one uncommon shape, and a guess that did not come off should not be
/// what the reader is told about their file.
fn unlock(path: &Path, key: &mut MasterKey) -> Result<(Held, Content), VaultError> {
    let refused = match read(path, key) {
        Ok(opened) => return Ok(opened),
        Err(error) => error,
    };

    if !could_be_the_key(&refused) || !key.stand_alone() {
        return Err(refused);
    }

    read(path, key).map_err(|_| refused)
}

/// Whether a refusal could be the credentials rather than the file.
///
/// A key that does not open a KDBX 4 database fails inside the cipher, and the
/// cipher cannot tell a key it was never given from bytes somebody corrupted.
/// So every report of damage is also a possible wrong key. A file that is not
/// there, or is not a database at all, is neither.
fn could_be_the_key(error: &VaultError) -> bool {
    matches!(
        error,
        VaultError::WrongCredentials
            | VaultError::DamagedHeader
            | VaultError::DamagedPayload
            | VaultError::DamagedContent
    )
}

fn read(path: &Path, key: &MasterKey) -> Result<(Held, Content), VaultError> {
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
    let content = watch::digest(&bytes);
    parse_without_dying(&bytes, database_key).map(|held| (Held(held), content))
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

/// What the database is, for the purpose of deciding whether it may be written
/// back: why not, or nothing for anything Coffer can write back as KDBX 4.1.
///
/// The format comes first and the place second, on purpose. A KDBX 3 database
/// carrying attachments is refused a read of those attachments as well as a
/// save, and that refusal has to survive the file being on a read-only medium:
/// the medium is why a save cannot go anywhere, the format is why the bytes
/// cannot be trusted, and only the second of those is about the data.
fn classify(database: &Database, path: &Path, lock: Option<&Lock>) -> Option<ReadOnly> {
    match database.config.version {
        DatabaseVersion::KDB(_) | DatabaseVersion::KDB2(_) => Some(ReadOnly::Kdb),
        DatabaseVersion::KDB3(_) if database.num_attachments() > 0 => {
            Some(ReadOnly::Kdbx3Attachments)
        }
        _ if snapshot::slot_of(path).is_some() => Some(ReadOnly::Snapshot),
        _ if lock.is_none() => Some(ReadOnly::Place),
        _ => None,
    }
}

/// Reads a group and everything in it, given where the group holding it stands
/// with respect to the recycle bin.
///
/// Where each group stands is worked out once, on the way down, and handed to
/// the entries in it: a vault of fifty thousand entries asks the question once
/// per folder rather than once per entry. Which group holds the templates is
/// worked out once for the whole tree, for the same reason.
fn project_of(
    database: &Database,
    bin: &Bin,
    templates: Option<GroupId>,
    group: GroupRef<'_>,
    holder: Standing,
) -> Project {
    let group_id = group.id();
    let standing = bin.enter(holder, group_id, &group.times);
    let deletion = bin.deletion(standing);

    Project {
        id: group_id,
        name: group.name.clone(),
        notes: group.notes.clone(),
        is_recycle_bin: Some(group_id) == bin.id(),
        is_templates: Some(group_id) == templates,
        binned: bin.binned(
            database,
            holder,
            &group.times,
            group.previous_parent().map(|previous| previous.id()),
        ),
        deletion: bin.group_deletion(group_id, standing),
        sections: group
            .groups()
            .map(|section| project_of(database, bin, templates, section, standing))
            .collect(),
        entries: group
            .entries()
            .map(|entry| {
                let binned = bin.binned(
                    database,
                    standing,
                    &entry.times,
                    entry.previous_parent().map(|previous| previous.id()),
                );
                entry_of(&entry, group_id, binned, deletion).summary()
            })
            .collect(),
    }
}

/// Reads an entry, or one previous version of one.
///
/// The group is passed in rather than read off the entry, because a previous
/// version carries the group it was in when it was written and that group may
/// be gone; asking a version for its parent is a way to bring the window down.
/// Where it stands with respect to the bin is passed in for the same reason: a
/// version is wherever its entry is.
fn entry_of(
    entry: &EntryRef<'_>,
    group: GroupId,
    binned: Option<Binned>,
    deletion: Deletion,
) -> Entry {
    let mut fields: Vec<Field> = entry
        .fields
        .iter()
        .map(|(name, value)| Field {
            name: name.clone(),
            value: match value {
                Value::Unprotected(text) => FieldValue::Open(text.clone()),
                Value::Protected(_) => FieldValue::Protected {
                    empty: value.get().is_empty(),
                    lines: model::in_lines(value.get()),
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
        binned,
        deletion,
    }
}
