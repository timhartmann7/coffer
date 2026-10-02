//! The one database this process has open, and everything the screens may ask
//! of it.
//!
//! Every decision about the database itself is made in `vault-core`. What is
//! here is the fact that a window has exactly one vault at a time, and that
//! locking means dropping it: the decrypted tree, the master password and the
//! lock file all go with it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use vault_core::kdf::Work;
use vault_core::model::{Entry, EntryId, Project};
use vault_core::storage::{OnDisk, Seen, unsaved};
use vault_core::{
    Attached, LockPolicy, MasterKey, Recipe, Rescue, SecretValue, Vault, VaultError, Written,
};
use zeroize::Zeroizing;

use crate::autolock::Reason;
use crate::drafts::{Drafts, Over, Typed};
use crate::error::Failure;
use crate::offered::Waiting;
use crate::recent;
use slot::Slot;

pub struct Session {
    held: Mutex<Held>,
    /// Whether a vault is open, readable without waiting for `held`.
    ///
    /// A save holds that lock for a key derivation and the encryption of the
    /// whole file, which on a vault carrying documents is seconds. The menu bar
    /// asks whether there is anything to lock every time what the window offers
    /// changes, and a question stuck behind a save left an item grey that
    /// applied - and AppKit drops the key of a grey item. Shared with the
    /// [`Slot`] the vault is held in, which is the one thing that changes it.
    unlocked: Arc<AtomicBool>,
    /// Where the vault that opened is written down for the next launch, when
    /// this Mac gave Coffer a configuration directory to write it in.
    remembering: Option<PathBuf>,
}

struct Held {
    /// What the next unlock will open. Chosen before a password is asked for,
    /// which is why it outlives the vault.
    database: Option<PathBuf>,
    /// Why the vault that was open is not open any more, when it is worth
    /// saying. Cleared by the next unlock.
    locked_by: Option<Reason>,
    /// Whether the last lock found work the file had not got and could not put
    /// it anywhere at all. There is no file to point the reader at, which is why
    /// this is a flag and not a path. Cleared with `locked_by`.
    lost: bool,
    /// Where the last lock put text the reader was still typing, when it wrote
    /// it into the vault and saved it where the reader believes it is:
    /// [`Written::Beside`] when any of it was a new value kept beside the old
    /// one. Where and nothing more: after a lock nothing of the vault may be
    /// left to name the entry it went into. Cleared with `locked_by`.
    typed: Written,
    /// The key file the next unlock will use alongside the password, when the
    /// database asks for one.
    ///
    /// Beside `database` and settled before the password is asked for, for the
    /// same reason `making` is: the password arrives as the whole body of its
    /// message and carries no named arguments beside it. It survives a lock,
    /// because the rebuilt window is asking for the same vault, and it goes
    /// when the session is pointed at a different one.
    key_file: Option<PathBuf>,
    /// Where a vault being made will go, if one is. Kept apart from
    /// `database`, which is the file the unlock screen offers to open: there is
    /// usually nothing at this path yet, and pointing the unlock screen at it
    /// would be offering to open a file that does not exist.
    making: Option<PathBuf>,
    /// The vault in Coffer's own folder the first-run screen was last told
    /// about. Kept for the same reason `making` is: pressing the offer sends
    /// nothing, and what it opens has to be the file the screen named rather
    /// than whatever a second search turns up.
    found: Option<PathBuf>,
    /// What a one-second unlock costs on this machine.
    ///
    /// A property of the Mac and not of the file, which is why it is here and
    /// not beside the path. The screen measures as it opens and asks where to
    /// put the vault afterwards, so a measurement kept beside the place would
    /// be thrown away by the question that comes next - and the vault could
    /// never be made at all.
    measured: Option<Work>,
    /// The open database, and the file waiting on the reader's word about one
    /// of its entries. Dropping it wipes the decrypted tree and the file, and
    /// removes the lock file beside the database.
    open: Slot,
    /// Bumped every time the session is pointed somewhere else or emptied. Key
    /// derivation takes a second and does not hold the lock, so an unlock that
    /// started before such a change must not finish over it.
    generation: u64,
    /// Moved every time the open vault changes, and when a vault lands.
    ///
    /// A previous version is addressed by its position in its entry's history,
    /// and nearly everything moves those positions: an edit adds one, a drop or
    /// a restore renumbers the rest, a save prunes and re-sorts every entry's,
    /// and a reload or another vault replaces them all. Rust answers one command
    /// at a time and a save holds it for a key derivation, so a press made
    /// behind a save used to reach the vault after the save had moved the
    /// positions, and dropped, restored or copied a version nobody chose.
    ///
    /// So a list of versions goes to the window with this number, the window
    /// sends it back with every position it acts on, and [`Session::at`] and
    /// [`Session::at_mut`] refuse a number that is not this one, under the same
    /// lock the action then runs in.
    ///
    /// Counted for every change the vault made rather than for every history
    /// that moved. Which histories a change moved is the question this is here
    /// to avoid answering, and a change that moved none costs the window one
    /// more reading of a list. A change the vault refused, or one that found
    /// nothing to do, is not one: the window does not read the list again after
    /// those, and its next press on a version would be refused for nothing.
    /// Nothing in it comes from what the vault holds, and typing held as a
    /// draft is not a change to the vault.
    ///
    /// Read through [`Held::revision`], which catches it up with
    /// [`Vault::edits`] first.
    revision: u64,
    /// The vault's [`Vault::edits`] when `revision` last caught up with it.
    edits: u64,
    /// How the vault's file stood when the window was last told, while the
    /// chosen file is a copy a lock left. "Make this my vault" is held to it
    /// (see [`Vault::promote`]): what the banner said is what the reader
    /// decided on. Forgotten when the session is pointed somewhere else.
    shown: Option<Seen>,
    /// Whether the open copy is being left for the vault it was taken from.
    /// The lock that closes it decides where the session points afterwards:
    /// see [`Session::back_to_vault`].
    returning: bool,
}

impl Held {
    /// The open vault, to be changed. The revision moves only if the vault
    /// says it did: see [`Held::revision`].
    fn changing(&mut self) -> Result<&mut Open, Failure> {
        self.open.get_mut().ok_or_else(Failure::no_vault)
    }

    /// The revision of the vault as it is now. Every change the vault made
    /// since this was last asked moves it once.
    fn revision(&mut self) -> u64 {
        if let Some(open) = self.open.get() {
            let edits = open.vault.edits();
            if edits != self.edits {
                self.edits = edits;
                self.revision += 1;
            }
        }
        self.revision
    }

    /// The open vault, when nothing has changed it since `revision`.
    fn at(&mut self, revision: u64) -> Result<&mut Open, Failure> {
        if self.open.get().is_none() {
            return Err(Failure::no_vault());
        }
        if revision != self.revision() {
            return Err(Failure::versions_changed());
        }
        self.changing()
    }
}

/// An open vault, the file the reader chose for one of its entries while they
/// are asked about a name that entry already gives another, and what they are
/// typing into its entries and have not finished.
///
/// One value rather than three fields, so that nothing can take the vault away
/// and leave the file or the typing behind. A lock, another database chosen, an
/// unlock over the top: every way the vault goes, the rest goes with it, wiped,
/// and no path through here has to remember to say so. The file goes the same
/// way with its entry (see [`Waiting::settle`]).
struct Open {
    vault: Vault,
    offered: Waiting,
    drafts: Drafts,
}

impl Open {
    fn of(vault: Vault) -> Open {
        Open {
            vault,
            offered: Waiting::default(),
            drafts: Drafts::default(),
        }
    }

    /// Writes what the reader was typing into the vault, the way leaving each
    /// field would have, and says where it went. A new value typed in a Change
    /// field is kept beside the value it was for, never over it (see
    /// [`Vault::set_typed`]), and one kept so is the answer whatever else went
    /// where it was typed: it is the one the reader has to be told about.
    ///
    /// A draft the vault will not take - its entry gone, its field removed, a
    /// vault Coffer does not write back - is let go, and the rest are still
    /// written: nothing here may stop a lock.
    fn finish_typing(&mut self) -> Written {
        let mut written = Written::Nothing;
        for (entry, field, value, typing) in self.drafts.take() {
            match self.vault.set_typed(entry, &field, value, typing) {
                Ok(Written::Beside) => written = Written::Beside,
                Ok(Written::Into) if written == Written::Nothing => written = Written::Into,
                _ => {}
            }
        }
        written
    }
}

mod slot {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::Open;

    /// Where the open vault is held, and whether there is one, said where
    /// [`Session::is_unlocked`](super::Session::is_unlocked) reads it without
    /// waiting for the lock the vault is behind.
    ///
    /// A module of its own so that its fields are private to it: the vault
    /// goes in and comes out through [`Slot::put`] and nowhere else, which
    /// keeps the two in step whichever way a later change takes the vault -
    /// `take`, `replace`, a plain assignment - because no other way compiles.
    pub(super) struct Slot {
        open: Option<Open>,
        unlocked: Arc<AtomicBool>,
    }

    impl Slot {
        /// An empty slot, saying so in `unlocked`.
        pub(super) fn new(unlocked: Arc<AtomicBool>) -> Slot {
            unlocked.store(false, Ordering::Release);
            Slot {
                open: None,
                unlocked,
            }
        }

        /// Puts a vault in, or takes the one there out - which wipes it.
        pub(super) fn put(&mut self, open: Option<Open>) {
            self.unlocked.store(open.is_some(), Ordering::Release);
            self.open = open;
        }

        pub(super) fn get(&self) -> Option<&Open> {
            self.open.as_ref()
        }

        pub(super) fn get_mut(&mut self) -> Option<&mut Open> {
            self.open.as_mut()
        }
    }
}

impl Session {
    /// A session pointed at `database`, writing down in `remembering` every
    /// vault that opens.
    pub fn new(database: Option<PathBuf>, remembering: Option<PathBuf>) -> Session {
        let unlocked = Arc::new(AtomicBool::new(false));
        Session {
            held: Mutex::new(Held {
                database,
                locked_by: None,
                lost: false,
                typed: Written::Nothing,
                key_file: None,
                making: None,
                found: None,
                measured: None,
                open: Slot::new(Arc::clone(&unlocked)),
                generation: 0,
                revision: 0,
                edits: 0,
                shown: None,
                returning: false,
            }),
            unlocked,
            remembering,
        }
    }

    /// A poisoned lock means a panic happened while the state was borrowed.
    /// Nothing here leaves the state half-written, so the contents are still
    /// worth having, and the alternative is a window that can never be used
    /// again.
    fn held(&self) -> MutexGuard<'_, Held> {
        self.held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The database Coffer will open next, whether or not it is open now.
    pub fn database(&self) -> Option<PathBuf> {
        self.held().database.clone()
    }

    /// Points the session at another database. Whatever was open is locked
    /// first: two databases at once is not something Coffer does, and the
    /// lock file beside the old one has to go before the new one is opened.
    ///
    /// The path is resolved here, so that everything downstream - the snapshots
    /// beside it, the name in the window, what gets remembered - is about the
    /// file rather than about a link to it. A path that is not there yet is
    /// kept as it was given; opening it is what will say so.
    pub fn choose(&self, database: PathBuf) {
        let database = database.canonicalize().unwrap_or(database);
        let mut held = self.held();
        held.open.put(None);
        held.database = Some(database);
        // A key file belongs to the vault it opens. Carrying one over to a
        // different file would turn a right password into a wrong one, with
        // nothing on the screen to explain it.
        held.key_file = None;
        // A flag about the vault that was open says nothing about the one being
        // chosen, for the same reason the key file does not carry over.
        held.lost = false;
        held.typed = Written::Nothing;
        held.shown = None;
        held.returning = false;
        held.found = None;
        held.generation += 1;
    }

    pub fn is_unlocked(&self) -> bool {
        self.unlocked.load(Ordering::Acquire)
    }

    /// Whether something holds the session now - a save, usually - asked
    /// without waiting for it to let go. A session left poisoned by a panic is
    /// not held by anybody.
    pub fn busy(&self) -> bool {
        matches!(self.held.try_lock(), Err(TryLockError::WouldBlock))
    }

    /// The key file the next unlock will use, if one has been chosen.
    pub fn key_file(&self) -> Option<PathBuf> {
        self.held().key_file.clone()
    }

    /// Chooses a key file for the next unlock, or takes the choice back.
    pub fn use_key_file(&self, path: Option<PathBuf>) {
        self.held().key_file = path;
    }

    /// Points the session at one of this database's own files: a snapshot, or
    /// the unsaved copy a lock left beside it.
    ///
    /// [`Session::choose`] with the key file put back. Both are copies of the
    /// same vault and open with the same credentials, so forgetting the key
    /// file - which is right for any other file - would leave one taken from a
    /// database that wants one impossible to open.
    pub fn choose_sibling(&self, sibling: PathBuf) {
        let key_file = self.key_file();
        self.choose(sibling);
        self.use_key_file(key_file);
    }

    /// Points the session back at the vault the chosen copy was taken from.
    /// The path is read off the copy's name, so nothing the window sends names
    /// it.
    ///
    /// A copy that is open stays open, and stays chosen: the lock that has to
    /// follow is what closes it, and it writes out whatever the copy holds the
    /// way every lock does, into the copy. Choosing another file would drop it
    /// unwritten. Only once that lock has kept nothing beside the copy - it had
    /// nothing to write, or the copy took it - does the session point at the
    /// vault. A lock that had to put the copy's work in a copy of its own, or
    /// could put it nowhere, leaves the copy chosen, so that the screen that
    /// comes back is the copy's and offers that copy or says what was lost. The
    /// vault's screen would find neither: they are about the copy.
    pub fn back_to_vault(&self) -> Result<(), Failure> {
        let mut held = self.held();
        let chosen = held.database.clone().ok_or_else(Failure::no_vault)?;
        let vault = unsaved::taken_from(&chosen).ok_or(VaultError::NotACopy)?;

        if held.open.get().is_some() {
            held.returning = true;
        } else {
            drop(held);
            self.choose_sibling(vault);
        }
        Ok(())
    }

    /// The vault the chosen copy was taken from, and how its file stands, as
    /// the window is about to be told. What it is told is what "Make this my
    /// vault" is held to.
    pub fn telling(&self) -> Option<(PathBuf, OnDisk)> {
        let mut held = self.held();
        let vault = held.database.as_deref().and_then(unsaved::taken_from)?;
        let seen = Seen::of(&vault);
        held.shown = Some(seen);
        Some((vault, seen.on_disk))
    }

    /// Opens the chosen database.
    ///
    /// The password is consumed: it goes into the vault, which holds it for as
    /// long as it is open because KeePass derives a fresh key on every save,
    /// and wipes it on the way out.
    ///
    /// `policy` is how a lock file beside the database is answered. Only a
    /// reader who has been shown who holds it, and said to open anyway, gets
    /// [`LockPolicy::TakeOver`]; nothing decides that on their behalf.
    pub fn unlock(&self, password: Zeroizing<Vec<u8>>, policy: LockPolicy) -> Result<(), Failure> {
        let (database, key_file, generation) = {
            let mut held = self.held();
            // Anything already open is dropped before the new one is opened, so
            // that reopening the same file does not find Coffer's own lock
            // beside it and refuse.
            held.open.put(None);
            (
                held.database.clone().ok_or_else(Failure::no_vault)?,
                held.key_file.clone(),
                held.generation,
            )
        };

        let mut key = MasterKey::from_password(password);
        if let Some(path) = &key_file {
            // Named rather than passed through: a key file that has been moved
            // or unplugged since it was chosen comes back from the filesystem as
            // a bare errno, which on this screen reads as the vault being gone.
            key = key
                .with_key_file(path)
                .map_err(|_| Failure::refused("the key file could not be read"))?;
        }

        // Key derivation is a second of work, and it happens with nothing held.
        // A window that asks the session anything meanwhile is answered rather
        // than left waiting on a lock this thread is holding.
        let vault = Vault::open(&database, key, policy)?;

        self.land(vault, generation, key_file)
    }

    /// Where both ways in end: the vault that has just opened becomes the one
    /// this session holds, and the file it came from is written down for the
    /// next launch.
    ///
    /// One door, because a vault used to be written down where it was picked
    /// in a panel, and a vault made here is never picked: the next launch
    /// greeted its owner as somebody who had nothing. Written down only here,
    /// because a file that was picked and never opened is not the reader's
    /// vault either.
    ///
    /// `key_file` is the one the vault opened with, which is the one the next
    /// unlock of it needs.
    fn land(
        &self,
        vault: Vault,
        generation: u64,
        key_file: Option<PathBuf>,
    ) -> Result<(), Failure> {
        let opened = {
            let mut held = self.held();
            if held.generation != generation {
                // Somebody chose another database, or locked, while this one
                // was opening. Dropping the vault here takes the lock file off
                // a database nobody is asking about any more.
                return Err(Failure::stale());
            }

            // The database Coffer opened is the one it followed the links to.
            let opened = vault.path().to_path_buf();
            held.database = Some(opened.clone());
            held.edits = vault.edits();
            held.open.put(Some(Open::of(vault)));
            // Every position the window was sent was about the vault that was
            // open before, if any was.
            held.revision += 1;
            held.key_file = key_file;
            held.making = None;
            held.locked_by = None;
            held.lost = false;
            held.typed = Written::Nothing;
            held.returning = false;
            opened
        };

        // Outside the lock: nothing about it needs the vault, and a write that
        // flushes the disk has no business holding up whatever else the window
        // is asking.
        self.write_down(&opened);
        Ok(())
    }

    /// Writes a vault down for the next launch. A vault Coffer fails to write
    /// down is one the reader picks again next launch, which is not a reason
    /// to refuse what has just happened to it.
    fn write_down(&self, vault: &Path) {
        if let Some(directory) = &self.remembering {
            let _ = recent::remember(directory, vault);
        }
    }

    /// Makes the open copy a lock left the vault it was taken from, and leaves
    /// the session open on that vault: see [`Vault::promote`].
    ///
    /// The vault opened, as far as the next launch is concerned, so it is
    /// written down the way a vault that opened is. The key file stays: the
    /// copy was written under the vault's credentials, and they are the same
    /// ones now.
    ///
    /// The vault's file is held to how the window was last told it stood.
    pub fn promote(&self) -> Result<PathBuf, Failure> {
        let vault = {
            let mut held = self.held();
            let shown = held.shown;
            let open = held.changing()?;
            open.vault.promote(shown)?;
            let vault = open.vault.path().to_path_buf();
            held.database = Some(vault.clone());
            held.shown = None;
            vault
        };

        self.write_down(&vault);
        Ok(vault)
    }

    /// Wipes the decrypted database out of memory, and says whether there was
    /// one to wipe.
    ///
    /// The answer is what stops two triggers arriving together tearing down two
    /// windows. Tauri goes on handing out the window between a destroy being
    /// queued and the event that says it happened, so a second caller that
    /// checked whether a vault was open and then destroyed would be destroying
    /// the window the first caller's rebuild had just made. Exactly one caller
    /// is told `true`.
    pub fn lock(&self, reason: Reason) -> bool {
        let mut held = self.held();
        // Written out before it is taken. Taking it is what destroys the only
        // copy of anything that never reached the file, and a vault is dirty
        // exactly when saving is the thing that failed - so a wipe on its own
        // is a session's work ended by a timer nobody was watching. What the
        // reader was typing goes in first, so that it is written out with the
        // rest, or kept beside the vault with the rest when the file will not
        // take it.
        let ended = held.open.get_mut().map(|open| {
            let typed = open.finish_typing();
            (typed, open.vault.rescue())
        });
        held.open.put(None);
        held.generation += 1;
        let returning = std::mem::take(&mut held.returning);

        let Some((typed, rescue)) = ended else {
            return false;
        };
        held.locked_by = Some(reason);
        held.lost = rescue == Rescue::Lost;
        held.typed = if rescue == Rescue::Saved {
            typed
        } else {
            Written::Nothing
        };

        // Leaving a copy for its vault, and the copy kept everything. Both
        // flags were about the copy, and the vault's screen must not say its
        // own file took what the copy took.
        let kept_all = matches!(rescue, Rescue::Nothing | Rescue::Saved);
        if let Some(vault) = held
            .database
            .as_deref()
            .and_then(unsaved::taken_from)
            .filter(|_| returning && kept_all)
        {
            held.database = Some(vault);
            held.typed = Written::Nothing;
            held.shown = None;
        }
        true
    }

    /// Whether the last lock in this run had to give something up.
    pub fn lost(&self) -> bool {
        self.held().lost
    }

    /// Whether the last lock in this run wrote what the reader was typing into
    /// the vault, and saved it there.
    pub fn typed(&self) -> bool {
        self.held().typed != Written::Nothing
    }

    /// Whether any of what the last lock wrote and saved was a new value kept
    /// beside the value it was typed for, which is still the field's.
    pub fn typed_beside(&self) -> bool {
        self.held().typed == Written::Beside
    }

    /// Why the window is asking for a password again, when there is something
    /// worth saying. A lock the reader asked for has nothing to explain.
    pub fn locked_by(&self) -> Option<Reason> {
        self.held().locked_by
    }

    /// Where a new vault will go.
    ///
    /// Settled before the password is asked for, because the password arrives
    /// as the whole body of its message and carries no named arguments beside
    /// it. Choosing somewhere else replaces this, and so does measuring the
    /// work: the two are halves of one answer.
    pub fn making(&self, target: PathBuf) {
        self.held().making = Some(target);
    }

    /// Where a new vault would go, once somewhere has been settled.
    pub fn target(&self) -> Option<PathBuf> {
        self.held().making.clone()
    }

    /// Keeps the vault in Coffer's own folder that the first-run screen is
    /// about to be told about, or that there is none.
    pub fn finding(&self, found: Option<PathBuf>) {
        self.held().found = found;
    }

    /// The vault in Coffer's own folder the screen was last told about.
    pub fn found(&self) -> Option<PathBuf> {
        self.held().found.clone()
    }

    /// What a one-second unlock costs on this machine, remembered for the
    /// creation that is about to happen.
    pub fn measured(&self, work: Work) {
        self.held().measured = Some(work);
    }

    /// Makes the chosen vault and opens it.
    ///
    /// On the same terms as an unlock: nothing is held while the key is
    /// derived, and a creation that finishes after the session has been pointed
    /// somewhere else does not land.
    pub fn create(&self, password: Zeroizing<Vec<u8>>) -> Result<(), Failure> {
        let (target, work, generation) = {
            let mut held = self.held();
            held.open.put(None);
            let target = held.making.clone().ok_or_else(Failure::nowhere_chosen)?;
            let work = held.measured.ok_or_else(|| {
                Failure::refused("the vault's key derivation has not been measured yet")
            })?;
            (target, work, held.generation)
        };

        let name = target
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();

        // `Vault::create` refuses a parent that is not there rather than guessing
        // at one, and the place the creation screen offers by default is a folder
        // that does not exist until the first vault goes into it.
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(Failure::io)?;
        }

        let vault = Vault::create(
            &target,
            MasterKey::from_password(password),
            &Recipe { name: &name, work },
        )?;

        // Coffer never makes a vault that wants a key file, so one chosen for
        // whatever was on the unlock screen before must not follow the new vault
        // into the next unlock.
        self.land(vault, generation, None)
    }

    pub fn tree(&self) -> Result<Project, Failure> {
        self.with(|vault| vault.tree())
    }

    pub fn entry(&self, id: EntryId) -> Result<Entry, Failure> {
        self.with(|vault| vault.entry(id))?
            .ok_or_else(Failure::no_such_entry)
    }

    /// Hands out one field's value. Every call is a place a secret can escape,
    /// so there are two callers: revealing and copying.
    ///
    /// A field the entry does not carry and an entry the database does not have
    /// are different answers, and the screen is told which one it got.
    pub fn reveal(&self, id: EntryId, field: &str) -> Result<SecretValue, Failure> {
        match self.with(|vault| vault.reveal(id, field))? {
            Some(secret) => Ok(secret),
            None => {
                self.entry(id)?;
                Err(Failure::refused("that entry has no such field"))
            }
        }
    }

    /// Hands out one field's value from a previous version, on the same terms
    /// as [`Session::reveal`] and to the same two callers, and only while the
    /// vault is at the revision the version's position was read at.
    pub fn reveal_version(
        &self,
        id: EntryId,
        index: usize,
        revision: u64,
        field: &str,
    ) -> Result<SecretValue, Failure> {
        self.at(revision, |vault| vault.reveal_version(id, index, field))?
            .ok_or_else(|| Failure::refused("that version has no such field"))
    }

    /// Borrows the open vault. It never leaves the lock, so nothing can hold a
    /// vault past a lock that was supposed to wipe it.
    pub fn with<T>(&self, read: impl FnOnce(&Vault) -> T) -> Result<T, Failure> {
        let held = self.held();
        let open = held.open.get().ok_or_else(Failure::no_vault)?;
        Ok(read(&open.vault))
    }

    /// Borrows the open vault for an answer that names versions by position,
    /// and says which revision of the vault the answer is about: see the
    /// `revision` the session holds.
    pub fn listing<T>(&self, read: impl FnOnce(&Vault) -> T) -> Result<(u64, T), Failure> {
        let mut held = self.held();
        let revision = held.revision();
        let open = held.open.get().ok_or_else(Failure::no_vault)?;
        Ok((revision, read(&open.vault)))
    }

    /// Borrows the open vault to read a version by its position, when nothing
    /// has changed the vault since `revision`, the one the position was read at.
    pub fn at<T>(&self, revision: u64, read: impl FnOnce(&Vault) -> T) -> Result<T, Failure> {
        let mut held = self.held();
        Ok(read(&held.at(revision)?.vault))
    }

    /// Borrows the open vault to act on a version by its position, on the same
    /// terms as [`Session::at`]. The check and the change happen under one
    /// lock, so nothing can land between them and move the position.
    pub fn at_mut<T>(
        &self,
        revision: u64,
        change: impl FnOnce(&mut Vault) -> T,
    ) -> Result<T, Failure> {
        let mut held = self.held();
        Ok(change(&mut held.at(revision)?.vault))
    }

    /// Borrows the open vault to change it.
    ///
    /// The lock is held for the whole change, and a save holds it for the
    /// second the key derivation takes. Nothing else may touch the vault
    /// meanwhile, which is the point: a window drawing a tree from a database
    /// that is half way through a change would be drawing something that was
    /// never true.
    ///
    /// A file waiting on an entry the change takes away goes with it.
    pub fn with_mut<T>(&self, change: impl FnOnce(&mut Vault) -> T) -> Result<T, Failure> {
        let mut held = self.held();
        let open = held.changing()?;
        let changed = change(&mut open.vault);
        open.offered.settle(&open.vault);
        Ok(changed)
    }

    /// Borrows the open vault to change it, the way [`Session::with_mut`]
    /// does, and lets go of the typing the change leaves nothing more to say
    /// about - under the same lock, so that no lock can land between the two
    /// and write that typing over the change.
    ///
    /// `sequence` is the window's number for the change. Typing it said before
    /// is let go whether or not the change goes through: a value refused is put
    /// back in the window, and an entry that could not be deleted or a file
    /// that could not be read again is still something the reader chose to
    /// leave behind.
    ///
    /// A file waiting on an entry goes on the same terms as in
    /// [`Session::with_mut`], and every file waiting goes with a reload.
    pub fn overtaking<T>(
        &self,
        over: Over<'_>,
        sequence: u64,
        change: impl FnOnce(&mut Vault) -> T,
    ) -> Result<T, Failure> {
        let mut held = self.held();
        let open = held.changing()?;
        // The file read again keeps an entry's id and may not keep the file
        // the reader was asked about, so whatever was waiting goes as well.
        if matches!(over, Over::Everything) {
            open.offered.clear();
        }
        open.drafts.over(over, sequence);
        let changed = change(&mut open.vault);
        open.offered.settle(&open.vault);
        Ok(changed)
    }

    /// Holds the window's latest word about a field the reader is typing into:
    /// what is in it, or nothing when that was taken back. The next lock writes
    /// it; see [`crate::drafts`].
    pub fn draft(
        &self,
        entry: EntryId,
        field: &str,
        typed: Option<Typed>,
        sequence: u64,
    ) -> Result<(), Failure> {
        let mut held = self.held();
        let open = held.open.get_mut().ok_or_else(Failure::no_vault)?;
        open.drafts.hear(entry, field, typed, sequence);
        Ok(())
    }

    /// Offers a file to an entry, and holds on to it when the entry already
    /// gives its name to another: see [`Waiting::offer`].
    pub fn offer(
        &self,
        entry: EntryId,
        name: String,
        data: Zeroizing<Vec<u8>>,
    ) -> Result<Attached, Failure> {
        let mut held = self.held();
        let open = held.changing()?;
        Ok(open.offered.offer(&mut open.vault, entry, name, data)?)
    }

    /// Puts the file waiting on `entry` where the reader said: see
    /// [`Waiting::answer`].
    pub fn answer(
        &self,
        entry: EntryId,
        answer: impl FnOnce(&mut Vault, EntryId, &str, &[u8]) -> Result<(), VaultError>,
    ) -> Result<(), Failure> {
        let mut held = self.held();
        let open = held.changing()?;
        open.offered.answer(&mut open.vault, entry, answer)
    }

    /// Lets go of the file waiting on `entry`, when there is one: see
    /// [`Waiting::withdraw`].
    pub fn withdraw(&self, entry: EntryId) {
        if let Some(open) = self.held().open.get_mut() {
            open.offered.withdraw(entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use vault_core::kind::Kind;
    use vault_core::model::{Deletion, GroupId, Move, fields};

    use super::*;

    /// The suite opens the databases `vault-core` generates with
    /// `keepassxc-cli`, because the only interesting session is one holding a
    /// database somebody else wrote.
    const RICH: &str = "rich-kdbx41.kdbx";
    const SECRET: &[u8] = b"coffer-test";

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vault-core/tests/fixtures")
            .join(name)
    }

    /// A copy, because opening a database writes a lock file beside it.
    fn scratch(name: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let target = directory.path().join(name);
        std::fs::copy(fixture(name), &target).expect("the fixture copies");
        (directory, target)
    }

    fn password(text: &[u8]) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(text.to_vec())
    }

    fn unlocked(name: &str) -> (tempfile::TempDir, Session) {
        let (directory, database) = scratch(name);
        let session = Session::new(Some(database), None);
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the database opens");
        (directory, session)
    }

    fn entry_titled(session: &Session, title: &str) -> Entry {
        fn walk(group: &Project, into: &mut Vec<EntryId>, title: &str) {
            for entry in &group.entries {
                if entry.title.open() == Some(title) {
                    into.push(entry.id);
                }
            }
            for section in &group.sections {
                walk(section, into, title);
            }
        }

        let mut found = Vec::new();
        walk(
            &session.tree().expect("the tree comes back"),
            &mut found,
            title,
        );
        assert_eq!(found.len(), 1, "expected one entry titled {title:?}");
        session
            .entry(*found.first().expect("it is there"))
            .expect("the entry comes back")
    }

    #[test]
    fn a_locked_session_hands_out_nothing() {
        let (_scratch, database) = scratch(RICH);
        let session = Session::new(Some(database), None);

        assert!(!session.is_unlocked());
        assert!(session.tree().is_err());
        assert!(
            session
                .entry(EntryId::from_uuid(uuid::Uuid::nil()))
                .is_err()
        );
        assert!(
            session
                .reveal(EntryId::from_uuid(uuid::Uuid::nil()), fields::PASSWORD)
                .is_err()
        );
    }

    #[test]
    fn unlocking_gives_the_tree_and_locking_takes_it_away() {
        let (_scratch, session) = unlocked(RICH);
        assert!(session.is_unlocked());
        assert!(
            !session
                .tree()
                .expect("the tree comes back")
                .sections
                .is_empty()
        );

        session.lock(Reason::ByHand);

        assert!(!session.is_unlocked());
        assert!(session.tree().is_err());
    }

    /// The lock file beside the database goes when the vault does. A session
    /// that left it behind could never open the same database again.
    #[test]
    fn locking_releases_the_database_for_the_next_unlock() {
        let (_scratch, database) = scratch(RICH);
        let session = Session::new(Some(database.clone()), None);

        for _ in 0..3 {
            session
                .unlock(password(SECRET), LockPolicy::Respect)
                .expect("the database opens");
            session.lock(Reason::ByHand);
        }

        assert!(
            vault_core::storage::lock::inspect(&database)
                .expect("the lock file reads")
                .is_none()
        );
    }

    /// The defect the rescue exists for. A save the file refuses leaves the
    /// change in memory, and the tree is what locking wipes: without a write on
    /// the way out, a timer nobody watched is what ends a session's work.
    #[test]
    fn locking_a_vault_that_cannot_save_writes_it_beside_the_database_first() {
        let (_scratch, database) = scratch(RICH);
        let session = Session::new(Some(database.clone()), None);
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the database opens");

        let id = entry_titled(&session, "basic").id;
        session
            .with_mut(|vault| {
                vault.set_field(
                    id,
                    fields::NOTES,
                    vault_core::NewValue::Open("never saved".to_owned()),
                )
            })
            .expect("the session is open")
            .expect("the note is written");

        // Somebody else writes the file, so the save on the way out is refused
        // and the copy beside it is the only way the change survives.
        {
            let mut theirs = vault_core::Vault::open(
                &database,
                vault_core::MasterKey::from_password(password(SECRET)),
                LockPolicy::TakeOver,
            )
            .expect("the other client opens it");
            theirs
                .set_field(
                    id,
                    fields::URL,
                    vault_core::NewValue::Open("https://theirs.example".to_owned()),
                )
                .expect("their change is applied");
            theirs.save().expect("their save goes through");
        }

        assert!(session.lock(Reason::Idle));
        assert!(!session.lost(), "the work was written and nothing was lost");

        let kept = vault_core::storage::unsaved::beside(&database).expect("a sibling path");
        let rescued = vault_core::Vault::open(
            &kept,
            vault_core::MasterKey::from_password(password(SECRET)),
            LockPolicy::Respect,
        )
        .expect("the copy opens with the same password");
        assert_eq!(notes(&rescued, id).as_deref(), Some("never saved"));
    }

    /// Two triggers arriving together still write once, because the write
    /// happens under the same lock that takes the vault away.
    #[test]
    fn two_locks_arriving_together_write_the_vault_once() {
        let (_scratch, session) = unlocked(RICH);
        let id = entry_titled(&session, "basic").id;
        session
            .with_mut(|vault| {
                vault.set_field(
                    id,
                    fields::NOTES,
                    vault_core::NewValue::Open("once".to_owned()),
                )
            })
            .expect("the session is open")
            .expect("the note is written");

        assert!(session.lock(Reason::Idle));
        assert!(
            !session.lock(Reason::Sleeping),
            "a second lock found a vault"
        );
        assert!(!session.lost());
    }

    /// Whatever the last lock could not write says nothing about the next vault
    /// the reader opens, so it goes when one is opened or another is chosen.
    #[test]
    fn an_unlock_forgets_what_the_last_lock_could_not_write() {
        let (_scratch, database) = scratch(RICH);
        let session = Session::new(Some(database.clone()), None);
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the database opens");
        session.lock(Reason::Idle);
        assert!(!session.lost(), "an untouched vault lost something");

        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the database opens again");
        assert!(!session.lost());
    }

    /// Unlocking twice without locking in between must not trip over Coffer's
    /// own lock file.
    #[test]
    fn unlocking_an_open_database_again_replaces_it() {
        let (_scratch, session) = unlocked(RICH);
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("it opens again");
        assert!(session.is_unlocked());
    }

    #[test]
    fn a_wrong_password_leaves_nothing_open() {
        let (_scratch, database) = scratch(RICH);
        let session = Session::new(Some(database), None);

        for wrong in [b"".as_slice(), b"coffer-tes".as_slice(), &[0xff, 0xfe]] {
            assert!(
                session
                    .unlock(password(wrong), LockPolicy::Respect)
                    .is_err()
            );
            assert!(!session.is_unlocked());
        }

        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the right one opens");
    }

    #[test]
    fn choosing_another_database_locks_the_one_that_is_open() {
        let (_scratch, session) = unlocked(RICH);
        let (_other, database) = scratch(RICH);

        session.choose(database.clone());

        assert!(!session.is_unlocked());
        // The session keeps the file rather than the way in to it: on macOS a
        // scratch directory is reached through a link.
        assert_eq!(
            session.database(),
            Some(database.canonicalize().expect("the copy is there"))
        );
    }

    #[test]
    fn there_is_nothing_to_unlock_until_something_is_chosen() {
        let session = Session::new(None, None);
        assert!(
            session
                .unlock(password(SECRET), LockPolicy::Respect)
                .is_err()
        );
    }

    /// Key derivation happens with nothing held, so the session can move while
    /// it runs. Whatever the timing, the session must end up where it was last
    /// pointed rather than where the unlock started.
    #[test]
    fn an_unlock_that_finishes_late_does_not_win() {
        use std::sync::Arc;

        let (_first, one) = scratch(RICH);
        let (_second, two) = scratch(RICH);

        let session = Arc::new(Session::new(Some(one), None));
        let unlocking = {
            let session = Arc::clone(&session);
            std::thread::spawn(move || session.unlock(password(SECRET), LockPolicy::Respect))
        };

        // Long enough for the unlock to be inside key derivation, which takes
        // the better part of a second, and short enough that it is still there.
        std::thread::sleep(std::time::Duration::from_millis(50));
        session.choose(two.clone());
        let _ = unlocking.join();

        // Whichever of the two got there first, the session points at what it
        // was last told to point at. Without the check inside `unlock`, the
        // vault that finished opening writes the old path back over this one.
        assert_eq!(
            session.database(),
            Some(two.canonicalize().expect("the copy is there"))
        );
    }

    /// The unlock screen tells a database somebody else has open apart from a
    /// password that did not fit, and it does that on the code alone.
    #[test]
    fn a_database_another_process_holds_is_its_own_answer() {
        let (_scratch, database) = scratch(RICH);
        let holding = Session::new(Some(database.clone()), None);
        holding
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the first one opens");

        let second = Session::new(Some(database), None);
        let failure = second
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect_err("the second one is refused");

        let payload = serde_json::to_value(&failure).expect("a failure serialises");
        assert_eq!(payload["code"], "heldByAnother");
        assert!(!second.is_unlocked());
    }

    #[test]
    fn a_field_comes_out_one_at_a_time() {
        let (_scratch, session) = unlocked(RICH);
        let entry = entry_titled(&session, "basic");

        let secret = session
            .reveal(entry.id, fields::PASSWORD)
            .expect("the password comes back");
        assert_eq!(secret.expose_str(), Some("correct horse battery staple"));

        assert!(session.reveal(entry.id, "no such field").is_err());
        assert!(session.reveal(entry.id, "").is_err());
    }

    /// A previous version hands out its values the way the entry does: one at
    /// a time, by name, and the part a reader selected is cut from the
    /// version's value rather than from what the entry holds now.
    #[test]
    fn a_version_hands_out_its_own_value_and_the_part_of_it_asked_for() {
        let (_scratch, session) = unlocked(RICH);
        let entry = entry_titled(&session, "basic");
        session
            .with_mut(|vault| {
                vault.set_field(
                    entry.id,
                    fields::PASSWORD,
                    vault_core::NewValue::Protected(Zeroizing::new("a new one".to_owned())),
                )
            })
            .expect("the vault is open")
            .expect("the password is written");

        let (revision, versions) = session
            .listing(|vault| vault.versions(entry.id))
            .expect("the vault is open");
        let newest = versions.last().expect("the edit kept a version").index;

        let old = session
            .reveal_version(entry.id, newest, revision, fields::PASSWORD)
            .expect("the old password comes back");
        assert_eq!(old.expose_str(), Some("correct horse battery staple"));
        assert_eq!(
            old.part(8, 13).expect("a word of it").expose_str(),
            Some("horse")
        );
        assert!(old.part(8, 99).is_err());

        assert!(
            session
                .reveal_version(entry.id, newest, revision, "no such field")
                .is_err()
        );
        assert!(
            session
                .reveal_version(entry.id, versions.len() + 5, revision, fields::PASSWORD)
                .is_err()
        );
        assert!(
            session
                .reveal_version(
                    EntryId::from_uuid(uuid::Uuid::nil()),
                    0,
                    revision,
                    fields::PASSWORD
                )
                .is_err()
        );
    }

    /// What an answer about a version came to: the code it was refused with,
    /// or that it went through.
    fn refusal<T>(answer: Result<T, Failure>) -> String {
        match answer {
            Ok(_) => "went through".to_owned(),
            Err(failure) => code_of(&failure),
        }
    }

    fn password_of(session: &Session, id: EntryId) -> Option<String> {
        session
            .reveal(id, fields::PASSWORD)
            .ok()
            .and_then(|secret| secret.expose_str().map(str::to_owned))
    }

    /// Every way a version is acted on by its position - read, handed out for
    /// a reveal or a copy, restored, dropped - is refused once the vault has
    /// changed since the position was read, even by a change nowhere near the
    /// entry, and nothing is done. The refusal changes nothing either, so the
    /// list read again is the one to act from.
    #[test]
    fn a_version_is_acted_on_only_at_the_revision_its_position_was_read_at() {
        let (_scratch, session) = unlocked(RICH);
        let basic = entry_titled(&session, "basic").id;
        for password in ["first", "second"] {
            session
                .with_mut(|vault| {
                    vault.set_field(
                        basic,
                        fields::PASSWORD,
                        vault_core::NewValue::Protected(Zeroizing::new(password.to_owned())),
                    )
                })
                .expect("the vault is open")
                .expect("the password is written");
        }
        let (listed, before) = session
            .listing(|vault| vault.versions(basic))
            .expect("the vault is open");
        let index = before.first().expect("the entry has versions").index;

        let root = session.tree().expect("the tree comes back").id;
        session
            .with_mut(|vault| vault.create_group(root, "Elsewhere"))
            .expect("the vault is open")
            .expect("the folder is made");

        let moved = "versionsChanged";
        assert_eq!(
            refusal(session.at(listed, |vault| vault.version(basic, index))),
            moved,
            "a version was read at a position from before the change"
        );
        assert_eq!(
            refusal(session.reveal_version(basic, index, listed, fields::PASSWORD)),
            moved,
            "a version's value was handed out for a reveal or a copy"
        );
        assert_eq!(
            refusal(session.at_mut(listed, |vault| vault.restore_version(basic, index))),
            moved,
            "a version was restored"
        );
        assert_eq!(
            refusal(session.at_mut(listed, |vault| vault.delete_version(basic, index))),
            moved,
            "a version was dropped"
        );

        let (now, after) = session
            .listing(|vault| vault.versions(basic))
            .expect("the vault is open");
        assert_eq!(
            after.len(),
            before.len(),
            "a refused drop dropped a version"
        );
        assert_eq!(password_of(&session, basic).as_deref(), Some("second"));

        session
            .at(now, |vault| vault.version(basic, index))
            .expect("the list is current")
            .expect("the version is read");
        session
            .reveal_version(basic, index, now, fields::PASSWORD)
            .expect("the value is handed out");
        session
            .at_mut(now, |vault| vault.restore_version(basic, index))
            .expect("the list is current")
            .expect("the version is restored");
        // The restore is a change of its own, and the list it was pressed in
        // is now as old as the first one was.
        assert_eq!(
            refusal(session.at_mut(now, |vault| vault.delete_version(basic, index))),
            moved
        );
    }

    /// Looking changes nothing, so it leaves the revision alone, and a list
    /// read before any amount of looking can still be acted on. Typing held as
    /// a draft is not a change to the vault either: it is written by the lock.
    #[test]
    fn reading_and_typing_leave_the_revision_where_it_was() {
        let (_scratch, session) = unlocked(RICH);
        let basic = entry_titled(&session, "basic").id;
        session
            .with_mut(|vault| {
                vault.set_field(
                    basic,
                    fields::NOTES,
                    vault_core::NewValue::Open("edited".to_owned()),
                )
            })
            .expect("the vault is open")
            .expect("the notes are written");
        let (listed, versions) = session
            .listing(|vault| vault.versions(basic))
            .expect("the vault is open");
        let newest = versions.last().expect("the edit kept a version").index;

        session.tree().expect("the tree comes back");
        session.entry(basic).expect("the entry comes back");
        session
            .reveal(basic, fields::PASSWORD)
            .expect("the password comes back");
        session
            .at(listed, |vault| vault.version(basic, newest))
            .expect("the list is current")
            .expect("the version is read");
        session
            .reveal_version(basic, newest, listed, fields::PASSWORD)
            .expect("the value is handed out");
        session
            .draft(basic, fields::NOTES, words("half typed", false), 1)
            .expect("the draft is held");
        session.withdraw(basic);

        let (again, _) = session
            .listing(|vault| vault.versions(basic))
            .expect("the vault is open");
        assert_eq!(again, listed);
        session
            .at_mut(listed, |vault| vault.delete_version(basic, newest))
            .expect("the list is current")
            .expect("the version is dropped");
    }

    /// A change the vault refused did not happen, and neither did one that
    /// found nothing to do. The window reads no list again after either - it
    /// says why and goes on - so a revision moved by them left every press on
    /// a version refused with a sentence about versions changing when none had.
    #[test]
    fn a_refused_change_leaves_the_revision_where_it_was() {
        let (_scratch, session) = unlocked(RICH);
        let basic = entry_titled(&session, "basic").id;
        session
            .with_mut(|vault| {
                vault.set_field(
                    basic,
                    fields::NOTES,
                    vault_core::NewValue::Open("edited".to_owned()),
                )
            })
            .expect("the vault is open")
            .expect("the notes are written");
        let (listed, versions) = session
            .listing(|vault| vault.versions(basic))
            .expect("the vault is open");
        let newest = versions.last().expect("the edit kept a version").index;
        let root = session.tree().expect("the tree comes back");

        type Change = fn(&mut Vault, EntryId) -> Result<(), VaultError>;
        let refused: [(&str, Change); 8] = [
            ("a field that is not there", |vault, id| {
                vault.remove_field(id, "no such field", false)
            }),
            ("an undo of a removal that never happened", |vault, id| {
                vault.undo_removal(id, fields::NOTES)
            }),
            ("an erasure of an entry outside the bin", |vault, id| {
                vault.delete_entries(&[(id, Deletion::Forever)])
            }),
            ("tags that cannot be written", |vault, id| {
                vault.set_tags(id, vec![" padded ".to_owned()])
            }),
            ("a move into a folder that is not there", |vault, id| {
                let nowhere = GroupId::from_uuid(uuid::Uuid::nil());
                vault.move_entries(&[id], nowhere).map(drop)
            }),
            ("a folder moved into itself", |vault, _| {
                let work = folder_named(vault, "Work");
                vault.move_group(work, work)
            }),
            ("a move taken back that never happened", |vault, id| {
                let top = vault.tree().id;
                let into = vault.entry(id).map_or(top, |entry| entry.group);
                vault.move_entries_back(
                    &[Move {
                        entry: id,
                        from: top,
                    }],
                    into,
                )
            }),
            (
                "a folder's move taken back that never happened",
                |vault, _| {
                    let (top, work) = (vault.tree().id, folder_named(vault, "Work"));
                    vault.move_group_back(work, folder_named(vault, "Personal"), top)
                },
            ),
        ];
        for (what, change) in refused {
            let done = session
                .with_mut(|vault| change(vault, basic))
                .expect("the vault is open");
            assert!(done.is_err(), "{what} was not refused");
        }
        session
            .with_mut(|vault| vault.rename_group(root.id, &root.name))
            .expect("the vault is open")
            .expect("a folder keeps the name it has");
        session
            .with_mut(|vault| {
                vault.set_typed(
                    basic,
                    fields::NOTES,
                    vault_core::NewValue::Open("edited".to_owned()),
                    vault_core::Typing::InPlace,
                )
            })
            .expect("the vault is open")
            .expect("typing what is there already writes nothing");
        let home = session.entry(basic).expect("the entry comes back").group;
        let moved = session
            .with_mut(|vault| vault.move_entries(&[basic], home))
            .expect("the vault is open")
            .expect("a move to where it is already moves nothing");
        assert!(moved.is_empty());

        let (again, _) = session
            .listing(|vault| vault.versions(basic))
            .expect("the vault is open");
        assert_eq!(
            again, listed,
            "a change that did not happen moved the revision"
        );
        session
            .at_mut(listed, |vault| vault.delete_version(basic, newest))
            .expect("the list is current")
            .expect("the version is dropped");
    }

    /// The folder with this name among the vault's own.
    fn folder_named(vault: &Vault, name: &str) -> GroupId {
        vault
            .tree()
            .sections
            .iter()
            .find(|section| section.name == name)
            .map(|section| section.id)
            .expect("the fixture has the folder")
    }

    /// A move is a change like any other to a position read before it: the
    /// revision moves, and a version named from a list read before the move is
    /// refused rather than acted on. Taking the move back moves it again.
    #[test]
    fn a_move_moves_the_revision() {
        let (_scratch, session) = unlocked(RICH);
        let versioned = entry_titled(&session, "versioned").id;
        let work = session
            .with(|vault| folder_named(vault, "Work"))
            .expect("the vault is open");
        let (listed, versions) = session
            .listing(|vault| vault.versions(versioned))
            .expect("the vault is open");
        let oldest = versions.first().expect("the entry has history").index;

        let moved = session
            .with_mut(|vault| vault.move_entries(&[versioned], work))
            .expect("the vault is open")
            .expect("it moves");
        let (now, _) = session
            .listing(|vault| vault.versions(versioned))
            .expect("the vault is open");
        assert_eq!(now, listed + 1, "a move is one change");
        assert_eq!(
            refusal(session.at_mut(listed, |vault| vault.restore_version(versioned, oldest))),
            "versionsChanged"
        );

        session
            .with_mut(|vault| vault.move_entries_back(&moved, work))
            .expect("the vault is open")
            .expect("the move is taken back");
        let (back, _) = session
            .listing(|vault| vault.versions(versioned))
            .expect("the vault is open");
        assert_eq!(back, now + 1);

        let (top, personal) = session
            .with(|vault| (vault.tree().id, folder_named(vault, "Personal")))
            .expect("the vault is open");
        type Folder = fn(&mut Vault, GroupId, GroupId, GroupId) -> Result<(), VaultError>;
        let there_and_back: [Folder; 2] = [
            |vault, folder, into, _| vault.move_group(folder, into),
            |vault, folder, into, from| vault.move_group_back(folder, from, into),
        ];
        for change in there_and_back {
            let (before, _) = session
                .listing(|vault| vault.versions(versioned))
                .expect("the vault is open");
            session
                .with_mut(|vault| change(vault, work, personal, top))
                .expect("the vault is open")
                .expect("the folder moves");
            let (after, _) = session
                .listing(|vault| vault.versions(versioned))
                .expect("the vault is open");
            assert_eq!(after, before + 1, "a folder's move is one change");
        }
    }

    /// A move takes nothing the reader was typing into an entry, and no file
    /// waiting on one: an entry keeps its id wherever it goes. What was being
    /// typed is written by the lock into the entry where it now stands, and a
    /// file asked about before the move goes on when the question is answered
    /// after it.
    #[test]
    fn a_move_keeps_what_is_being_typed_and_the_file_waiting() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic").id;
        let key = entry_titled(&session, "ssh key").id;
        let (personal, work) = session
            .with(|vault| (folder_named(vault, "Personal"), folder_named(vault, "Work")))
            .expect("the vault is open");

        session
            .draft(
                basic,
                fields::URL,
                words("https://half.example/pa", false),
                1,
            )
            .expect("the draft is heard");
        let asked = session
            .offer(key, KEY.to_owned(), Zeroizing::new(b"a new key".to_vec()))
            .expect("the file is offered");
        assert!(matches!(asked, Attached::Taken(_)));

        for (id, into) in [(basic, work), (key, personal)] {
            session
                .with_mut(|vault| vault.move_entries(&[id], into))
                .expect("the vault is open")
                .expect("it moves");
        }
        session
            .answer(key, Vault::keep_both)
            .expect("the file is still waiting on the entry");
        assert!(
            files_of(&session, key).contains(&(format!("{KEY} 2"), b"a new key".to_vec())),
            "the file waiting went with the move"
        );

        assert!(session.lock(Reason::Sleeping));
        let file = reopened(&database);
        assert_eq!(
            value_of(&file, basic, fields::URL).as_deref(),
            Some("https://half.example/pa"),
            "what was typed went with the move"
        );
        assert_eq!(file.entry(basic).map(|entry| entry.group), Some(work));
        assert_eq!(file.entry(key).map(|entry| entry.group), Some(personal));
    }

    /// A copy is a new entry, and what the reader was typing into the one it
    /// was copied from stays with that one: the lock writes it there and not
    /// into the copy, which keeps what the entry held when it was copied.
    #[test]
    fn typing_in_an_entry_that_was_copied_stays_with_that_entry() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic").id;

        session
            .draft(
                basic,
                fields::URL,
                words("https://half.example/pa", false),
                1,
            )
            .expect("the draft is heard");
        let copy = session
            .with_mut(|vault| vault.duplicate_entry(basic))
            .expect("the vault is open")
            .expect("the copy is made");

        assert!(session.lock(Reason::Sleeping));
        let file = reopened(&database);
        assert_eq!(
            value_of(&file, basic, fields::URL).as_deref(),
            Some("https://half.example/pa"),
            "what was typed did not reach the entry it was typed into"
        );
        assert_eq!(
            value_of(&file, copy, fields::URL).as_deref(),
            Some("https://example.com/login?a=1&b=2"),
            "the copy took what was typed into the original"
        );
    }

    /// A file waiting on the reader's answer about one entry is that entry's:
    /// its copy has none waiting, an answer naming the copy is refused, and the
    /// question about the original can still be answered.
    #[test]
    fn a_file_waiting_on_an_entry_is_not_carried_to_its_copy() {
        let (_scratch, session) = unlocked(RICH);
        let key = entry_titled(&session, "ssh key").id;

        let asked = session
            .offer(key, KEY.to_owned(), Zeroizing::new(b"a new key".to_vec()))
            .expect("the file is offered");
        assert!(matches!(asked, Attached::Taken(_)));
        let copy = session
            .with_mut(|vault| vault.duplicate_entry(key))
            .expect("the vault is open")
            .expect("the copy is made");
        let copied = files_of(&session, copy);
        assert_eq!(copied, files_of(&session, key));

        let refused = session
            .answer(copy, Vault::keep_both)
            .expect_err("the copy answered for the original's file");
        assert_eq!(code_of(&refused), "refused");
        assert_eq!(files_of(&session, copy), copied);

        session
            .answer(key, Vault::keep_both)
            .expect("the file is still waiting on the entry it was chosen for");
        assert!(
            files_of(&session, key).contains(&(format!("{KEY} 2"), b"a new key".to_vec())),
            "the answer did not go on"
        );
    }

    /// Making a copy is a change like any other to a position read before it,
    /// and a copy refused - out of the bin - is not.
    #[test]
    fn a_copy_moves_the_revision() {
        let (_scratch, session) = unlocked(RICH);
        let versioned = entry_titled(&session, "versioned").id;
        let deleted = entry_titled(&session, "deleted entry").id;
        let (listed, versions) = session
            .listing(|vault| vault.versions(versioned))
            .expect("the vault is open");
        let oldest = versions.first().expect("the entry has history").index;

        assert!(
            session
                .with_mut(|vault| vault.duplicate_entry(deleted))
                .expect("the vault is open")
                .is_err()
        );
        let (unmoved, _) = session
            .listing(|vault| vault.versions(versioned))
            .expect("the vault is open");
        assert_eq!(unmoved, listed, "a refused copy moved the revision");

        session
            .with_mut(|vault| vault.duplicate_entry(versioned))
            .expect("the vault is open")
            .expect("the copy is made");
        let (now, _) = session
            .listing(|vault| vault.versions(versioned))
            .expect("the vault is open");
        assert_eq!(now, listed + 1, "a copy is one change");
        assert_eq!(
            refusal(session.at_mut(listed, |vault| vault.restore_version(versioned, oldest))),
            "versionsChanged"
        );
    }

    /// The case the revision is for. A save brings every entry's history
    /// inside the database's limits, so a position read before it names a
    /// different version after it - and a press made during the save reaches
    /// the vault after it. The save moves the revision, and the press is
    /// refused rather than dropping the neighbour of the version it was about.
    #[test]
    fn a_save_that_prunes_moves_the_revision() {
        let (_scratch, session) = unlocked(RICH);
        let basic = entry_titled(&session, "basic").id;
        for round in 0..12 {
            session
                .with_mut(|vault| {
                    vault.set_field(
                        basic,
                        fields::NOTES,
                        vault_core::NewValue::Open(format!("round {round}")),
                    )
                })
                .expect("the vault is open")
                .expect("the notes are written");
        }
        let (listed, before) = session
            .listing(|vault| vault.versions(basic))
            .expect("the vault is open");
        let notes_at = |index: usize, revision: u64| {
            session
                .reveal_version(basic, index, revision, fields::NOTES)
                .ok()
                .and_then(|notes| notes.expose_str().map(str::to_owned))
        };
        let chosen = before
            .get(before.len() - 2)
            .expect("the entry has versions")
            .index;
        let held = notes_at(chosen, listed);

        session
            .with_mut(Vault::save)
            .expect("the vault is open")
            .expect("the vault saves");
        let (now, after) = session
            .listing(|vault| vault.versions(basic))
            .expect("the vault is open");
        assert!(
            after.len() < before.len(),
            "the premise is a save that prunes"
        );
        assert_ne!(now, listed);
        assert_ne!(
            notes_at(chosen, now),
            held,
            "the premise is a position that names another version after the save"
        );

        assert_eq!(
            refusal(session.at_mut(listed, |vault| vault.delete_version(basic, chosen))),
            "versionsChanged"
        );
        assert_eq!(
            session
                .with(|vault| vault.versions(basic).len())
                .expect("the vault is open"),
            after.len()
        );
    }

    /// A reload replaces every history, and so does another vault: a lock and
    /// an unlock, even of the same file, leave nothing a position from before
    /// them can name.
    #[test]
    fn a_reload_or_a_lock_moves_the_revision() {
        let (_directory, _database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic").id;
        let (first, _) = session
            .listing(|vault| vault.versions(basic))
            .expect("the vault is open");

        session
            .overtaking(Over::Everything, 1, Vault::reload)
            .expect("the vault is open")
            .expect("the file is read again");
        let (second, _) = session
            .listing(|vault| vault.versions(basic))
            .expect("the vault is open");
        assert_ne!(second, first);

        session.lock(Reason::ByHand);
        assert_eq!(
            refusal(session.at(second, |vault| vault.version(basic, 0))),
            "noVault"
        );
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the database opens again");
        assert_eq!(
            refusal(session.at(second, |vault| vault.version(basic, 0))),
            "versionsChanged"
        );
    }

    /// A change goes through the same lock the reads do, so a screen can never
    /// draw a database that is half way through one.
    #[test]
    fn a_change_needs_an_open_vault_and_shows_up_in_the_next_read() {
        let (_scratch, database) = scratch(RICH);
        let session = Session::new(Some(database), None);

        // Nothing is open, so nothing changes.
        assert!(
            session
                .with_mut(|vault| vault.create_group(vault.tree().id, "Later"))
                .is_err()
        );

        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the database opens");
        let root = session.tree().expect("the tree comes back").id;
        session
            .with_mut(|vault| vault.create_group(root, "Later"))
            .expect("the session is open")
            .expect("the folder is made");

        assert!(
            session
                .tree()
                .expect("the tree comes back")
                .sections
                .iter()
                .any(|section| section.name == "Later")
        );

        // Locking takes the change with the vault: nothing was written.
        session.lock(Reason::ByHand);
        assert!(session.tree().is_err());
    }

    /// The end of the boundary, from a real database to the bytes the webview
    /// would receive: the password of an entry the screen is drawing is not in
    /// them.
    #[test]
    fn nothing_the_screen_is_sent_carries_a_password() {
        let (_scratch, session) = unlocked(RICH);
        let entry = entry_titled(&session, "basic");

        let secret = session
            .reveal(entry.id, fields::PASSWORD)
            .expect("the password comes back");
        let secret = secret.expose_str().expect("it is text").to_owned();

        let drawn = serde_json::to_string(&crate::dto::Entry::of(&entry)).expect("it serialises");
        let listed = serde_json::to_string(&crate::dto::Group::of(
            &session.tree().expect("the tree comes back"),
        ))
        .expect("it serialises");

        assert!(!secret.is_empty());
        assert!(
            !drawn.contains(&secret),
            "the entry payload carries the password"
        );
        assert!(
            !listed.contains(&secret),
            "the tree payload carries the password"
        );
        assert!(
            session
                .reveal(EntryId::from_uuid(uuid::Uuid::nil()), fields::PASSWORD)
                .is_err()
        );
    }

    /// What a kind hides crosses as nothing from the moment the entry is made,
    /// the way any protected value does: a card's number, CVV and PIN typed
    /// into a new entry wait in Rust for a reveal. Its other fields cross as
    /// the empty text they hold, and `protected` says which is which.
    #[test]
    fn a_new_cards_secrets_cross_as_nothing_until_revealed() {
        let (_scratch, session) = unlocked(RICH);
        let card = session
            .with_mut(|vault| vault.create_entry(vault.tree().id, Kind::BankCard))
            .expect("the session is open")
            .expect("the card is made");
        let drawn = serde_json::to_value(crate::dto::Entry::of(
            &session.entry(card).expect("the entry comes back"),
        ))
        .expect("it serialises");
        let sent = |name: &str| {
            drawn["fields"]
                .as_array()
                .and_then(|all| all.iter().find(|field| field["name"] == name))
                .map(|field| (field["value"].clone(), field["protected"].clone()))
        };

        for hidden in ["Number", "CVV", "PIN"] {
            assert_eq!(
                sent(hidden),
                Some((serde_json::Value::Null, serde_json::json!(true))),
                "{hidden} crossed to the window"
            );
        }
        for open in ["Cardholder", "Expires", "Bank phone"] {
            assert_eq!(
                sent(open),
                Some((serde_json::json!(""), serde_json::json!(false))),
                "{open}"
            );
        }
    }

    /// The bin as the window is sent it, from a file KeePassXC wrote: the entry
    /// in it says which folder it goes back to by that folder's id, says that
    /// deleting it again is for good, and once put back crosses as an entry like
    /// any other. A second put back is a refusal rather than a second move.
    #[test]
    fn an_entry_in_the_bin_crosses_saying_where_it_goes_and_is_put_back_there() {
        let (_scratch, session) = unlocked(RICH);
        let deleted = entry_titled(&session, "deleted entry");
        let tree = serde_json::to_value(crate::dto::Group::of(
            &session.tree().expect("the tree comes back"),
        ))
        .expect("it serialises");

        let sections = tree["sections"].as_array().expect("the top has folders");
        let named = |name: &str| {
            sections
                .iter()
                .find(|section| section["name"] == name)
                .unwrap_or_else(|| panic!("the fixture has {name}"))
        };
        let work = named("Work")["id"].clone();
        let bin = named("Recycle Bin");
        assert_eq!(bin["isRecycleBin"], true);
        assert_eq!(bin["entries"][0]["binned"]["from"], work);
        assert_eq!(named("Work")["deletion"], "bin");

        let drawn = serde_json::to_value(crate::dto::Entry::of(&deleted)).expect("it serialises");
        assert_eq!(drawn["deletion"], "forever");
        assert_eq!(drawn["binned"]["from"], work);

        session
            .with_mut(|vault| vault.put_back_entries(&[deleted.id]))
            .expect("the session is open")
            .expect("it is put back");
        let back = serde_json::to_value(crate::dto::Entry::of(
            &session.entry(deleted.id).expect("the entry comes back"),
        ))
        .expect("it serialises");
        assert_eq!(back["group"], work);
        assert_eq!(back["binned"], serde_json::Value::Null);
        assert_eq!(back["deletion"], "bin");

        assert!(matches!(
            session.with_mut(|vault| vault.put_back_entries(&[deleted.id])),
            Ok(Err(VaultError::NotInRecycleBin))
        ));
    }

    /// The screen locks while a vault is opening.
    ///
    /// Opening is the moment a vault is most exposed: the password is in
    /// memory, nothing is holding the session, and the machine can say the
    /// reader has gone at any point in it. A vault that finished opening after
    /// that would be one sitting decrypted behind a locked screen, which is the
    /// one thing the machine's triggers exist to prevent.
    #[test]
    fn a_lock_during_key_derivation_leaves_nothing_open() {
        use std::sync::Arc;

        let directory = tempfile::tempdir().expect("a scratch directory");
        let target = directory.path().join("slow.kdbx");

        // Heavy enough that the unlock is still deriving a key when the screen
        // locks. The fixtures open in a few milliseconds, which is too fast to
        // be inside.
        let session = Arc::new(Session::new(None, None));
        session.making(target.clone());
        session.measured(Work::at(40));
        session.create(password(SECRET)).expect("the vault is made");
        assert!(session.lock(Reason::ByHand));

        let unlocking = {
            let session = Arc::clone(&session);
            std::thread::spawn(move || session.unlock(password(SECRET), LockPolicy::Respect))
        };

        // Long enough for the unlock to be inside key derivation, and short
        // enough that it is still there.
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(
            !session.lock(Reason::ScreenLocked),
            "the unlock had already finished, so this proved nothing"
        );
        let landed = unlocking.join().expect("the unlocking thread finishes");

        assert!(landed.is_err(), "a vault opened after the screen locked");
        assert!(!session.is_unlocked());
    }

    /// The menu bar asks whether a vault is open every time what the window
    /// offers changes. A save holds the session for a key derivation, and an
    /// answer that waited behind it left Lock Vault and Move to Recycle Bin
    /// grey - their keys dead - for as long as the save took.
    #[test]
    fn whether_a_vault_is_open_is_answered_while_a_save_holds_the_session() {
        use std::sync::{Arc, mpsc};
        use std::time::Duration;

        /// Asks from another thread while this one holds the session the way a
        /// save does, and gives up after five seconds.
        fn asked_behind_a_save(session: &Arc<Session>) -> Result<bool, mpsc::RecvTimeoutError> {
            let saving = session.held();
            let (answer, answered) = mpsc::channel();
            let asking = {
                let session = Arc::clone(session);
                std::thread::spawn(move || answer.send(session.is_unlocked()))
            };
            let open = answered.recv_timeout(Duration::from_secs(5));
            drop(saving);
            let _ = asking.join();
            open
        }

        let (_directory, _database, session) = holding(RICH);
        let session = Arc::new(session);
        assert_eq!(
            asked_behind_a_save(&session),
            Ok(true),
            "the answer waited for the save"
        );

        assert!(session.lock(Reason::ByHand));
        assert_eq!(
            asked_behind_a_save(&session),
            Ok(false),
            "a locked vault was said to be open"
        );
    }

    /// A close from Rust waits out the page's grace only while nothing holds
    /// the session: a page whose drafts are queued behind a save is still
    /// answering. So whether it is held is asked without joining the queue -
    /// and a session a panic left poisoned is held by nobody, or the close
    /// would wait for it for good.
    #[test]
    fn whether_the_session_is_held_is_answered_without_waiting_for_it() {
        use std::sync::mpsc;
        use std::time::Duration;

        let (_directory, _database, session) = holding(RICH);
        let session = Arc::new(session);
        assert!(
            !session.busy(),
            "a session nobody holds was said to be held"
        );

        let saving = session.held();
        let (answer, answered) = mpsc::channel();
        let asking = {
            let session = Arc::clone(&session);
            std::thread::spawn(move || answer.send(session.busy()))
        };
        let held = answered.recv_timeout(Duration::from_secs(5));
        drop(saving);
        let _ = asking.join();
        assert_eq!(
            held,
            Ok(true),
            "the question waited for the save, or missed it"
        );
        assert!(
            !session.busy(),
            "a session let go of was still said to be held"
        );

        let poisoning = Arc::clone(&session);
        let panicked = std::thread::spawn(move || {
            let _held = poisoning.held();
            panic!("a panic while the session is held");
        })
        .join();
        assert!(panicked.is_err());
        assert!(
            !session.busy(),
            "a session a panic let go of was said to be held for good"
        );
    }

    /// The place the creation screen offers by default is a folder that does not
    /// exist until the first vault goes into it, so making one has to make the
    /// folder too - and a folder it cannot make is a refusal that still says
    /// something true rather than a panic or a half-written file.
    #[test]
    fn a_vault_makes_the_folder_it_is_going_into() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let target = directory.path().join("Coffer/nested/deeper/vault.kdbx");

        let session = Session::new(None, None);
        session.making(target.clone());
        session.measured(Work::at(1));
        session.create(password(SECRET)).expect("the vault is made");

        assert!(target.is_file(), "the vault is not where it was asked for");
        assert!(session.is_unlocked());

        // A folder whose parent is a file. `create_dir_all` cannot make it, and
        // neither could anything else.
        let blocked = directory.path().join("a-file");
        std::fs::write(&blocked, b"not a folder").expect("the file is written");

        let session = Session::new(None, None);
        session.making(blocked.join("under/vault.kdbx"));
        session.measured(Work::at(1));
        assert!(session.create(password(SECRET)).is_err());
        assert!(
            !session.is_unlocked(),
            "a vault opened that was never written"
        );
        assert_eq!(
            std::fs::read(&blocked).expect("the file is still there"),
            b"not a folder",
            "the file in the way was written over"
        );
    }

    /// Making a vault is an unlock that happens to write the file first, and it
    /// is held to the same terms.
    #[test]
    fn a_vault_made_here_is_the_one_that_ends_up_open() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let target = directory.path().join("made.kdbx");

        let session = Session::new(None, None);
        session.making(target.clone());
        session.measured(Work::at(1));
        session.create(password(SECRET)).expect("the vault is made");

        assert!(session.is_unlocked());
        assert_eq!(session.tree().expect("the tree comes back").name, "made");
        assert_eq!(
            session.database(),
            Some(target.canonicalize().expect("the file is there"))
        );

        // The vault made is the vault open, so there is nothing left to make.
        assert!(session.create(password(SECRET)).is_err());
    }

    /// The screen measures the machine as it opens and asks where to put the
    /// vault afterwards, so the measurement has to survive the question. It did
    /// not, and a creation was refused for want of a number it already had.
    #[test]
    fn a_measurement_survives_being_asked_where_the_vault_goes() {
        let directory = tempfile::tempdir().expect("a scratch directory");

        for (round, order) in [true, false].into_iter().enumerate() {
            let target = directory.path().join(format!("either-way-{round}.kdbx"));
            let session = Session::new(None, None);

            // Both orders, because either is a screen somebody might build and
            // only one of them used to work.
            if order {
                session.measured(Work::at(1));
                session.making(target.clone());
            } else {
                session.making(target.clone());
                session.measured(Work::at(1));
            }

            session
                .create(password(SECRET))
                .expect("the vault is made whichever way round the two arrived");
            assert!(session.is_unlocked());
        }
    }

    /// Nothing is written until both halves of the answer are there.
    #[test]
    fn a_vault_cannot_be_made_before_it_is_known_where_or_how_hard() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let target = directory.path().join("made.kdbx");

        let session = Session::new(None, None);
        assert!(session.create(password(SECRET)).is_err());

        session.making(target.clone());
        assert!(session.create(password(SECRET)).is_err());
        assert!(!target.exists(), "a vault was written with no measurement");

        session.measured(Work::at(1));
        assert!(session.create(password(SECRET)).is_ok());
    }

    /// The same race the unlock has. A creation that finishes after the session
    /// was pointed somewhere else must not land, and the file it wrote is still
    /// on the disk for whoever wants it.
    #[test]
    fn a_creation_that_finishes_late_does_not_win() {
        use std::sync::Arc;

        let directory = tempfile::tempdir().expect("a scratch directory");
        let target = directory.path().join("made.kdbx");
        let (_other, elsewhere) = scratch(RICH);
        let config = tempfile::tempdir().expect("a scratch directory");

        let session = Arc::new(Session::new(None, Some(config.path().to_path_buf())));
        session.making(target.clone());
        // Heavy enough that the creation is still deriving a key when the
        // session is pointed elsewhere. At one pass it would be finished before
        // the racing thread had started, and the test would prove nothing.
        session.measured(Work::at(40));

        let making = {
            let session = Arc::clone(&session);
            std::thread::spawn(move || session.create(password(SECRET)))
        };

        // Long enough for the creation to be inside key derivation, and short
        // enough that it is still there.
        std::thread::sleep(std::time::Duration::from_millis(50));
        session.choose(elsewhere.clone());
        let _ = making.join();

        assert_eq!(
            session.database(),
            Some(elsewhere.canonicalize().expect("the copy is there"))
        );
        assert!(!session.is_unlocked());
        // A vault that did not land is not the one the next launch offers.
        assert_eq!(recent::remembered(config.path()), None);
    }

    /// A session writing down what opens into a directory of its own, pointed
    /// at a copy of a fixture.
    fn remembering(name: &str) -> (tempfile::TempDir, PathBuf, tempfile::TempDir, Session) {
        let (directory, database) = scratch(name);
        let config = tempfile::tempdir().expect("a scratch directory");
        let session = Session::new(Some(database.clone()), Some(config.path().to_path_buf()));
        (directory, database, config, session)
    }

    /// The vault the next launch offers is the one that opened, and only once
    /// it has. A pick that never opened, and a password that did not fit, write
    /// nothing down.
    #[test]
    fn an_unlock_writes_the_vault_down_for_the_next_launch() {
        let (_directory, database, config, session) = remembering(RICH);

        assert!(
            session
                .unlock(password(b"not it"), LockPolicy::Respect)
                .is_err()
        );
        assert_eq!(recent::remembered(config.path()), None);

        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the database opens");
        let opened = database.canonicalize().expect("the copy is there");
        assert_eq!(recent::remembered(config.path()), Some(opened.clone()));

        let (_elsewhere, other) = scratch(RICH);
        session.choose(other);
        assert_eq!(
            recent::remembered(config.path()),
            Some(opened),
            "a file that was only picked was written down"
        );
    }

    /// The defect this exists for: a vault made here was never picked in a
    /// panel, so it was never written down, and the next launch showed its
    /// owner the screen for somebody who had nothing.
    #[test]
    fn a_vault_made_here_is_written_down_for_the_next_launch() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let target = directory.path().join("Coffer/vault.kdbx");
        let config = tempfile::tempdir().expect("a scratch directory");

        let session = Session::new(None, Some(config.path().to_path_buf()));
        session.making(target.clone());
        session.measured(Work::at(1));
        session.create(password(SECRET)).expect("the vault is made");

        assert_eq!(
            recent::remembered(config.path()),
            Some(target.canonicalize().expect("the vault is there"))
        );

        // And the next launch, which starts from what was written down, is
        // pointed at it and opens it.
        session.lock(Reason::Quitting);
        let next = Session::new(
            recent::remembered(config.path()),
            Some(config.path().to_path_buf()),
        );
        next.unlock(password(SECRET), LockPolicy::Respect)
            .expect("the next launch opens the vault it was pointed at");
    }

    /// A snapshot and the copy a lock left both open with the vault's password
    /// and are older than it. However one came to be open, the vault that was
    /// written down stays written down.
    #[test]
    fn a_snapshot_or_a_rescue_copy_is_never_written_down() {
        let (_directory, database, config, session) = remembering(RICH);
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the database opens");
        session.lock(Reason::ByHand);
        let vault = database.canonicalize().expect("the copy is there");

        let snapshot = vault_core::storage::snapshot::slot(&vault, 1).expect("a slot has a name");
        let rescue = vault_core::storage::unsaved::beside(&vault).expect("a sibling path");
        for sibling in [snapshot, rescue] {
            std::fs::copy(fixture(RICH), &sibling).expect("the copy is written");
            session.choose_sibling(sibling.clone());
            session
                .unlock(password(SECRET), LockPolicy::Respect)
                .expect("the sibling opens with the same password");
            assert_eq!(
                recent::remembered(config.path()),
                Some(vault.clone()),
                "{} was written down as the vault",
                sibling.display()
            );
            session.lock(Reason::ByHand);
            session.choose(vault.clone());
        }
    }

    /// A configuration directory that cannot be written is a vault the reader
    /// picks again next launch. It is not a vault that refuses to open now,
    /// by either way in.
    #[test]
    fn a_vault_that_cannot_be_written_down_still_opens() {
        let (directory, database) = scratch(RICH);
        let blocked = directory.path().join("a-file");
        std::fs::write(&blocked, b"not a folder").expect("the file is written");
        let nowhere = blocked.join("Application Support/Coffer");

        let session = Session::new(Some(database), Some(nowhere.clone()));
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the database opens without being written down");
        assert!(session.is_unlocked());

        let session = Session::new(None, Some(nowhere.clone()));
        session.making(directory.path().join("made.kdbx"));
        session.measured(Work::at(1));
        session
            .create(password(SECRET))
            .expect("the vault is made without being written down");
        assert!(session.is_unlocked());
        assert_eq!(recent::remembered(&nowhere), None);
    }

    /// Both ways in end in one door, and that door is the one place a vault is
    /// put into the session and the one place one is written down. A third way
    /// in that set the vault itself would be a vault the next launch had
    /// forgotten, which is exactly the defect the door exists for.
    ///
    /// Read out of the source, because nothing else can see a way in that has
    /// not been written yet.
    #[test]
    fn every_way_a_vault_comes_to_be_open_is_written_down() {
        use crate::source::{functions, shipped};

        let source = shipped(include_str!("session.rs"));
        let putting: Vec<String> = functions(source)
            .into_iter()
            .filter(|body| body.contains("held.open.put(Some("))
            .collect();
        assert_eq!(putting.len(), 1, "a vault is put into the session twice");
        assert!(
            putting
                .iter()
                .all(|body| body.contains("fn land(") && body.contains("self.write_down(")),
            "the vault is put into the session somewhere that does not write it down"
        );

        // One place writes a vault down, and the door is one of its callers.
        let remembering: Vec<String> = functions(source)
            .into_iter()
            .filter(|body| body.contains("recent::remember("))
            .collect();
        assert_eq!(
            remembering.len(),
            1,
            "a vault is written down in two places"
        );
        assert!(
            remembering
                .iter()
                .all(|body| body.contains("fn write_down(")),
            "something other than write_down writes a vault down"
        );

        let opening: Vec<String> = functions(source)
            .into_iter()
            .filter(|body| body.contains("Vault::open(") || body.contains("Vault::create("))
            .collect();
        assert_eq!(
            opening.len(),
            2,
            "an unlock and a creation, and nothing else"
        );
        for body in opening {
            assert!(
                body.contains("self.land("),
                "a vault is opened without going through the door:\n{}",
                body.lines().take(3).collect::<Vec<_>>().join("\n")
            );
        }

        // And nowhere else writes one down: a pick is not an opening.
        for (file, other) in [
            ("commands.rs", shipped(include_str!("commands.rs"))),
            ("lib.rs", shipped(include_str!("lib.rs"))),
        ] {
            assert!(
                !other.contains("recent::remember("),
                "{file} writes a vault down that has not opened"
            );
        }
    }

    /// Every file on an entry, with its bytes, as the session hands them out.
    fn files_of(session: &Session, id: EntryId) -> Vec<(String, Vec<u8>)> {
        let entry = session.entry(id).expect("the entry comes back");
        let mut found: Vec<(String, Vec<u8>)> = entry
            .attachments
            .iter()
            .map(|attachment| {
                let bytes = session
                    .with(|vault| vault.attachment(id, &attachment.name))
                    .expect("the session is open")
                    .expect("the file is there");
                (attachment.name.clone(), bytes.expose().to_vec())
            })
            .collect();
        found.sort();
        found
    }

    fn code_of(failure: &Failure) -> String {
        serde_json::to_value(failure).expect("a failure serialises")["code"]
            .as_str()
            .expect("a code is a string")
            .to_owned()
    }

    const KEY: &str = "id_ed25519";

    /// The file that was chosen goes on once, when the reader answers, and not
    /// before and not twice.
    #[test]
    fn a_file_whose_name_is_taken_waits_for_the_answer_and_goes_on_once() {
        let (_scratch, session) = unlocked(RICH);
        let id = entry_titled(&session, "ssh key").id;
        let before = files_of(&session, id);

        let asked = session
            .offer(id, KEY.to_owned(), Zeroizing::new(b"a new key".to_vec()))
            .expect("the file is offered");
        assert!(matches!(asked, Attached::Taken(_)));
        assert_eq!(
            files_of(&session, id),
            before,
            "the offer alone changed the entry"
        );

        session.answer(id, Vault::keep_both).expect("both are kept");
        let mut wanted = before.clone();
        wanted.push((format!("{KEY} 2"), b"a new key".to_vec()));
        wanted.sort();
        assert_eq!(files_of(&session, id), wanted);

        // The same answer again, as a second press or a message that arrived
        // twice would send it: there is nothing left waiting to put anywhere.
        let again = session
            .answer(id, Vault::keep_both)
            .expect_err("the file went on once");
        assert_eq!(code_of(&again), "refused");
        assert_eq!(files_of(&session, id), wanted);
    }

    /// The window names the entry the question was about, and only the file
    /// chosen for that entry answers to it. A message about any other entry -
    /// an answer or a withdrawal - leaves the file where it is.
    #[test]
    fn a_file_chosen_for_one_entry_never_goes_on_another() {
        let (_scratch, session) = unlocked(RICH);
        let key = entry_titled(&session, "ssh key").id;
        let other = entry_titled(&session, "basic").id;
        let (theirs, mine) = (files_of(&session, other), files_of(&session, key));

        let _ = session
            .offer(key, KEY.to_owned(), Zeroizing::new(b"a new key".to_vec()))
            .expect("the file is offered");

        for answer in [Vault::keep_both, Vault::replace_attachment] {
            assert!(session.answer(other, answer).is_err());
        }
        session.withdraw(other);
        assert_eq!(
            files_of(&session, other),
            theirs,
            "it went on the wrong entry"
        );
        assert_eq!(files_of(&session, key), mine);

        session
            .answer(key, Vault::replace_attachment)
            .expect("the file is still waiting on the entry it was chosen for");
        assert!(
            files_of(&session, key).contains(&(KEY.to_owned(), b"a new key".to_vec())),
            "the replacement did not go on"
        );
    }

    /// Every way the vault goes takes the file with it. A lock in particular:
    /// the question alone must not make the lock write the vault, and after it
    /// an answer from the window that was drawn before finds nothing to put.
    #[test]
    fn the_file_waiting_goes_whenever_the_vault_does() {
        type Going = fn(&Session, &Path);
        let ways: [(&str, Going); 3] = [
            ("a lock", |session, _| {
                assert!(session.lock(Reason::Idle));
            }),
            ("another unlock", |_, _| {}),
            ("the database chosen again", |session, database| {
                session.choose(database.to_path_buf());
            }),
        ];

        for (way, going) in ways {
            let (_scratch, database) = scratch(RICH);
            let session = Session::new(Some(database.clone()), None);
            session
                .unlock(password(SECRET), LockPolicy::Respect)
                .expect("the database opens");
            let id = entry_titled(&session, "ssh key").id;
            let before = files_of(&session, id);

            let _ = session
                .offer(id, KEY.to_owned(), Zeroizing::new(b"a new key".to_vec()))
                .expect("the file is offered");
            going(&session, &database);

            let snapshot =
                vault_core::storage::snapshot::slot(&database, 1).expect("a slot has a name");
            assert!(
                !snapshot.exists(),
                "{way}: an unanswered question left the vault something to write"
            );

            session
                .unlock(password(SECRET), LockPolicy::Respect)
                .expect("the database opens again");
            for answer in [Vault::keep_both, Vault::replace_attachment] {
                let refused = session
                    .answer(id, answer)
                    .expect_err("nothing is waiting any more");
                assert_eq!(code_of(&refused), "refused", "{way}");
            }
            assert_eq!(files_of(&session, id), before, "{way}");
        }
    }

    /// A file waiting on an entry goes with the entry, under the lock that takes
    /// the entry away: a reload, the entry deleted, the folder it is in deleted,
    /// the bin it is in emptied. An answer that was already on its way, or
    /// queued behind the change, would otherwise land on a vault the reader
    /// was never asked about, and a replacement would go over a file they never
    /// saw.
    #[test]
    fn the_file_waiting_goes_with_its_entry() {
        type Step = fn(&Session, EntryId);
        let nothing: Step = |_, _| {};
        let ways: [(&str, Step, Step); 4] = [
            (
                "a reload",
                |session, _| {
                    session
                        .with_mut(Vault::save)
                        .expect("the session is open")
                        .expect("the entry reaches the file");
                },
                |session, _| {
                    session
                        .overtaking(Over::Everything, 1, Vault::reload)
                        .expect("the session is open")
                        .expect("the file reads again");
                },
            ),
            ("the entry deleted", nothing, |session, id| {
                session
                    .overtaking(Over::Entries(&[id]), 1, |vault| {
                        vault.delete_entries(&[(id, Deletion::Bin)])
                    })
                    .expect("the session is open")
                    .expect("the entry goes");
            }),
            ("its folder deleted", nothing, |session, id| {
                let folder = session.entry(id).expect("the entry is there").group;
                session
                    .with_mut(|vault| vault.delete_group(folder, Deletion::Bin))
                    .expect("the session is open")
                    .expect("the folder goes");
            }),
            (
                "the bin it is in emptied",
                |session, id| {
                    session
                        .with_mut(|vault| vault.delete_entries(&[(id, Deletion::Bin)]))
                        .expect("the session is open")
                        .expect("the entry goes to the bin");
                },
                |session, _| {
                    session
                        .with_mut(Vault::empty_recycle_bin)
                        .expect("the session is open")
                        .expect("the bin empties");
                },
            ),
        ];

        for (way, before, going) in ways {
            let (_scratch, session) = unlocked(RICH);
            let root = session.tree().expect("the tree comes back").id;
            let id = session
                .with_mut(|vault| -> Result<EntryId, VaultError> {
                    let folder = vault.create_group(root, "Keys")?;
                    vault.create_entry(folder, Kind::Login)
                })
                .expect("the session is open")
                .expect("an entry is made in a folder of its own");
            let added = session
                .offer(id, KEY.to_owned(), Zeroizing::new(b"the key".to_vec()))
                .expect("the file goes on");
            assert!(matches!(added, Attached::Added), "{way}");
            before(&session, id);

            let asked = session
                .offer(id, KEY.to_owned(), Zeroizing::new(b"a new key".to_vec()))
                .expect("the file is offered");
            assert!(matches!(asked, Attached::Taken(_)), "{way}");
            going(&session, id);

            for answer in [Vault::keep_both, Vault::replace_attachment] {
                let refused = session
                    .answer(id, answer)
                    .expect_err("nothing is waiting any more");
                assert_eq!(code_of(&refused), "refused", "{way}");
            }
            if session.entry(id).is_ok() {
                assert_eq!(
                    files_of(&session, id),
                    [(KEY.to_owned(), b"the key".to_vec())],
                    "{way}"
                );
            }
        }
    }

    /// Only a change that takes the entry away lets go of its file: the
    /// reader is still being asked about it.
    #[test]
    fn the_file_waiting_outlasts_a_change_that_leaves_its_entry() {
        let (_scratch, session) = unlocked(RICH);
        let id = entry_titled(&session, "ssh key").id;
        let other = entry_titled(&session, "basic").id;
        let root = session.tree().expect("the tree comes back").id;

        let _ = session
            .offer(id, KEY.to_owned(), Zeroizing::new(b"a new key".to_vec()))
            .expect("the file is offered");
        session
            .with_mut(|vault| vault.create_group(root, "Elsewhere"))
            .expect("the session is open")
            .expect("a folder is made");
        session
            .overtaking(Over::Entries(&[other]), 1, |vault| {
                vault.delete_entries(&[(other, Deletion::Bin)])
            })
            .expect("the session is open")
            .expect("another entry goes");

        session
            .answer(id, Vault::keep_both)
            .expect("the file is still waiting");
        assert!(
            files_of(&session, id).contains(&(format!("{KEY} 2"), b"a new key".to_vec())),
            "the answer did not put the file that was chosen"
        );
    }

    /// A file picked is a new question. The last one is let go, whichever entry
    /// it was for and whether or not the new one is asked about at all.
    #[test]
    fn choosing_another_file_lets_go_of_the_last_one() {
        let (_scratch, session) = unlocked(RICH);
        let key = entry_titled(&session, "ssh key").id;
        let other = entry_titled(&session, "basic").id;

        let _ = session
            .offer(key, KEY.to_owned(), Zeroizing::new(b"first pick".to_vec()))
            .expect("the file is offered");
        let _ = session
            .offer(key, KEY.to_owned(), Zeroizing::new(b"second pick".to_vec()))
            .expect("the file is offered");
        session
            .answer(key, Vault::keep_both)
            .expect("both are kept");
        assert!(
            files_of(&session, key).contains(&(format!("{KEY} 2"), b"second pick".to_vec())),
            "the answer put the file that was chosen first"
        );

        let _ = session
            .offer(key, KEY.to_owned(), Zeroizing::new(b"third pick".to_vec()))
            .expect("the file is offered");
        let added = session
            .offer(other, "fresh.txt".to_owned(), Zeroizing::new(b"x".to_vec()))
            .expect("the file is offered");
        assert!(matches!(added, Attached::Added));
        assert!(
            session.answer(key, Vault::keep_both).is_err(),
            "a pick for another entry left the last one waiting"
        );
    }

    /// A replacement the versions stand in the way of is refused with the code
    /// the entry screen answers, and the file stays waiting, so the reader can
    /// still keep both without choosing it again.
    #[test]
    fn a_refused_replacement_leaves_the_file_waiting() {
        let (_scratch, session) = unlocked(RICH);
        let id = entry_titled(&session, "basic").id;
        let _ = session
            .offer(id, "held.pem".to_owned(), Zeroizing::new(b"old".to_vec()))
            .expect("the file is offered");
        session
            .with_mut(|vault| {
                vault.set_field(
                    id,
                    fields::NOTES,
                    vault_core::NewValue::Open("a version that holds the file".to_owned()),
                )
            })
            .expect("the session is open")
            .expect("the note is written");
        let before = files_of(&session, id);

        let _ = session
            .offer(id, "held.pem".to_owned(), Zeroizing::new(b"new".to_vec()))
            .expect("the file is offered");
        let refused = session
            .answer(id, Vault::replace_attachment)
            .expect_err("the version holds the old one in place");
        assert_eq!(code_of(&refused), "attachmentInHistory");
        assert_eq!(files_of(&session, id), before);

        session
            .answer(id, Vault::keep_both)
            .expect("the file was still waiting");
        assert!(files_of(&session, id).contains(&("held 2.pem".to_owned(), b"new".to_vec())));
    }

    /// Nothing is open, so nothing is waiting and nothing can be answered - and
    /// letting go of nothing is not a failure.
    #[test]
    fn a_locked_session_has_no_file_waiting() {
        let (_scratch, database) = scratch(RICH);
        let session = Session::new(Some(database), None);
        let nobody = EntryId::from_uuid(uuid::Uuid::nil());

        session.withdraw(nobody);
        assert!(
            session
                .offer(nobody, KEY.to_owned(), Zeroizing::new(b"x".to_vec()))
                .is_err()
        );
        assert!(session.answer(nobody, Vault::keep_both).is_err());
    }

    /// Text the reader typed into a field and never left, as the window says
    /// it.
    fn words(text: &str, protect: bool) -> Option<Typed> {
        Some(Typed {
            value: Zeroizing::new(text.to_owned()),
            protect,
            typing: vault_core::Typing::InPlace,
        })
    }

    /// A new value typed in a Change field and never saved, as the window says
    /// it. Always for a protected value: that is what a Change is for.
    fn replacement(text: &str) -> Option<Typed> {
        Some(Typed {
            value: Zeroizing::new(text.to_owned()),
            protect: true,
            typing: vault_core::Typing::Beside,
        })
    }

    /// The database as the next launch would find it: from the file, with
    /// nothing of the session that wrote it.
    fn reopened(database: &Path) -> Vault {
        Vault::open(
            database,
            MasterKey::from_password(password(SECRET)),
            LockPolicy::Respect,
        )
        .expect("the database opens again")
    }

    /// One field's value, whether or not the file protects it.
    fn value_of(vault: &Vault, id: EntryId, field: &str) -> Option<String> {
        vault
            .reveal(id, field)
            .and_then(|secret| secret.expose_str().map(str::to_owned))
    }

    /// A session holding a copy of the fixture, and where that copy is.
    fn holding(name: &str) -> (tempfile::TempDir, PathBuf, Session) {
        let (directory, database) = scratch(name);
        let session = Session::new(Some(database.clone()), None);
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the database opens");
        (directory, database, session)
    }

    /// The defect this exists for: a note half written when the lid closed was
    /// in the window and nowhere else, and the lock that destroyed the window
    /// took it along. It is in the file now, as an edit like any other - the
    /// entry's previous state kept as a version - and the unlock screen is told
    /// that it was saved.
    #[test]
    fn a_lock_writes_what_the_reader_was_typing_and_saves_it() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");

        session
            .draft(
                basic.id,
                fields::URL,
                words("https://half.example/pa", false),
                1,
            )
            .expect("the draft is heard");

        assert!(session.lock(Reason::Sleeping));
        assert!(
            !session.lock(Reason::Quitting),
            "a second lock found a vault"
        );
        assert!(session.typed(), "the unlock screen is not told");
        assert!(
            !session.typed_beside(),
            "the unlock screen says a value was kept beside one"
        );
        assert!(!session.lost());

        let file = reopened(&database);
        assert_eq!(
            value_of(&file, basic.id, fields::URL).as_deref(),
            Some("https://half.example/pa")
        );
        assert_eq!(
            file.versions(basic.id).len(),
            basic.versions + 1,
            "the draft went in without keeping what it replaced"
        );
    }

    /// A draft still on its way when its field was written arrives after the
    /// write, and after a restore that came later still. It is older than both
    /// and must not be what the lock writes over them.
    #[test]
    fn a_draft_overtaken_by_a_write_never_comes_back() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");

        session
            .draft(basic.id, fields::URL, words("https://half.ex", false), 1)
            .expect("the draft is heard");
        session
            .overtaking(Over::Field(basic.id, fields::URL), 2, |vault| {
                vault.set_field(
                    basic.id,
                    fields::URL,
                    vault_core::NewValue::Open("https://whole.example".to_owned()),
                )
            })
            .expect("the vault is open")
            .expect("the address is written");
        let newest = session
            .with(|vault| vault.versions(basic.id))
            .expect("the vault is open")
            .last()
            .expect("the write kept a version")
            .index;
        session
            .with_mut(|vault| vault.restore_version(basic.id, newest))
            .expect("the vault is open")
            .expect("the version is restored");

        // The draft sent first, arriving last, twice over.
        for late in [1, 2] {
            session
                .draft(basic.id, fields::URL, words("https://half.ex", false), late)
                .expect("the draft is heard");
        }

        assert!(session.lock(Reason::Idle));
        assert!(!session.typed(), "a draft nobody stood behind was written");
        assert_eq!(
            value_of(&reopened(&database), basic.id, fields::URL).as_deref(),
            Some("https://example.com/login?a=1&b=2"),
            "the restore did not stand"
        );
    }

    /// Escape, Cancel and Discard take typing back, and what was taken back is
    /// not written - nor is there anything for the lock to save.
    #[test]
    fn typing_taken_back_is_not_written() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");

        session
            .draft(basic.id, fields::USERNAME, words("mallory", false), 1)
            .expect("the draft is heard");
        session
            .draft(basic.id, fields::USERNAME, None, 2)
            .expect("the draft is taken back");
        // A second copy of the first word, arriving after the second.
        session
            .draft(basic.id, fields::USERNAME, words("mallory", false), 1)
            .expect("the draft is heard");

        assert!(session.lock(Reason::ByHand));
        assert!(!session.typed());
        let snapshot =
            vault_core::storage::snapshot::slot(&database, 1).expect("a slot has a name");
        assert!(!snapshot.exists(), "a lock with nothing to write wrote");
        assert_eq!(
            value_of(&reopened(&database), basic.id, fields::USERNAME).as_deref(),
            Some("alice")
        );
    }

    /// Text that is what the field already holds is not an edit, and a lock
    /// that heard only that has nothing to save and nothing to say.
    #[test]
    fn typing_that_came_back_to_where_it_started_writes_nothing() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");

        session
            .draft(basic.id, fields::USERNAME, words("alice", false), 1)
            .expect("the draft is heard");
        // A new password that is the one there is nothing to keep beside it.
        session
            .draft(
                basic.id,
                fields::PASSWORD,
                replacement("correct horse battery staple"),
                2,
            )
            .expect("the draft is heard");
        // A standard field the entry does not have, left empty. Every entry is
        // drawn with one, and one Coffer did not make may well lack it.
        let made = session
            .with_mut(|vault| {
                let made = vault.create_entry(vault.tree().id, Kind::Login)?;
                vault.remove_field(made, fields::NOTES, false)?;
                vault.save()?;
                Ok::<_, VaultError>(made)
            })
            .expect("the vault is open")
            .expect("an entry without notes is made and saved");
        session
            .draft(made, fields::NOTES, words("", false), 3)
            .expect("the draft is heard");

        assert!(session.lock(Reason::Idle));
        assert!(!session.typed());
        assert_eq!(
            reopened(&database).versions(basic.id).len(),
            basic.versions,
            "an edit that changed nothing kept a version"
        );
    }

    /// The entry went, the field was removed, or the entry was never there: a
    /// draft of any of them is let go, and the lock goes on to write the rest.
    /// A field of the reader's own that was removed is not made again by its
    /// own late draft.
    #[test]
    fn drafts_of_what_has_gone_are_let_go_and_the_rest_are_written() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");
        let binned = entry_titled(&session, "deleted entry");

        session
            .with_mut(|vault| {
                vault.set_field(
                    basic.id,
                    "PIN",
                    vault_core::NewValue::Protected(Zeroizing::new("1234".to_owned())),
                )?;
                vault.remove_field(basic.id, "PIN", false)?;
                // Out of the bin, and so out of the file.
                vault.delete_entries(&[(binned.id, Deletion::Forever)])
            })
            .expect("the vault is open")
            .expect("the changes are made");

        let nobody = EntryId::from_uuid(uuid::Uuid::nil());
        for (sequence, (entry, field)) in [
            (binned.id, fields::NOTES),
            (basic.id, "PIN"),
            (nobody, fields::TITLE),
            (basic.id, "a name nothing ever had"),
        ]
        .into_iter()
        .enumerate()
        {
            session
                .draft(entry, field, words("lost cause", true), sequence as u64 + 1)
                .expect("the draft is heard");
        }
        // Text a KeePass file cannot hold is refused the way a commit of it
        // would be, and the drafts after it are still written.
        session
            .draft(basic.id, fields::USERNAME, words("null\0byte", false), 8)
            .expect("the draft is heard");
        session
            .draft(
                basic.id,
                fields::URL,
                words("https://kept.example", false),
                9,
            )
            .expect("the draft is heard");

        assert!(session.lock(Reason::ScreenLocked));
        assert!(session.typed());

        let file = reopened(&database);
        assert!(file.entry(binned.id).is_none(), "an erased entry came back");
        let entry = file.entry(basic.id).expect("the entry is there");
        assert!(entry.field("PIN").is_none(), "a removed field came back");
        assert!(entry.field("a name nothing ever had").is_none());
        assert_eq!(
            value_of(&file, basic.id, fields::URL).as_deref(),
            Some("https://kept.example")
        );
        assert_eq!(
            value_of(&file, basic.id, fields::USERNAME).as_deref(),
            Some("alice")
        );
    }

    /// A deletion and a reading of the file again each overtake what was typed
    /// before them, and a draft of that time arriving afterwards stays gone -
    /// the entry put back from the bin included.
    #[test]
    fn a_deletion_or_a_reload_overtakes_what_was_typed_before_it() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");

        session
            .draft(
                basic.id,
                fields::USERNAME,
                words("before the bin", false),
                1,
            )
            .expect("the draft is heard");
        session
            .overtaking(Over::Entries(&[basic.id]), 2, |vault| {
                vault.delete_entries(&[(basic.id, Deletion::Bin)])
            })
            .expect("the vault is open")
            .expect("the entry goes to the bin");
        session
            .draft(
                basic.id,
                fields::USERNAME,
                words("before the bin", false),
                1,
            )
            .expect("the draft is heard");
        session
            .with_mut(|vault| vault.put_back_entries(&[basic.id]))
            .expect("the vault is open")
            .expect("the entry comes back");
        session.with_mut(Vault::save).expect("open").expect("saved");

        session
            .draft(
                basic.id,
                fields::URL,
                words("https://before.reload", false),
                3,
            )
            .expect("the draft is heard");
        session
            .overtaking(Over::Everything, 4, Vault::reload)
            .expect("the vault is open")
            .expect("the file is read again");
        session
            .draft(
                basic.id,
                fields::URL,
                words("https://before.reload", false),
                3,
            )
            .expect("the draft is heard");

        assert!(session.lock(Reason::Idle));
        assert!(!session.typed());
        let file = reopened(&database);
        assert_eq!(
            value_of(&file, basic.id, fields::USERNAME).as_deref(),
            Some("alice")
        );
        assert_eq!(
            value_of(&file, basic.id, fields::URL).as_deref(),
            Some("https://example.com/login?a=1&b=2")
        );
    }

    /// A deletion of several entries lets go of what was typed into each of
    /// them, and of nothing else: the lock still writes what was typed into an
    /// entry the batch did not name.
    #[test]
    fn a_batch_deletion_lets_go_of_what_was_typed_into_every_entry_it_names() {
        let (_directory, database, session) = holding(RICH);
        let (basic, key, versioned) = (
            entry_titled(&session, "basic").id,
            entry_titled(&session, "ssh key").id,
            entry_titled(&session, "versioned").id,
        );
        for (sequence, id) in [basic, key, versioned].into_iter().enumerate() {
            session
                .draft(
                    id,
                    fields::NOTES,
                    words("typed and never left", false),
                    sequence as u64 + 1,
                )
                .expect("the draft is heard");
        }

        session
            .overtaking(Over::Entries(&[basic, key]), 4, |vault| {
                vault.delete_entries(&[(basic, Deletion::Bin), (key, Deletion::Bin)])
            })
            .expect("the vault is open")
            .expect("both go to the bin");

        assert!(session.lock(Reason::Idle));
        let file = reopened(&database);
        for id in [basic, key] {
            assert_ne!(
                value_of(&file, id, fields::NOTES).as_deref(),
                Some("typed and never left"),
                "typing in a deleted entry was written"
            );
            assert!(file.entry(id).is_some_and(|entry| entry.binned.is_some()));
        }
        assert_eq!(
            value_of(&file, versioned, fields::NOTES).as_deref(),
            Some("typed and never left"),
            "typing in an entry the batch did not name was let go"
        );
    }

    /// A batch refused - one of its entries would no longer go where the
    /// window said - deletes nothing, and still lets go of the typing said
    /// before it: the reader chose to leave those panes behind, as
    /// [`Session::overtaking`] says.
    #[test]
    fn a_refused_batch_deletion_still_lets_go_of_typing_said_before_it() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");
        let key = entry_titled(&session, "ssh key");
        for (sequence, id) in [basic.id, key.id].into_iter().enumerate() {
            session
                .draft(
                    id,
                    fields::NOTES,
                    words("typed and never left", false),
                    sequence as u64 + 1,
                )
                .expect("the draft is heard");
        }

        let refused = session
            .overtaking(Over::Entries(&[basic.id, key.id]), 3, |vault| {
                vault.delete_entries(&[(basic.id, Deletion::Bin), (key.id, Deletion::Forever)])
            })
            .expect("the vault is open");
        assert!(matches!(refused, Err(VaultError::DeletionChanged)));

        assert!(session.lock(Reason::Idle));
        assert!(!session.typed());
        let file = reopened(&database);
        for entry in [&basic, &key] {
            let kept = file.entry(entry.id).expect("nothing was deleted");
            assert_eq!(kept.group, entry.group, "a refused batch moved an entry");
            assert_ne!(
                value_of(&file, entry.id, fields::NOTES).as_deref(),
                Some("typed and never left")
            );
        }
    }

    /// The file waiting on an entry goes when a batch takes that entry away,
    /// under the batch's own lock, whichever entry of the batch it was.
    #[test]
    fn the_file_waiting_goes_with_any_entry_a_batch_takes_away() {
        let (_scratch, session) = unlocked(RICH);
        let basic = entry_titled(&session, "basic").id;
        let key = entry_titled(&session, "ssh key").id;
        let before = files_of(&session, key);

        let asked = session
            .offer(key, KEY.to_owned(), Zeroizing::new(b"a new key".to_vec()))
            .expect("the file is offered");
        assert!(matches!(asked, Attached::Taken(_)));
        session
            .overtaking(Over::Entries(&[basic, key]), 1, |vault| {
                vault.delete_entries(&[(basic, Deletion::Bin), (key, Deletion::Bin)])
            })
            .expect("the vault is open")
            .expect("both go to the bin");

        for answer in [Vault::keep_both, Vault::replace_attachment] {
            let refused = session
                .answer(key, answer)
                .expect_err("nothing is waiting any more");
            assert_eq!(code_of(&refused), "refused");
        }
        assert_eq!(files_of(&session, key), before);
    }

    /// However many entries a batch changes, the window has one list of
    /// versions to read again, and a batch refused has changed nothing it
    /// would need to.
    #[test]
    fn a_batch_moves_the_revision_once_and_a_refused_one_not_at_all() {
        let (_scratch, session) = unlocked(RICH);
        let ids: Vec<EntryId> = ["basic", "ssh key", "versioned"]
            .into_iter()
            .map(|title| entry_titled(&session, title).id)
            .collect();
        let revision = || {
            session
                .listing(|vault| vault.versions(ids[0]))
                .expect("the vault is open")
                .0
        };

        let start = revision();
        type Batch = fn(&mut Vault, &[EntryId]) -> Result<(), VaultError>;
        type Refusal = fn(&VaultError) -> bool;
        let refused: [(&str, Batch, Refusal); 4] = [
            (
                "a deletion naming an entry that is not there",
                |vault, ids| {
                    let mut shown: Vec<_> = ids.iter().map(|&id| (id, Deletion::Bin)).collect();
                    shown.push((EntryId::from_uuid(uuid::Uuid::nil()), Deletion::Bin));
                    vault.delete_entries(&shown)
                },
                |error| matches!(error, VaultError::NoSuchEntry),
            ),
            (
                "a put back of entries outside the bin",
                |vault, ids| vault.put_back_entries(ids),
                |error| matches!(error, VaultError::NotInRecycleBin),
            ),
            (
                "a tag the format would split",
                |vault, ids| vault.tag_entries(ids, "a;b").map(drop),
                |error| matches!(error, VaultError::UnwritableTag),
            ),
            (
                "a tag taken off an entry that is not there",
                |vault, ids| {
                    let mut named = ids.to_vec();
                    named.push(EntryId::from_uuid(uuid::Uuid::nil()));
                    vault.untag_entries(&named, "work")
                },
                |error| matches!(error, VaultError::NoSuchEntry),
            ),
        ];
        for (what, batch, refusal) in refused {
            let done = session
                .with_mut(|vault| batch(vault, &ids))
                .expect("the vault is open");
            assert!(
                done.as_ref().is_err_and(refusal),
                "{what} was not refused for its own reason: {done:?}"
            );
            assert_eq!(revision(), start, "{what} moved the revision");
        }

        let tagged = session
            .with_mut(|vault| vault.tag_entries(&ids, "batched"))
            .expect("the vault is open")
            .expect("the tag goes on");
        assert_eq!(tagged, ids);
        assert_eq!(revision(), start + 1, "three versions are one change");
        session
            .with_mut(|vault| vault.tag_entries(&ids, "batched"))
            .expect("the vault is open")
            .expect("a tag every entry has already writes nothing");
        assert_eq!(revision(), start + 1, "a batch with nothing to do moved it");

        let shown: Vec<_> = ids.iter().map(|&id| (id, Deletion::Bin)).collect();
        session
            .with_mut(|vault| vault.delete_entries(&shown))
            .expect("the vault is open")
            .expect("all three go to the bin");
        assert_eq!(revision(), start + 2);
        session
            .with_mut(|vault| vault.put_back_entries(&ids))
            .expect("the vault is open")
            .expect("all three come back");
        assert_eq!(revision(), start + 3);
    }

    /// What a batch hands the window - the tree, and the entries a tag went
    /// on - carries no password the vault holds: not one any entry has now,
    /// and not one a version of it keeps, however many entries it changed.
    #[test]
    fn nothing_a_batch_answers_carries_a_password() {
        fn walk(group: &Project, into: &mut Vec<EntryId>) {
            into.extend(group.entries.iter().map(|entry| entry.id));
            for section in &group.sections {
                walk(section, into);
            }
        }

        let (_scratch, session) = unlocked(RICH);
        let mut ids = Vec::new();
        walk(&session.tree().expect("the tree comes back"), &mut ids);
        let mut secrets: Vec<String> = ids
            .iter()
            .filter_map(|&id| password_of(&session, id))
            .collect();
        session
            .with(|vault| {
                for &id in &ids {
                    for version in vault.versions(id) {
                        let kept = vault.reveal_version(id, version.index, fields::PASSWORD);
                        if let Some(text) = kept.as_ref().and_then(|secret| secret.expose_str()) {
                            secrets.push(text.to_owned());
                        }
                    }
                }
            })
            .expect("the vault is open");
        secrets.retain(|secret| !secret.is_empty());
        assert!(
            secrets.len() > ids.len() / 2,
            "the fixture holds few passwords"
        );
        let carried = |payload: &str, what: &str| {
            for secret in &secrets {
                assert!(
                    !payload.contains(secret.as_str()),
                    "{what} carries a password"
                );
            }
        };

        let changed = session
            .with_mut(|vault| vault.tag_entries(&ids, "batched"))
            .expect("the vault is open")
            .expect("the tag goes on");
        assert_eq!(changed.len(), ids.len());
        let tagged =
            crate::dto::Tagged::of(&session.tree().expect("the tree comes back"), &changed);
        carried(
            &serde_json::to_string(&tagged).expect("it serialises"),
            "a tag's answer",
        );

        let live: Vec<_> = ids
            .iter()
            .map(|&id| (id, session.entry(id).expect("the entry is there").deletion))
            .filter(|(_, deletion)| *deletion == Deletion::Bin)
            .collect();
        assert!(!live.is_empty());
        let binned: Vec<_> = live.iter().map(|&(id, _)| id).collect();
        for (what, batch) in [
            (
                "a deletion's tree",
                Box::new(|vault: &mut Vault| vault.delete_entries(&live))
                    as Box<dyn FnOnce(&mut Vault) -> Result<(), VaultError>>,
            ),
            (
                "a put back's tree",
                Box::new(|vault: &mut Vault| vault.put_back_entries(&binned)),
            ),
        ] {
            session
                .with_mut(batch)
                .expect("the vault is open")
                .expect("the batch goes through");
            let tree = crate::dto::Group::of(&session.tree().expect("the tree comes back"));
            carried(&serde_json::to_string(&tree).expect("it serialises"), what);
        }
    }

    /// A value the database protects goes back protected: a draft is written
    /// under the protection the window read, a field typed into where it
    /// stands and a new password kept beside the old one alike.
    #[test]
    fn a_protected_fields_draft_is_written_protected() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");
        session
            .with_mut(|vault| {
                vault.set_field(
                    basic.id,
                    "Passport",
                    vault_core::NewValue::Protected(Zeroizing::new(String::new())),
                )
            })
            .expect("the vault is open")
            .expect("the field is made");

        session
            .draft(
                basic.id,
                fields::PASSWORD,
                replacement("a new one, half"),
                1,
            )
            .expect("the draft is heard");
        session
            .draft(basic.id, "Passport", words("C01X00T4", true), 2)
            .expect("the draft is heard");
        assert!(session.lock(Reason::Sleeping));

        let file = reopened(&database);
        let entry = file.entry(basic.id).expect("the entry is there");
        for (name, wanted) in [
            (fields::PASSWORD, "correct horse battery staple"),
            ("Password (typed before locking)", "a new one, half"),
            ("Passport", "C01X00T4"),
        ] {
            let field = entry.field(name).expect("the field is there");
            assert!(
                field.value.open().is_none(),
                "{name} was written in the open"
            );
            assert_eq!(value_of(&file, basic.id, name).as_deref(), Some(wanted));
        }
    }

    /// The harm a Change field exists to prevent, done by the lock instead. A
    /// reader pressed Change on "Home Wi-Fi", typed the first half of a new
    /// password, and walked away with the question under it unanswered; the
    /// idle lock wrote "Tr0ub" over the router's password and the next copy
    /// handed that to a router that rejects it. The password stays what it was
    /// now, what was typed is kept beside it in a field that says what it is,
    /// and the unlock screen is told so: "saved" on its own reads as the new
    /// password being the entry's. A note typed into the same entry and written
    /// where it was typed does not make that go unsaid.
    #[test]
    fn a_lock_keeps_a_new_password_half_typed_beside_the_old_one() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");

        session
            .draft(basic.id, fields::PASSWORD, replacement("Tr0ub"), 1)
            .expect("the draft is heard");
        session
            .draft(basic.id, fields::NOTES, words("and a note", false), 2)
            .expect("the draft is heard");
        assert!(session.lock(Reason::Idle));
        assert!(session.typed(), "the unlock screen is not told");
        assert!(
            session.typed_beside(),
            "the unlock screen does not say the new password is beside the old"
        );

        let file = reopened(&database);
        assert_eq!(
            value_of(&file, basic.id, fields::PASSWORD).as_deref(),
            Some("correct horse battery staple"),
            "the lock wrote a password nobody saved over the real one"
        );
        assert_eq!(
            value_of(&file, basic.id, "Password (typed before locking)").as_deref(),
            Some("Tr0ub")
        );
        assert_eq!(
            value_of(&file, basic.id, fields::NOTES).as_deref(),
            Some("and a note")
        );
        assert_eq!(file.versions(basic.id).len(), basic.versions + 2);
    }

    /// "Set one" on an entry with no password has nothing to keep a new one
    /// beside, and the lock writes it where the reader was putting it.
    #[test]
    fn a_lock_writes_a_first_password_half_typed_into_the_field() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");
        session
            .with_mut(|vault| {
                vault.set_field(
                    basic.id,
                    fields::PASSWORD,
                    vault_core::NewValue::Protected(Zeroizing::new(String::new())),
                )
            })
            .expect("the vault is open")
            .expect("the password is emptied");

        session
            .draft(basic.id, fields::PASSWORD, replacement("first one"), 1)
            .expect("the draft is heard");
        assert!(session.lock(Reason::Sleeping));
        assert!(session.typed());
        assert!(!session.typed_beside(), "nothing was kept beside anything");

        let file = reopened(&database);
        assert_eq!(
            value_of(&file, basic.id, fields::PASSWORD).as_deref(),
            Some("first one")
        );
        assert!(
            file.entry(basic.id)
                .expect("the entry is there")
                .field("Password (typed before locking)")
                .is_none()
        );
    }

    /// A Change field typed into and emptied again holds no new password. The
    /// window takes such a draft back; one that arrives all the same writes
    /// nothing, empties nothing, and gives the lock nothing to save or say.
    #[test]
    fn a_lock_writes_nothing_for_a_new_password_emptied_again() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");

        session
            .draft(basic.id, fields::PASSWORD, replacement(""), 1)
            .expect("the draft is heard");
        assert!(session.lock(Reason::ScreenLocked));
        assert!(!session.typed());

        let snapshot =
            vault_core::storage::snapshot::slot(&database, 1).expect("a slot has a name");
        assert!(!snapshot.exists(), "a lock with nothing to write wrote");
        assert_eq!(
            value_of(&reopened(&database), basic.id, fields::PASSWORD).as_deref(),
            Some("correct horse battery staple")
        );
    }

    /// Everything the reader was in the middle of, across many entries and
    /// many fields of one entry, is written by one lock.
    #[test]
    fn many_drafts_are_all_written() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");
        let root = session.tree().expect("the tree comes back").id;
        let made: Vec<EntryId> = (0..50)
            .map(|_| {
                session
                    .with_mut(|vault| vault.create_entry(root, Kind::Login))
                    .expect("the vault is open")
                    .expect("an entry is made")
            })
            .collect();
        session
            .with_mut(|vault| {
                (0..200).try_for_each(|n| {
                    vault.set_field(
                        basic.id,
                        &format!("field {n}"),
                        vault_core::NewValue::Open(String::new()),
                    )
                })
            })
            .expect("the vault is open")
            .expect("the fields are made");

        let mut sequence = 0;
        for (n, id) in made.iter().enumerate() {
            sequence += 1;
            session
                .draft(
                    *id,
                    fields::TITLE,
                    words(&format!("entry {n}"), false),
                    sequence,
                )
                .expect("the draft is heard");
        }
        let megabyte = "a line of a long note\n".repeat(50_000);
        sequence += 1;
        session
            .draft(made[0], fields::NOTES, words(&megabyte, false), sequence)
            .expect("the draft is heard");
        for n in 0..200 {
            sequence += 1;
            session
                .draft(
                    basic.id,
                    &format!("field {n}"),
                    words(&format!("value {n}"), false),
                    sequence,
                )
                .expect("the draft is heard");
        }

        assert!(session.lock(Reason::Idle));
        assert!(session.typed());
        let file = reopened(&database);
        for (n, id) in made.iter().enumerate() {
            assert_eq!(
                value_of(&file, *id, fields::TITLE),
                Some(format!("entry {n}"))
            );
        }
        for n in 0..200 {
            assert_eq!(
                value_of(&file, basic.id, &format!("field {n}")),
                Some(format!("value {n}"))
            );
        }
        assert_eq!(value_of(&file, made[0], fields::NOTES), Some(megabyte));
    }

    /// A draft of a vault that will not take a change - a snapshot opened from
    /// the unlock screen - is let go, and the lock still locks.
    #[test]
    fn a_read_only_vault_writes_no_draft_and_still_locks() {
        let (_directory, database, session) = holding(RICH);
        session.with_mut(Vault::save).expect("open").expect("saved");
        session.lock(Reason::ByHand);

        let snapshot =
            vault_core::storage::snapshot::slot(&database, 1).expect("a slot has a name");
        let before = std::fs::read(&snapshot).expect("the snapshot is there");
        session.choose_sibling(snapshot.clone());
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the snapshot opens");
        assert!(
            session
                .with(Vault::is_read_only)
                .expect("the vault is open")
        );

        let basic = entry_titled(&session, "basic");
        session
            .draft(
                basic.id,
                fields::URL,
                words("https://nowhere.example", false),
                1,
            )
            .expect("the draft is heard");

        assert!(session.lock(Reason::Idle));
        assert!(!session.typed());
        assert!(!session.lost(), "nothing was there to lose");
        assert_eq!(
            std::fs::read(&snapshot).expect("the snapshot is there"),
            before,
            "a snapshot was written"
        );
    }

    /// Somebody else wrote the file, so the save on the way out is refused. The
    /// typing goes where the rest of the work goes, which is the copy beside
    /// the vault, and the unlock screen's line about it is the copy's rather
    /// than the one that says it was saved.
    #[test]
    fn typing_the_file_will_not_take_lands_in_the_copy_beside_it() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");
        session
            .draft(
                basic.id,
                fields::URL,
                words("https://rescued.example", false),
                1,
            )
            .expect("the draft is heard");

        {
            let mut theirs = Vault::open(
                &database,
                MasterKey::from_password(password(SECRET)),
                LockPolicy::TakeOver,
            )
            .expect("the other client opens it");
            theirs
                .set_field(
                    basic.id,
                    fields::USERNAME,
                    vault_core::NewValue::Open("theirs".to_owned()),
                )
                .expect("their change is applied");
            theirs.save().expect("their save goes through");
        }

        assert!(session.lock(Reason::Sleeping));
        assert!(!session.typed(), "the typing was said to be in the vault");
        assert!(!session.lost());

        let kept = vault_core::storage::unsaved::beside(&database).expect("a sibling path");
        assert_eq!(
            value_of(&reopened(&kept), basic.id, fields::URL).as_deref(),
            Some("https://rescued.example")
        );
    }

    /// Nothing of the typing outlives the lock: a draft that arrives after it
    /// finds no vault, and the next session has nothing to write - so its lock
    /// saves nothing and says nothing, and the line about the last one is gone
    /// once the vault is open again.
    #[test]
    fn nothing_typed_outlives_the_lock() {
        let (_directory, database, session) = holding(RICH);
        let basic = entry_titled(&session, "basic");
        session
            .draft(
                basic.id,
                fields::URL,
                words("https://once.example", false),
                1,
            )
            .expect("the draft is heard");
        assert!(session.lock(Reason::Idle));
        assert!(session.typed());

        let late = session
            .draft(
                basic.id,
                fields::URL,
                words("https://twice.example", false),
                2,
            )
            .expect_err("a draft with no vault is refused");
        assert_eq!(code_of(&late), "noVault");

        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the database opens again");
        assert!(!session.typed(), "an unlock kept the last lock's line");

        assert!(session.lock(Reason::Idle));
        assert!(
            !session.typed(),
            "the last session's typing was written again"
        );
        let second = vault_core::storage::snapshot::slot(&database, 2).expect("a slot has a name");
        assert!(
            !second.exists(),
            "a lock with nothing typed saved the vault again"
        );
        assert_eq!(
            value_of(&reopened(&database), basic.id, fields::URL).as_deref(),
            Some("https://once.example")
        );
    }

    /// A database whose owner chose a key file in KeePassXC. `vault-core` has
    /// read one since slice 1; until the window could ask for it, every one of
    /// these was a vault Coffer answered with "wrong password" and no way on.
    #[test]
    fn a_key_file_database_opens_once_the_key_file_has_been_chosen() {
        let (directory, database) = scratch("keyfile-kdbx41.kdbx");
        let session = Session::new(Some(database), None);

        assert!(
            session
                .unlock(password(b"coffer-keyfile"), LockPolicy::Respect)
                .is_err(),
            "the password alone opened a database that wants a key file"
        );

        session.use_key_file(Some(fixture("keyfile.key")));
        session
            .unlock(password(b"coffer-keyfile"), LockPolicy::Respect)
            .expect("both halves open it");
        assert!(session.is_unlocked());
        drop(directory);
    }

    /// A key file belongs to one vault. Following the reader to the next one
    /// would turn a right password into a wrong one with nothing to explain it.
    #[test]
    fn pointing_the_session_at_another_database_forgets_the_key_file() {
        let (_directory, database) = scratch("keyfile-kdbx41.kdbx");
        let session = Session::new(Some(database), None);
        session.use_key_file(Some(fixture("keyfile.key")));

        let (_elsewhere, other) = scratch(RICH);
        session.choose(other);

        assert_eq!(session.key_file(), None);
    }

    /// A snapshot of a vault that wants a key file wants the same one. Choosing
    /// any other file forgets it, which is right, and a snapshot forgetting it
    /// left the offer the unlock screen makes for a database that will not open
    /// leading somewhere that could never be opened either.
    #[test]
    fn opening_a_snapshot_keeps_the_key_file_the_vault_needs() {
        let (_directory, database) = scratch("keyfile-kdbx41.kdbx");
        let session = Session::new(Some(database.clone()), None);
        session.use_key_file(Some(fixture("keyfile.key")));

        let mut snapshot = database.into_os_string();
        snapshot.push(".1.bak");
        session.choose_sibling(PathBuf::from(snapshot));

        assert_eq!(session.key_file(), Some(fixture("keyfile.key")));
    }

    /// A lock file from a Mac that lost power cannot be told from one held by a
    /// Coffer running right now: the process id belongs to a machine that has
    /// rebooted, or the host name has changed since. Respecting it is right, and
    /// so is letting a reader who has been shown who holds it say to open
    /// anyway - without that, the vault could never be opened again.
    #[test]
    fn a_lock_nobody_can_prove_stale_is_respected_and_can_still_be_taken_over() {
        let (_directory, database) = scratch(RICH);

        // A lock naming another machine, which is never stale as far as this one
        // is concerned.
        let mut lock = database.clone().into_os_string();
        lock.push(".lock");
        std::fs::write(
            &lock,
            "[Lock]\nTime=2026-08-30T10:00:00Z\nUserName=someone\nMachine=another-mac\nPID=1\nToken=1\n",
        )
        .expect("the lock file is written");

        let session = Session::new(Some(database), None);
        let refused = session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect_err("somebody else's lock is respected");
        // `Failure` derives `Debug` because every part of it is written to be
        // shown, which is what makes this readable from here.
        let said = format!("{refused:?}");
        assert!(said.contains("HeldByAnother"), "{said}");
        assert!(
            said.contains("another-mac") && said.contains("someone"),
            "the reader is not told who holds it: {said}"
        );

        session
            .unlock(password(SECRET), LockPolicy::TakeOver)
            .expect("a reader who said to open anyway gets in");
        assert!(session.is_unlocked());
    }

    /// A vault and the copy a lock left beside it, under the same credentials,
    /// and the copy's path.
    fn with_a_copy(name: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let (directory, database) = scratch(name);
        let database = database.canonicalize().expect("the vault is there");
        let copy = unsaved::beside(&database).expect("a sibling path");
        std::fs::copy(&database, &copy).expect("the copy is written");
        (directory, database, copy)
    }

    /// What an entry's notes say, read the way a reveal reads them: the
    /// fixture protects its notes, and what was typed into them kept that.
    fn notes(vault: &Vault, id: EntryId) -> Option<String> {
        vault
            .reveal(id, fields::NOTES)?
            .expose_str()
            .map(str::to_owned)
    }

    /// "Make this my vault" from inside a copy of a vault that wants a key
    /// file. The key file chosen for the vault opened the copy and still opens
    /// the vault; the session is left open on the vault; and the vault, never
    /// the copy, is what the next launch offers.
    #[test]
    fn a_copy_made_the_vault_leaves_the_session_open_on_the_vault() {
        let (_directory, database, copy) = with_a_copy("keyfile-kdbx41.kdbx");
        let config = tempfile::tempdir().expect("a scratch directory");
        let session = Session::new(Some(database.clone()), Some(config.path().to_path_buf()));
        session.use_key_file(Some(fixture("keyfile.key")));

        session.choose_sibling(copy.clone());
        assert_eq!(session.key_file(), Some(fixture("keyfile.key")));
        session
            .unlock(password(b"coffer-keyfile"), LockPolicy::Respect)
            .expect("the vault's key file opens the copy");
        assert_eq!(
            recent::remembered(config.path()),
            None,
            "the copy was written down as the vault"
        );

        let id = session.tree().expect("the tree comes back").entries[0].id;
        session
            .with_mut(|vault| {
                vault.set_field(
                    id,
                    fields::NOTES,
                    vault_core::NewValue::Open("made in the copy".to_owned()),
                )?;
                vault.save()
            })
            .expect("the session is open")
            .expect("the copy takes the change");

        assert!(
            matches!(session.telling(), Some((vault, _)) if vault == database),
            "the banner is not told about the vault the copy came from"
        );
        assert_eq!(
            session.promote().expect("the copy becomes the vault"),
            database
        );
        assert!(session.is_unlocked());
        assert_eq!(session.database(), Some(database.clone()));
        assert_eq!(session.key_file(), Some(fixture("keyfile.key")));
        assert_eq!(recent::remembered(config.path()), Some(database.clone()));
        assert!(!copy.exists(), "the copy is still beside the vault");

        // The next unlock is of the vault, with the same two halves.
        assert!(session.lock(Reason::ByHand));
        session
            .unlock(password(b"coffer-keyfile"), LockPolicy::Respect)
            .expect("the vault opens");
        assert_eq!(
            session
                .with(|vault| notes(vault, id))
                .expect("the session is open"),
            Some("made in the copy".to_owned())
        );
    }

    /// "Back to my vault" from inside the copy. The copy stays open until the
    /// lock that follows, and that lock writes what the copy held into the
    /// copy - never into the vault, whose file is left exactly as it was - and
    /// the next unlock is of the vault.
    #[test]
    fn going_back_from_an_open_copy_writes_it_into_the_copy_and_asks_for_the_vault() {
        let (_directory, database, copy) = with_a_copy(RICH);
        let vault_bytes = std::fs::read(&database).expect("the vault reads");
        let session = Session::new(Some(copy.clone()), None);
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the copy opens");

        let id = entry_titled(&session, "basic").id;
        session
            .with_mut(|vault| {
                vault.set_field(
                    id,
                    fields::NOTES,
                    vault_core::NewValue::Open("typed in the copy".to_owned()),
                )
            })
            .expect("the session is open")
            .expect("the note is written");

        session.back_to_vault().expect("the copy has a vault");
        assert!(
            session.is_unlocked(),
            "the copy was dropped without its lock"
        );
        assert_eq!(
            session.database(),
            Some(copy.clone()),
            "the vault was chosen before the copy's lock said how it went"
        );
        assert!(session.lock(Reason::ByHand));

        assert_eq!(session.database(), Some(database.clone()));
        assert!(!session.typed() && !session.lost());
        assert_eq!(
            std::fs::read(&database).expect("the vault reads"),
            vault_bytes
        );
        let reread = Vault::open(
            &copy,
            MasterKey::from_password(password(SECRET)),
            LockPolicy::Respect,
        )
        .expect("the copy opens");
        assert_eq!(notes(&reread, id).as_deref(), Some("typed in the copy"));
        drop(reread);

        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the vault opens");
        assert_eq!(session.database(), Some(database));
    }

    /// From the unlock screen of a copy nothing is open, so going back is only
    /// a choice - with the key file the vault needs kept, since the copy was
    /// opened with it.
    #[test]
    fn going_back_from_a_chosen_copy_keeps_the_key_file() {
        let (_directory, database, copy) = with_a_copy("keyfile-kdbx41.kdbx");
        let session = Session::new(Some(copy), None);
        session.use_key_file(Some(fixture("keyfile.key")));

        session.back_to_vault().expect("the copy has a vault");
        assert!(!session.is_unlocked());
        assert_eq!(session.database(), Some(database));
        assert_eq!(session.key_file(), Some(fixture("keyfile.key")));
    }

    /// A vault that is not a copy has no vault to go back to and none to
    /// become: both are refused, and nothing the session holds moves.
    #[test]
    fn a_vault_that_is_not_a_copy_has_no_vault_to_go_back_to_or_become() {
        let (_scratch, session) = unlocked(RICH);
        let chosen = session.database();

        assert_eq!(
            code_of(&session.back_to_vault().expect_err("refused")),
            "refused"
        );
        assert_eq!(code_of(&session.promote().expect_err("refused")), "refused");
        assert_eq!(session.database(), chosen);
        assert!(session.is_unlocked());

        session.lock(Reason::ByHand);
        assert_eq!(
            code_of(&session.promote().expect_err("nothing is open")),
            "noVault"
        );
    }

    /// The copy's own file went while it was open - trashed in the Finder -
    /// and the reader pressed "Back to my vault". The lock that closes the copy
    /// can only keep its work in a copy of the copy, and that is what the next
    /// screen has to offer: so the session stays on the copy, whose unlock
    /// screen finds it. Pointed at the vault, it would look beside the vault
    /// and find nothing, and the work would sit in a file nothing points at.
    #[test]
    fn going_back_from_a_copy_whose_lock_kept_its_work_elsewhere_stays_on_the_copy() {
        let (_directory, database, copy) = with_a_copy(RICH);
        let vault_bytes = std::fs::read(&database).expect("the vault reads");
        let session = Session::new(Some(copy.clone()), None);
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the copy opens");

        let id = entry_titled(&session, "basic").id;
        session
            .draft(id, fields::NOTES, words("typed in the copy", false), 1)
            .expect("the draft is heard");
        std::fs::remove_file(&copy).expect("the copy goes to the Trash");

        session.back_to_vault().expect("the copy has a vault");
        assert!(session.lock(Reason::ByHand));

        assert_eq!(session.database(), Some(copy.clone()));
        let kept = unsaved::found(&copy)
            .expect("the folder reads")
            .expect("the copy's work is offered beside the copy");
        assert!(!session.lost());
        assert!(
            !session.typed(),
            "the typing went into a copy of the copy, not where the reader believes"
        );
        assert_eq!(
            std::fs::read(&database).expect("the vault reads"),
            vault_bytes,
            "the vault took what was the copy's"
        );
        assert!(
            unsaved::found(&database)
                .expect("the folder reads")
                .is_none(),
            "a copy beside the vault appeared"
        );

        let reread = Vault::open(
            &kept.path,
            MasterKey::from_password(password(SECRET)),
            LockPolicy::Respect,
        )
        .expect("the copy of the copy opens");
        assert_eq!(notes(&reread, id).as_deref(), Some("typed in the copy"));
    }

    /// Typing in the copy that the lock saved into the copy. The session goes
    /// back to the vault, and the vault's screen does not say its own file
    /// took the typing: nothing of it is in the vault.
    #[test]
    fn typing_saved_into_the_copy_is_not_said_on_the_vaults_screen() {
        let (_directory, database, copy) = with_a_copy(RICH);
        let session = Session::new(Some(copy.clone()), None);
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the copy opens");
        let id = entry_titled(&session, "basic").id;
        session
            .draft(id, fields::NOTES, words("a note for the copy", false), 1)
            .expect("the draft is heard");

        session.back_to_vault().expect("the copy has a vault");
        assert!(session.lock(Reason::ByHand));

        assert_eq!(session.database(), Some(database.clone()));
        assert!(!session.typed());
        let reread = Vault::open(
            &copy,
            MasterKey::from_password(password(SECRET)),
            LockPolicy::Respect,
        )
        .expect("the copy opens");
        assert_eq!(notes(&reread, id).as_deref(), Some("a note for the copy"));
    }

    /// Going back is the lock that follows it, and nothing else: an unlock in
    /// between - the reader typed the copy's password again - or another file
    /// chosen leaves a later lock where it is.
    #[test]
    fn going_back_ends_with_the_lock_it_was_for() {
        let (_directory, database, copy) = with_a_copy(RICH);
        let session = Session::new(Some(copy.clone()), None);
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the copy opens");
        session.back_to_vault().expect("the copy has a vault");
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the copy opens again");
        assert!(session.lock(Reason::ByHand));
        assert_eq!(session.database(), Some(copy.clone()));

        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the copy opens");
        session.back_to_vault().expect("the copy has a vault");
        assert!(session.lock(Reason::ByHand));
        assert_eq!(session.database(), Some(database));
    }

    /// "Make this my vault" goes over the file the banner described and over
    /// no other. Another client's save after the window was told is refused
    /// with nothing written, the copy still open; told again, the same press
    /// goes through, and what that client wrote is the newest snapshot.
    #[test]
    fn a_copy_is_made_the_vault_only_over_the_file_the_window_was_told_about() {
        let (_directory, database, copy) = with_a_copy(RICH);
        let session = Session::new(Some(copy.clone()), None);
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the copy opens");

        assert_eq!(
            code_of(&session.promote().expect_err("nothing was said")),
            "externalChange"
        );

        session.telling().expect("the copy has a vault");
        let id = entry_titled(&session, "basic").id;
        let mut theirs = Vault::open(
            &database,
            MasterKey::from_password(password(SECRET)),
            LockPolicy::Respect,
        )
        .expect("the other client opens the vault");
        theirs
            .set_field(
                id,
                fields::URL,
                vault_core::NewValue::Open("https://theirs.example".to_owned()),
            )
            .expect("their change is applied");
        theirs.save().expect("their save goes through");
        drop(theirs);
        let written = std::fs::read(&database).expect("the vault reads");

        assert_eq!(
            code_of(&session.promote().expect_err("the vault changed")),
            "externalChange"
        );
        assert_eq!(session.database(), Some(copy.clone()));
        assert!(copy.exists());
        assert_eq!(std::fs::read(&database).expect("the vault reads"), written);

        session.telling().expect("the copy has a vault");
        session
            .promote()
            .expect("told again, the copy becomes the vault");
        let first = vault_core::storage::snapshot::slot(&database, 1).expect("a slot has a name");
        assert_eq!(std::fs::read(first).expect("the snapshot reads"), written);
    }
}
