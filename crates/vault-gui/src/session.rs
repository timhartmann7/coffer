//! The one database this process has open, and everything the screens may ask
//! of it.
//!
//! Every decision about the database itself is made in `vault-core`. What is
//! here is the fact that a window has exactly one vault at a time, and that
//! locking means dropping it: the decrypted tree, the master password and the
//! lock file all go with it.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use vault_core::model::{Entry, EntryId, Project};
use vault_core::{LockPolicy, MasterKey, SecretValue, Vault};
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
}
