//! Open vaults for the suites that need one: the session's, the menus' and
//! the copies'.
//!
//! They open the databases `vault-core` generates with `keepassxc-cli`,
//! because the only interesting session is one holding a database somebody
//! else wrote.

use std::path::{Path, PathBuf};

use vault_core::LockPolicy;
use vault_core::model::{Entry, EntryId, Project};
use zeroize::Zeroizing;

use crate::session::Session;

/// The whole feature matrix, in the format Coffer writes.
pub const RICH: &str = "rich-kdbx41.kdbx";
/// The password every generated fixture opens with.
pub const SECRET: &[u8] = b"coffer-test";

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../vault-core/tests/fixtures")
        .join(name)
}

/// A copy, because opening a database writes a lock file beside it.
pub fn scratch(name: &str) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let target = directory.path().join(name);
    std::fs::copy(fixture(name), &target).expect("the fixture copies");
    (directory, target)
}

pub fn password(text: &[u8]) -> Zeroizing<Vec<u8>> {
    Zeroizing::new(text.to_vec())
}

pub fn unlocked(name: &str) -> (tempfile::TempDir, Session) {
    let (directory, database) = scratch(name);
    let session = Session::new(Some(database), None);
    session
        .unlock(password(SECRET), LockPolicy::Respect)
        .expect("the database opens");
    (directory, session)
}

pub fn entry_titled(session: &Session, title: &str) -> Entry {
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
