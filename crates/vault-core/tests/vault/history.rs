//! Entry history: when a version is written, when it is not, and which versions
//! survive a save.

use keepass::db::{Value, fields};
use vault_core::{NewValue, model::FieldValue};

use crate::support::{self, BUILT_PASSWORD, built, entry_titled, open};

const RICH: &str = "rich-kdbx41.kdbx";
const SECRET: &str = "coffer-test";

/// A database holding one entry, with `versions` previous states already on it.
fn with_versions(directory: &std::path::Path, versions: usize) -> std::path::PathBuf {
    built(directory, "versioned.kdbx", |database| {
        let id = database
            .root_mut()
            .add_entry()
            .edit(|entry| {
                entry.set_unprotected(fields::TITLE, "subject");
                entry.set_unprotected(fields::NOTES, "original");
            })
            .id();

        for round in 0..versions {
            database
                .entry_mut(id)
                .expect("the entry is there")
                .edit_tracking(|entry| {
                    entry.set(
                        fields::NOTES,
                        Value::unprotected(format!("version {round}")),
                    );
                });
        }
    })
}

#[test]
fn an_edit_records_the_previous_state() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let id = entry_titled(&vault, "basic").id;

    let before = vault.versions(id).len();
    vault
        .set_field(id, fields::NOTES, NewValue::Open("changed".to_owned()))
        .expect("the field is written");

    assert_eq!(vault.versions(id).len(), before + 1);

    let previous = vault
        .versions(id)
        .last()
        .copied()
        .expect("a version was recorded");
    assert_eq!(previous.index, before);
}

#[test]
fn an_edit_that_changes_nothing_records_nothing() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let entry = entry_titled(&vault, "basic");
    let id = entry.id;

    let before = vault.versions(id).len();
    let modified_before = entry.times.modified;

    // The same username the entry already carries.
    vault
        .set_field(id, fields::USERNAME, NewValue::Open("alice".to_owned()))
        .expect("the field is written");

    assert_eq!(
        vault.versions(id).len(),
        before,
        "an edit that changed nothing wrote a version"
    );
    assert_eq!(
        vault.entry(id).expect("the entry is there").times.modified,
        modified_before,
        "an edit that changed nothing moved the modification time"
    );
}

#[test]
fn changing_a_field_from_open_to_protected_counts_as_a_change() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let id = entry_titled(&vault, "basic").id;
    let before = vault.versions(id).len();

    vault
        .set_field(
            id,
            fields::USERNAME,
            NewValue::Protected(zeroize::Zeroizing::new("alice".to_owned())),
        )
        .expect("the field is written");

    assert_eq!(vault.versions(id).len(), before + 1);
    assert_eq!(
        vault
            .entry(id)
            .expect("the entry is there")
            .field(fields::USERNAME)
            .map(|field| field.value.clone()),
        Some(FieldValue::Protected { empty: false })
    );
}

#[test]
fn versions_come_back_and_are_written_oldest_first() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = with_versions(scratch.path(), 4);

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, BUILT_PASSWORD);
    let dates: Vec<_> = vault
        .versions(id)
        .into_iter()
        .map(|version| version.modified)
        .collect();

    assert_eq!(dates.len(), 4);
    let mut sorted = dates.clone();
    sorted.sort();
    assert_eq!(dates, sorted, "versions were not written oldest first");
}

#[test]
fn history_is_pruned_to_the_databases_item_limit_on_save() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "limited.kdbx", |db| {
        db.meta.history_max_items = Some(3);
        let id = db
            .root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "subject"))
            .id();
        for round in 0..9 {
            db.entry_mut(id)
                .expect("the entry is there")
                .edit_tracking(|entry| {
                    entry.set(
                        fields::NOTES,
                        Value::unprotected(format!("version {round}")),
                    );
                });
        }
    });

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    assert_eq!(
        vault.versions(id).len(),
        9,
        "the fixture starts over the limit"
    );

    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, BUILT_PASSWORD);
    assert_eq!(vault.versions(id).len(), 3);

    // The three kept are the newest three, so the oldest note is gone.
    let notes: Vec<_> = vault
        .versions(id)
        .into_iter()
        .filter_map(|version| version.modified)
        .collect();
    assert_eq!(notes.len(), 3);
}

#[test]
fn history_is_pruned_to_the_databases_size_limit_on_save() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "heavy.kdbx", |db| {
        db.meta.history_max_items = Some(100);
        // Each version carries a kilobyte of notes, so four fit in the limit.
        db.meta.history_max_size = Some(4 * 1100);
        let id = db
            .root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "subject"))
            .id();
        for round in 0..20 {
            db.entry_mut(id)
                .expect("the entry is there")
                .edit_tracking(|entry| {
                    entry.set(
                        fields::NOTES,
                        Value::unprotected(format!("{round}{}", "x".repeat(1024))),
                    );
                });
        }
    });

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    assert_eq!(vault.versions(id).len(), 20);

    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, BUILT_PASSWORD);
    let kept = vault.versions(id).len();
    assert!(
        (1..=5).contains(&kept),
        "the size limit kept {kept} versions of about a kilobyte each under a four kilobyte limit"
    );
}

#[test]
fn an_entry_that_arrives_over_the_limit_is_pruned_on_save_even_if_it_is_never_edited() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "inherited.kdbx", |db| {
        db.meta.history_max_items = Some(2);
        let untouched = db
            .root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "untouched"))
            .id();
        for round in 0..7 {
            db.entry_mut(untouched)
                .expect("the entry is there")
                .edit_tracking(|entry| {
                    entry.set(fields::NOTES, Value::unprotected(format!("v{round}")));
                });
        }
    });

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    assert_eq!(vault.versions(id).len(), 7);

    // Nothing is edited: the save alone has to bring it inside the limit.
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, BUILT_PASSWORD);
    assert_eq!(vault.versions(id).len(), 2);
}

#[test]
fn a_database_that_states_no_limit_falls_back_to_ten_versions() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "silent.kdbx", |db| {
        db.meta.history_max_items = None;
        db.meta.history_max_size = None;
        let id = db
            .root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "subject"))
            .id();
        for round in 0..25 {
            db.entry_mut(id)
                .expect("the entry is there")
                .edit_tracking(|entry| {
                    entry.set(fields::NOTES, Value::unprotected(format!("v{round}")));
                });
        }
    });

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault.save().expect("the database saves");
    drop(vault);

    assert_eq!(open(&database, BUILT_PASSWORD).versions(id).len(), 10);
}

#[test]
fn a_negative_limit_means_no_limit() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "unlimited.kdbx", |db| {
        db.meta.history_max_items = Some(-1);
        db.meta.history_max_size = Some(-1);
        let id = db
            .root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "subject"))
            .id();
        for round in 0..15 {
            db.entry_mut(id)
                .expect("the entry is there")
                .edit_tracking(|entry| {
                    entry.set(fields::NOTES, Value::unprotected(format!("v{round}")));
                });
        }
    });

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault.save().expect("the database saves");
    drop(vault);

    assert_eq!(open(&database, BUILT_PASSWORD).versions(id).len(), 15);
}

#[test]
fn a_thousand_saves_of_one_entry_leave_the_file_bounded() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "hammered.kdbx", |db| {
        db.meta.history_max_items = Some(10);
        db.root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "subject"));
    });

    let id = open(&database, BUILT_PASSWORD).tree().entries[0].id;
    let mut sizes = Vec::new();

    for round in 0..1000 {
        let mut vault = open(&database, BUILT_PASSWORD);
        vault
            .set_field(id, fields::NOTES, NewValue::Open(format!("edit {round}")))
            .expect("the field is written");
        vault.save().expect("the database saves");
        drop(vault);

        if round % 100 == 99 {
            sizes.push(
                std::fs::metadata(&database)
                    .expect("the file is there")
                    .len(),
            );
        }
    }

    let vault = open(&database, BUILT_PASSWORD);
    assert_eq!(vault.versions(id).len(), 10, "history was not pruned");

    let first = sizes.first().copied().unwrap_or_default();
    let last = sizes.last().copied().unwrap_or_default();
    assert!(
        last <= first + 1024,
        "the file grew from {first} to {last} bytes over a thousand saves"
    );
}
