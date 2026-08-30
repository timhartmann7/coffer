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
use vault_core::{LockPolicy, MasterKey, Recipe, SecretValue, Vault};
use zeroize::Zeroizing;

use crate::autolock::Reason;
use crate::error::Failure;

pub struct Session {
    held: Mutex<Held>,
}

struct Held {
    /// What the next unlock will open. Chosen before a password is asked for,
    /// which is why it outlives the vault.
    database: Option<PathBuf>,
    /// Why the vault that was open is not open any more, when it is worth
    /// saying. Cleared by the next unlock.
    locked_by: Option<Reason>,
    /// Where a vault being made will go, if one is. Kept apart from
    /// `database`, which is the file the unlock screen offers to open: there is
    /// nothing at this path yet, and pointing the unlock screen at it would be
    /// offering to open a file that does not exist.
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
    pub fn new(database: Option<PathBuf>) -> Session {
        Session {
            held: Mutex::new(Held {
                database,
                locked_by: None,
                making: None,
                measured: None,
                vault: None,
                generation: 0,
            }),
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
        held.generation += 1;
    }

    pub fn is_unlocked(&self) -> bool {
        self.held().vault.is_some()
    }

    /// Opens the chosen database.
    ///
    /// The password is consumed: it goes into the vault, which holds it for as
    /// long as it is open because KeePass derives a fresh key on every save,
    /// and wipes it on the way out.
    pub fn unlock(&self, password: Zeroizing<Vec<u8>>) -> Result<(), Failure> {
        let (database, generation) = {
            let mut held = self.held();
            // Anything already open is dropped before the new one is opened, so
            // that reopening the same file does not find Coffer's own lock
            // beside it and refuse.
            held.vault = None;
            (
                held.database.clone().ok_or_else(Failure::no_vault)?,
                held.generation,
            )
        };

        // Key derivation is a second of work, and it happens with nothing held.
        // A window that asks the session anything meanwhile is answered rather
        // than left waiting on a lock this thread is holding.
        let vault = Vault::open(
            &database,
            MasterKey::from_password(password),
            LockPolicy::Respect,
        )?;

        let mut held = self.held();
        if held.generation != generation {
            // Somebody chose another database, or locked, while this one was
            // opening. Dropping the vault here takes the lock file off a
            // database nobody is asking about any more.
            return Err(Failure::stale());
        }

        // The database Coffer opened is the one it followed the links to.
        held.database = Some(vault.path().to_path_buf());
        held.vault = Some(vault);
        held.locked_by = None;
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
        let had = held.vault.take().is_some();
        if had {
            held.locked_by = Some(reason);
        }
        held.generation += 1;
        had
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
            let target = held
                .making
                .clone()
                .ok_or_else(|| Failure::refused("nowhere has been chosen for the new vault"))?;
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

        let mut held = self.held();
        if held.generation != generation {
            return Err(Failure::stale());
        }

        held.database = Some(vault.path().to_path_buf());
        held.vault = Some(vault);
        held.locked_by = None;
        held.making = None;
        Ok(())
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
        let session = Session::new(Some(database));
        session
            .unlock(password(SECRET))
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
        let session = Session::new(Some(database));

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
        let session = Session::new(Some(database.clone()));

        for _ in 0..3 {
            session
                .unlock(password(SECRET))
                .expect("the database opens");
            session.lock(Reason::ByHand);
        }

        assert!(
            vault_core::storage::lock::inspect(&database)
                .expect("the lock file reads")
                .is_none()
        );
    }

    /// Unlocking twice without locking in between must not trip over Coffer's
    /// own lock file.
    #[test]
    fn unlocking_an_open_database_again_replaces_it() {
        let (_scratch, session) = unlocked(RICH);
        session.unlock(password(SECRET)).expect("it opens again");
        assert!(session.is_unlocked());
    }

    #[test]
    fn a_wrong_password_leaves_nothing_open() {
        let (_scratch, database) = scratch(RICH);
        let session = Session::new(Some(database));

        for wrong in [b"".as_slice(), b"coffer-tes".as_slice(), &[0xff, 0xfe]] {
            assert!(session.unlock(password(wrong)).is_err());
            assert!(!session.is_unlocked());
        }

        session
            .unlock(password(SECRET))
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
        let session = Session::new(None);
        assert!(session.unlock(password(SECRET)).is_err());
    }

    /// Key derivation happens with nothing held, so the session can move while
    /// it runs. Whatever the timing, the session must end up where it was last
    /// pointed rather than where the unlock started.
    #[test]
    fn an_unlock_that_finishes_late_does_not_win() {
        use std::sync::Arc;

        let (_first, one) = scratch(RICH);
        let (_second, two) = scratch(RICH);

        let session = Arc::new(Session::new(Some(one)));
        let unlocking = {
            let session = Arc::clone(&session);
            std::thread::spawn(move || session.unlock(password(SECRET)))
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
        let holding = Session::new(Some(database.clone()));
        holding
            .unlock(password(SECRET))
            .expect("the first one opens");

        let second = Session::new(Some(database));
        let failure = second
            .unlock(password(SECRET))
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
        let session = Session::new(Some(database));

        // Nothing is open, so nothing changes.
        assert!(
            session
                .with_mut(|vault| vault.create_group(vault.tree().id, "Later"))
                .is_err()
        );

        session
            .unlock(password(SECRET))
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
        let session = Arc::new(Session::new(None));
        session.making(target.clone());
        session.measured(Work::at(40));
        session.create(password(SECRET)).expect("the vault is made");
        assert!(session.lock(Reason::ByHand));

        let unlocking = {
            let session = Arc::clone(&session);
            std::thread::spawn(move || session.unlock(password(SECRET)))
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

        let session = Session::new(None);
        session.making(target.clone());
        session.measured(Work::at(1));
        session.create(password(SECRET)).expect("the vault is made");

        assert!(target.is_file(), "the vault is not where it was asked for");
        assert!(session.is_unlocked());

        // A folder whose parent is a file. `create_dir_all` cannot make it, and
        // neither could anything else.
        let blocked = directory.path().join("a-file");
        std::fs::write(&blocked, b"not a folder").expect("the file is written");

        let session = Session::new(None);
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

        let session = Session::new(None);
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
            let session = Session::new(None);

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

        let session = Session::new(None);
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

        let session = Arc::new(Session::new(None));
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
    }
}
