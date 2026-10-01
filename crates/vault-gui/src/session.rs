//! The one database this process has open, and everything the screens may ask
//! of it.
//!
//! Every decision about the database itself is made in `vault-core`. What is
//! here is the fact that a window has exactly one vault at a time, and that
//! locking means dropping it: the decrypted tree, the master password and the
//! lock file all go with it.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use vault_core::kdf::Work;
use vault_core::model::{Entry, EntryId, Project};
use vault_core::{LockPolicy, MasterKey, Recipe, Rescue, SecretValue, Vault};
use zeroize::Zeroizing;

use crate::autolock::Reason;
use crate::error::Failure;
use crate::recent;

pub struct Session {
    held: Mutex<Held>,
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
    /// What a one-second unlock costs on this machine.
    ///
    /// A property of the Mac and not of the file, which is why it is here and
    /// not beside the path. The screen measures as it opens and asks where to
    /// put the vault afterwards, so a measurement kept beside the place would
    /// be thrown away by the question that comes next - and the vault could
    /// never be made at all.
    measured: Option<Work>,
    /// The open database. Dropping it wipes the decrypted tree and removes the
    /// lock file beside the database.
    vault: Option<Vault>,
    /// Bumped every time the session is pointed somewhere else or emptied. Key
    /// derivation takes a second and does not hold the lock, so an unlock that
    /// started before such a change must not finish over it.
    generation: u64,
}

impl Session {
    /// A session pointed at `database`, writing down in `remembering` every
    /// vault that opens.
    pub fn new(database: Option<PathBuf>, remembering: Option<PathBuf>) -> Session {
        Session {
            held: Mutex::new(Held {
                database,
                locked_by: None,
                lost: false,
                key_file: None,
                making: None,
                measured: None,
                vault: None,
                generation: 0,
            }),
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
        held.vault = None;
        held.database = Some(database);
        // A key file belongs to the vault it opens. Carrying one over to a
        // different file would turn a right password into a wrong one, with
        // nothing on the screen to explain it.
        held.key_file = None;
        // A flag about the vault that was open says nothing about the one being
        // chosen, for the same reason the key file does not carry over.
        held.lost = false;
        held.generation += 1;
    }

    pub fn is_unlocked(&self) -> bool {
        self.held().vault.is_some()
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
            held.vault = None;
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
            held.vault = Some(vault);
            held.key_file = key_file;
            held.making = None;
            held.locked_by = None;
            held.lost = false;
            opened
        };

        // Outside the lock: nothing about it needs the vault, and a write that
        // flushes the disk has no business holding up whatever else the window
        // is asking. A vault Coffer fails to write down is one the reader picks
        // again next launch, which is not a reason to refuse to open it now.
        if let Some(directory) = &self.remembering {
            let _ = recent::remember(directory, &opened);
        }
        Ok(())
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
        // is a session's work ended by a timer nobody was watching.
        let rescue = held.vault.as_mut().map(Vault::rescue);
        held.vault = None;
        held.generation += 1;

        let Some(rescue) = rescue else { return false };
        held.locked_by = Some(reason);
        held.lost = rescue == Rescue::Lost;
        true
    }

    /// Whether the last lock in this run had to give something up.
    pub fn lost(&self) -> bool {
        self.held().lost
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
            held.vault = None;
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

    /// Borrows the open vault. It never leaves the lock, so nothing can hold a
    /// vault past a lock that was supposed to wipe it.
    pub fn with<T>(&self, read: impl FnOnce(&Vault) -> T) -> Result<T, Failure> {
        let held = self.held();
        let vault = held.vault.as_ref().ok_or_else(Failure::no_vault)?;
        Ok(read(vault))
    }

    /// Borrows the open vault to change it.
    ///
    /// The lock is held for the whole change, and a save holds it for the
    /// second the key derivation takes. Nothing else may touch the vault
    /// meanwhile, which is the point: a window drawing a tree from a database
    /// that is half way through a change would be drawing something that was
    /// never true.
    pub fn with_mut<T>(&self, change: impl FnOnce(&mut Vault) -> T) -> Result<T, Failure> {
        let mut held = self.held();
        let vault = held.vault.as_mut().ok_or_else(Failure::no_vault)?;
        Ok(change(vault))
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use vault_core::model::fields;

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
        assert_eq!(
            rescued
                .entry(id)
                .expect("the entry is there")
                .field(fields::NOTES)
                .and_then(|held| held.value.open()),
            Some("never saved")
        );
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
            .filter(|body| body.contains("held.vault = Some("))
            .collect();
        assert_eq!(putting.len(), 1, "a vault is put into the session twice");
        assert!(
            putting
                .iter()
                .all(|body| body.contains("fn land(") && body.contains("recent::remember(")),
            "the vault is put into the session somewhere that does not write it down"
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
}
