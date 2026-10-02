//! A copy of the vault on another disk, and the date a reminder to make one
//! counts from.
//!
//! The copy is the conflict dialog's write, aimed further away: what is asked
//! of it here is that it loses nothing, takes nothing the reader keeps beside
//! the vault, and leaves the vault and its backups as they were whatever the
//! target turns out to be.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

use chrono::{NaiveDate, NaiveDateTime, Utc};
use keepass::db::fields;

use vault_core::{EncryptedCopy, NewValue, Vault, VaultError};

use crate::normalise::{canonical, differences};
use crate::support::{
    self, BUILT_PASSWORD, Frozen, RICH, SECRET, built, everything, notes, notes_of, open,
    permissions_apply, scratch, snapshots, vault_with_an_entry,
};

/// Midnight on the first of January of `year`, as the file would hold it.
fn new_year(year: i32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(year, 1, 1)
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .expect("a date the calendar holds")
}

/// Everything in a directory that a write staged and left behind.
fn staged(directory: &Path) -> Vec<String> {
    std::fs::read_dir(directory)
        .expect("the directory reads")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".coffer-tmp"))
        .collect()
}

/// A vault Coffer makes is dated by the library as it makes the top group, and
/// that date is in the file: the vault opened again, and saved, says the same.
#[test]
fn a_vault_coffer_made_says_when_it_was_made() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let path = directory.path().join("new.kdbx");
    let before = Utc::now().naive_utc() - chrono::Duration::seconds(1);
    let mut made = Vault::create(
        &path,
        support::password(SECRET),
        &vault_core::Recipe {
            name: "Mine",
            work: vault_core::kdf::Work::at(1),
        },
    )
    .expect("the vault is made");
    let after = Utc::now().naive_utc() + chrono::Duration::seconds(1);

    let when = made.made().expect("a vault Coffer made says when");
    assert!(before <= when && when <= after, "{when} is not now");

    made.save().expect("the vault saves");
    drop(made);
    assert_eq!(open(&path, SECRET).made(), Some(when));
}

/// Another client's vault says when it was made as well, a save does not move
/// it, and neither does a copy: a copy of a vault began when the vault did.
#[test]
fn a_vault_another_client_made_says_when_it_was_made() {
    let (directory, database) = scratch(RICH);
    let mut vault = open(&database, SECRET);
    let when = vault
        .made()
        .expect("the fixture's top group carries a creation time");
    assert!(when < Utc::now().naive_utc());

    vault.save().expect("the vault saves");
    let copy = directory.path().join("elsewhere.kdbx");
    vault
        .encrypt_copy(&copy)
        .and_then(EncryptedCopy::write)
        .expect("the copy is written");
    assert_eq!(vault.made(), Some(when));
    drop(vault);

    assert_eq!(open(&database, SECRET).made(), Some(when));
    assert_eq!(open(&copy, SECRET).made(), Some(when));
}

/// A file whose top group carries no date says nothing, opens, saves, and is
/// not given one: Coffer does not make up when somebody else's vault began.
#[test]
fn a_vault_that_does_not_say_when_it_was_made_says_nothing() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = built(directory.path(), "undated.kdbx", |db| {
        db.root_mut().times.creation = None;
        db.root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "subject"));
    });

    let mut vault = open(&database, BUILT_PASSWORD);
    assert_eq!(vault.made(), None);
    vault.save().expect("the vault saves");
    drop(vault);
    assert_eq!(open(&database, BUILT_PASSWORD).made(), None);
}

/// The format holds dates centuries either side of now, and a date is handed
/// back as the file holds it. What one in 1600 or 3000 means is not decided
/// here.
#[test]
fn a_made_date_from_1600_or_3000_is_handed_back_as_it_is() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    for year in [1600, 3000] {
        let database = built(directory.path(), &format!("{year}.kdbx"), |db| {
            db.root_mut().times.creation = Some(new_year(year));
        });
        assert_eq!(
            open(&database, BUILT_PASSWORD).made(),
            Some(new_year(year)),
            "{year}"
        );
    }
}

/// Never lose a field, on the way to another disk as on the way back to the
/// vault: everything a reader can look at in the vault is in the copy, and
/// KeePassXC reads the copy as it read the file the vault was opened from.
#[test]
fn a_copy_loses_no_field() {
    let (directory, database) = scratch(RICH);
    let elsewhere = tempfile::tempdir().expect("another folder");
    let copy = elsewhere.path().join("vault 2026-10-02.kdbx");
    let original = directory.path().join("original.kdbx");
    std::fs::copy(&database, &original).expect("the fixture copies");

    let mut vault = open(&database, SECRET);
    vault
        .encrypt_copy(&copy)
        .and_then(EncryptedCopy::write)
        .expect("the copy is written");
    let held = everything(&vault);
    drop(vault);

    assert_eq!(everything(&open(&copy, SECRET)), held);

    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let before = canonical(&support::export(&tool, &original, SECRET, None), false);
    let after = canonical(&support::export(&tool, &copy, SECRET, None), false);
    let lost = differences(&before, &after);
    assert!(
        lost.is_empty(),
        "the copy lost or altered a field:\n  {}",
        lost.join("\n  ")
    );
}

/// A save panel asks before it replaces a file, and a reader who points it at
/// a link to their vault and says yes must not lose the vault. A symbolic link
/// leads to the vault and is the vault; a second name for its file holds the
/// vault's bytes. Either is refused before anything is written, and the vault,
/// the link and the backups are as they were.
#[test]
fn a_copy_aimed_at_a_link_to_the_vault_leaves_the_vault_whole() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let elsewhere = tempfile::tempdir().expect("another folder");
    let database = vault_with_an_entry(directory.path(), "vault.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    vault.save().expect("the vault saves");
    let id = vault.tree().entries[0].id;
    vault
        .set_field(id, fields::NOTES, NewValue::Open("not saved yet".into()))
        .expect("the note is written");

    let symbolic = elsewhere.path().join("pointing.kdbx");
    std::os::unix::fs::symlink(&database, &symbolic).expect("the link is made");
    let second = elsewhere.path().join("second name.kdbx");
    std::fs::hard_link(&database, &second).expect("the second name is made");

    let bytes = std::fs::read(&database).expect("the vault reads");
    let inode = std::fs::metadata(&database)
        .expect("the vault is there")
        .ino();
    let backups = snapshots(&database);

    // The link is followed to the vault itself; the second name is a file
    // somebody keeps, which a copy never takes.
    for (what, target, wanted) in [
        ("a symbolic link", &symbolic, VaultError::CopyOntoItself),
        ("a second name", &second, VaultError::DatabaseExists),
    ] {
        let answer = vault.encrypt_copy(target).and_then(EncryptedCopy::write);
        assert!(
            matches!(&answer, Err(error) if std::mem::discriminant(error) == std::mem::discriminant(&wanted)),
            "{what}: {answer:?}"
        );
        assert_eq!(
            std::fs::read(&database).expect("the vault reads"),
            bytes,
            "{what}"
        );
        assert_eq!(
            std::fs::metadata(&database)
                .expect("the vault is there")
                .ino(),
            inode,
            "{what}"
        );
    }

    assert!(
        symbolic
            .symlink_metadata()
            .is_ok_and(|about| about.file_type().is_symlink()),
        "the link was written over"
    );
    assert_eq!(
        std::fs::metadata(&second).expect("the name is there").ino(),
        inode,
        "the second name no longer names the vault's file"
    );
    assert_eq!(snapshots(&database), backups);
    assert_eq!(staged(elsewhere.path()), Vec::<String>::new());

    vault.save().expect("the vault still saves");
    drop(vault);
    assert_eq!(notes_of(&database).as_deref(), Some("not saved yet"));
}

/// A folder that will not take a file - a stick mounted read only, a share
/// the reader may only read - refuses the copy. Nothing is left in it, the
/// vault's file is as it was, and the change the copy was to carry is still
/// the vault's, for the next save.
#[test]
fn a_copy_into_a_folder_that_will_not_take_it_leaves_nothing_there() {
    if !permissions_apply() {
        return;
    }

    let directory = tempfile::tempdir().expect("a scratch directory");
    let elsewhere = tempfile::tempdir().expect("another folder");
    let database = vault_with_an_entry(directory.path(), "vault.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault
        .set_field(id, fields::NOTES, NewValue::Open("in the window".into()))
        .expect("the note is written");
    let bytes = std::fs::read(&database).expect("the vault reads");

    let copy = elsewhere.path().join("copy.kdbx");
    {
        let _frozen = Frozen::over(elsewhere.path());
        assert!(
            vault
                .encrypt_copy(&copy)
                .and_then(EncryptedCopy::write)
                .is_err(),
            "a frozen folder took it"
        );
    }

    assert!(!copy.exists());
    assert_eq!(staged(elsewhere.path()), Vec::<String>::new());
    assert_eq!(std::fs::read(&database).expect("the vault reads"), bytes);
    assert_eq!(notes(&vault).as_deref(), Some("in the window"));

    vault.save().expect("the vault saves");
    drop(vault);
    assert_eq!(notes_of(&database).as_deref(), Some("in the window"));
}

/// A folder that is not there - a stick pulled out while the panel was up -
/// refuses the copy, and is not made on the way.
#[test]
fn a_copy_into_a_folder_that_is_not_there_is_refused() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault_with_an_entry(directory.path(), "vault.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);

    let gone = directory.path().join("Stick");
    let answer = vault
        .encrypt_copy(&gone.join("copy.kdbx"))
        .and_then(EncryptedCopy::write);

    assert!(matches!(answer, Err(VaultError::Io(_))), "{answer:?}");
    assert!(!gone.exists(), "the folder was made on the way");
}

/// Owner-only from birth, whatever the folder and the process would let
/// anybody else have: a copy on a stick is the vault, and it is the first file
/// somebody else may pick up.
#[test]
fn a_copy_is_written_owner_only() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let elsewhere = tempfile::tempdir().expect("another folder");
    std::fs::set_permissions(elsewhere.path(), std::fs::Permissions::from_mode(0o777))
        .expect("the folder is opened to everybody");
    let database = vault_with_an_entry(directory.path(), "vault.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);

    let copy = elsewhere.path().join("copy.kdbx");
    vault
        .encrypt_copy(&copy)
        .and_then(EncryptedCopy::write)
        .expect("the copy is written");

    let mode = std::fs::metadata(&copy)
        .expect("the copy is there")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

/// A copy is a copy: three of them, one after another, leave the vault's file,
/// its lock and its backups as they were, and the change none of them saved
/// is still the vault's to save.
#[test]
fn a_copy_leaves_the_vault_and_its_snapshots_as_they_were() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let elsewhere = tempfile::tempdir().expect("another folder");
    let database = vault_with_an_entry(directory.path(), "vault.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    vault.save().expect("the vault saves");
    vault.save().expect("the vault saves again");
    let id = vault.tree().entries[0].id;
    vault
        .set_field(
            id,
            fields::NOTES,
            NewValue::Open("copied, not saved".into()),
        )
        .expect("the note is written");

    let bytes = std::fs::read(&database).expect("the vault reads");
    let lock = database.with_file_name("vault.kdbx.lock");
    let held = std::fs::read(&lock).expect("the lock is there");
    let backups = snapshots(&database);

    for number in 1..=3 {
        let copy = elsewhere.path().join(format!("copy {number}.kdbx"));
        vault
            .encrypt_copy(&copy)
            .and_then(EncryptedCopy::write)
            .expect("the copy is written");
        assert_eq!(notes_of(&copy).as_deref(), Some("copied, not saved"));
    }

    assert_eq!(std::fs::read(&database).expect("the vault reads"), bytes);
    assert_eq!(std::fs::read(&lock).expect("the lock is there"), held);
    assert_eq!(snapshots(&database), backups);

    vault.save().expect("the vault saves");
    drop(vault);
    assert_eq!(notes_of(&database).as_deref(), Some("copied, not saved"));
}

/// The copy is encrypted while the vault is held and written once it need not
/// be, so a lock can land between the two without waiting on a slow stick or a
/// share that went to sleep. The bytes are the vault as it was when they were
/// encrypted, the change it had not saved included, and writing them needs
/// nothing of the vault: not its tree, not its lock, not its file.
#[test]
fn a_copy_encrypted_before_the_vault_closed_is_written_whole_after() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let elsewhere = tempfile::tempdir().expect("another folder");
    let database = vault_with_an_entry(directory.path(), "vault.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault
        .set_field(id, fields::NOTES, NewValue::Open("in the copy".into()))
        .expect("the note is written");

    let copy = elsewhere.path().join("copy.kdbx");
    let encrypted = vault.encrypt_copy(&copy).expect("the copy is encrypted");
    assert!(
        !copy.exists(),
        "the copy was written before it was asked to be"
    );
    drop(vault);
    let lock = database.with_file_name("vault.kdbx.lock");
    assert!(!lock.exists(), "the vault was still held");

    encrypted.write().expect("the copy is written");
    assert_eq!(notes_of(&copy).as_deref(), Some("in the copy"));
    assert_eq!(notes_of(&database), None);
    assert!(!lock.exists(), "writing the copy took the vault's lock");
    assert_eq!(staged(elsewhere.path()), Vec::<String>::new());
}

/// A name that held nothing when the copy was encrypted and holds a file by
/// the time it is written - another copy saved there meanwhile, a file the
/// Finder put there - is refused, and what is there is left as it was. The
/// copy settled the histories on the way, so the revision moved all the same.
#[test]
fn a_name_taken_while_the_copy_was_encrypted_is_refused_and_left_alone() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let elsewhere = tempfile::tempdir().expect("another folder");
    let database = vault_with_an_entry(directory.path(), "vault.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    let before = vault.edits();

    let copy = elsewhere.path().join("copy.kdbx");
    let encrypted = vault.encrypt_copy(&copy).expect("the copy is encrypted");
    assert!(
        vault.edits() > before,
        "settling the histories moved nothing"
    );
    std::fs::write(&copy, b"somebody's file").expect("the name is taken");

    let answer = encrypted.write();
    assert!(
        matches!(answer, Err(VaultError::DatabaseExists)),
        "{answer:?}"
    );
    assert_eq!(
        std::fs::read(&copy).expect("the file is there"),
        b"somebody's file"
    );
    assert_eq!(staged(elsewhere.path()), Vec::<String>::new());
}
