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

use crate::error::Failure;

pub struct Session {
    held: Mutex<Held>,
}

struct Held {
    /// What the next unlock will open. Chosen before a password is asked for,
    /// which is why it outlives the vault.
    database: Option<PathBuf>,
    /// The open database. Dropping it wipes the decrypted tree and removes the
    /// lock file beside the database.
    vault: Option<Vault>,
}

impl Session {
    pub fn new(database: Option<PathBuf>) -> Session {
        Session {
            held: Mutex::new(Held {
                database,
                vault: None,
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
    pub fn choose(&self, database: PathBuf) {
        let mut held = self.held();
        held.vault = None;
        held.database = Some(database);
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
        let database = {
            let mut held = self.held();
            // Anything already open is dropped before the new one is opened, so
            // that reopening the same file does not find Coffer's own lock
            // beside it and refuse.
            held.vault = None;
            held.database.clone().ok_or_else(Failure::no_vault)?
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
        // The database Coffer opened is the one it followed the links to.
        held.database = Some(vault.path().to_path_buf());
        held.vault = Some(vault);
        Ok(())
    }

    /// Wipes the decrypted database out of memory.
    pub fn lock(&self) {
        self.held().vault = None;
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
    pub fn reveal(&self, id: EntryId, field: &str) -> Result<SecretValue, Failure> {
        self.with(|vault| vault.reveal(id, field))?
            .ok_or_else(Failure::no_such_entry)
    }

    /// Borrows the open vault. It never leaves the lock, so nothing can hold a
    /// vault past a lock that was supposed to wipe it.
    fn with<T>(&self, read: impl FnOnce(&Vault) -> T) -> Result<T, Failure> {
        let held = self.held();
        let vault = held.vault.as_ref().ok_or_else(Failure::no_vault)?;
        Ok(read(vault))
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

        session.lock();

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
            session.lock();
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
        assert_eq!(session.database(), Some(database));
    }

    #[test]
    fn there_is_nothing_to_unlock_until_something_is_chosen() {
        let session = Session::new(None);
        assert!(session.unlock(password(SECRET)).is_err());
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
        assert!(
            session
                .reveal(EntryId::from_uuid(uuid::Uuid::nil()), fields::PASSWORD)
                .is_err()
        );
    }
}
