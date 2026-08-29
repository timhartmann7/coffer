//! Writing a database back: what is checked before anything is disturbed, and
//! what the directory looks like afterwards.

use std::os::unix::fs::PermissionsExt;

use keepass::db::fields;
use vault_core::storage::snapshot;
use vault_core::{NewValue, VaultError};

use crate::support::{self, BUILT_PASSWORD, built, entry_titled, open};

const RICH: &str = "rich-kdbx41.kdbx";
const SECRET: &str = "coffer-test";

/// The notes on the fixture's single entry, as plain text, so that a failing
/// snapshot assertion says which generation it found.
fn note_of(vault: &vault_core::Vault) -> Option<String> {
    match entry_titled(vault, "one")
        .field(fields::NOTES)
        .map(|field| field.value.clone())
    {
        Some(vault_core::model::FieldValue::Open(text)) => Some(text),
        _ => None,
    }
}

fn mode_of(path: &std::path::Path) -> u32 {
    std::fs::metadata(path)
        .expect("the file is there")
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn the_saved_database_is_readable_only_by_its_owner() {
    let (_scratch, database) = support::scratch(RICH);
    // The fixture arrives however git left it; the point is what saving does.
    std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o644))
        .expect("the mode is set");

    let mut vault = open(&database, SECRET);
    vault.save().expect("the database saves");

    assert_eq!(mode_of(&database), 0o600);
}

#[test]
fn a_snapshot_is_taken_before_every_write() {
    let (scratch, database) = support::scratch(RICH);
    let original = std::fs::read(&database).expect("the fixture reads");

    let mut vault = open(&database, SECRET);
    vault.save().expect("the database saves");

    let first = snapshot::slot(&database, 1).expect("the slot has a path");
    assert_eq!(
        std::fs::read(&first).expect("the snapshot exists"),
        original,
        "the snapshot holds what the database held before the save"
    );
    assert_eq!(mode_of(&first), 0o600);
    assert!(!scratch.path().join("rich-kdbx41.kdbx.2.bak").exists());
}

#[test]
fn snapshots_rotate_and_stop_at_ten() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "cheap.kdbx", |db| {
        db.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "one");
        });
    });

    let generations = 13;
    for round in 0..generations {
        let mut vault = open(&database, BUILT_PASSWORD);
        let id = vault.tree().entries[0].id;
        vault
            .set_field(id, fields::NOTES, NewValue::Open(format!("round {round}")))
            .expect("the field is written");
        vault.save().expect("the database saves");
    }

    // Slot 1 holds the state before the last save, and each older slot holds one
    // generation further back. A hard-linked snapshot that the next save
    // rewrote in place would show the newest note in every slot.
    for index in 1..=snapshot::SNAPSHOT_COUNT {
        let slot = snapshot::slot(&database, index).expect("the slot has a path");
        assert!(slot.exists(), "slot {index} is missing");

        let kept = open(&slot, BUILT_PASSWORD);
        assert_eq!(
            note_of(&kept),
            Some(format!("round {}", generations - 1 - index)),
            "slot {index} holds the wrong generation"
        );
    }

    let eleventh =
        snapshot::slot(&database, snapshot::SNAPSHOT_COUNT + 1).expect("the slot has a path");
    assert!(!eleventh.exists(), "the chain grew past ten snapshots");
}

#[test]
fn a_change_on_disk_stops_the_save() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);

    // Another client saves while we hold the database open.
    let (_other, replacement) = support::scratch("minimal-kdbx41.kdbx");
    std::fs::rename(&replacement, &database).expect("the file is replaced");

    assert!(matches!(
        vault.save().expect_err("the save is refused"),
        VaultError::ExternalChange
    ));
}

#[test]
fn a_database_that_vanished_stops_the_save() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);

    std::fs::remove_file(&database).expect("the file is removed");

    assert!(matches!(
        vault.save().expect_err("the save is refused"),
        VaultError::DatabaseGone
    ));
}

#[test]
fn a_refused_save_disturbs_nothing_on_disk() {
    let (scratch, database) = support::scratch("rich-kdbx31.kdbx");
    let before = std::fs::read(&database).expect("the fixture reads");

    let mut vault = open(&database, SECRET);
    assert!(vault.save().is_err());

    assert_eq!(std::fs::read(&database).expect("the file is there"), before);
    assert_eq!(
        std::fs::read_dir(scratch.path())
            .expect("the directory reads")
            .count(),
        2,
        "only the database and its lock file are there: no snapshot was taken"
    );
}

#[test]
fn a_read_only_database_file_is_refused_before_anything_is_disturbed() {
    if !support::permissions_apply() {
        return;
    }

    let (scratch, database) = support::scratch(RICH);
    // The mode is set before the vault records what the file looked like, so
    // that this tests the write permission rather than the change detector.
    std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o400))
        .expect("the mode is set");

    let mut vault = open(&database, SECRET);
    let error = vault.save().expect_err("the save is refused");
    assert!(matches!(error, VaultError::Io(_)));

    assert!(
        !snapshot::slot(&database, 1)
            .expect("the slot has a path")
            .exists(),
        "the snapshot chain was rotated for a save that could never happen"
    );
    std::fs::set_permissions(scratch.path(), std::fs::Permissions::from_mode(0o755)).ok();
}

#[test]
fn a_character_no_keepass_file_can_hold_is_refused() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let id = entry_titled(&vault, "basic").id;

    for rejected in ["a\u{0}b", "a\u{7}b", "a\u{1b}b", "a\u{ffff}b"] {
        assert!(
            matches!(
                vault
                    .set_field(id, fields::NOTES, NewValue::Open(rejected.to_owned()))
                    .expect_err("the value is refused"),
                VaultError::UnwritableText
            ),
            "{rejected:?} was accepted"
        );
    }

    // Tab, newline and carriage return are the three XML does allow.
    vault
        .set_field(id, fields::NOTES, NewValue::Open("a\tb\nc\rd".to_owned()))
        .expect("the value is accepted");
    vault.save().expect("the database saves");
}

#[test]
fn ten_edits_of_an_entry_with_a_three_kilobyte_attachment_barely_grow_the_file() {
    let (_scratch, database) = support::scratch(RICH);

    let baseline = {
        let mut vault = open(&database, SECRET);
        vault.save().expect("the database saves");
        std::fs::metadata(&database)
            .expect("the file is there")
            .len()
    };

    let id = {
        let vault = open(&database, SECRET);
        let entry = entry_titled(&vault, "attachments");
        assert!(
            entry
                .attachments
                .iter()
                .any(|attachment| attachment.size == 3072),
            "the fixture entry carries a three kilobyte attachment"
        );
        entry.id
    };

    for round in 0..10 {
        let mut vault = open(&database, SECRET);
        vault
            .set_field(id, fields::NOTES, NewValue::Open(format!("edit {round}")))
            .expect("the field is written");
        vault.save().expect("the database saves");
    }

    let grown = std::fs::metadata(&database)
        .expect("the file is there")
        .len();
    let growth = grown.saturating_sub(baseline);

    assert!(
        growth < 10 * 3072,
        "ten edits grew the file by {growth} bytes, so the attachment is being copied into every version"
    );
}
