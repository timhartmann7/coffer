//! Entry history: when a version is written, when it is not, and which versions
//! survive a save.

use keepass::db::{Value, fields};
use vault_core::{NewValue, model::FieldValue};

use crate::support::{self, BUILT_PASSWORD, RICH, SECRET, built, entry_titled, open};

/// A database holding one entry, with `versions` previous states already on it.
///
/// Each version is stamped a minute apart. Versions written in a loop all land
/// in the same second, and a test that asserted an order over four identical
/// timestamps would pass whatever the order was.
fn with_versions(directory: &std::path::Path, versions: u32) -> std::path::PathBuf {
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
            let mut entry = database.entry_mut(id).expect("the entry is there");
            entry.times.last_modification = Some(stamp(round));
            entry.edit_tracking(|entry| {
                entry.set(
                    fields::NOTES,
                    Value::unprotected(format!("version {round}")),
                );
            });
        }
    })
}

/// A distinct moment for each version, ascending.
fn stamp(round: u32) -> chrono::NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2024, 1, 1)
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .map(|moment| moment + chrono::Duration::minutes(i64::from(round)))
        .expect("the moment is valid")
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

    // The fixture was built through the library, which puts the newest version
    // at position 0. The list still comes back oldest first, and each entry's
    // index still addresses the version it came from.
    let expected: Vec<_> = (0..4).map(|round| Some(stamp(round))).collect();
    let listed = vault.versions(id);
    assert_eq!(
        listed
            .iter()
            .map(|version| version.modified)
            .collect::<Vec<_>>(),
        expected,
        "versions came back in the wrong order before the save"
    );
    assert_eq!(
        listed
            .iter()
            .map(|version| version.index)
            .collect::<Vec<_>>(),
        vec![3, 2, 1, 0],
        "the index should stay the position in the file, not the position in the list"
    );

    // An edit of our own has to land at the end, not at the front where the
    // library puts it.
    vault
        .set_field(id, fields::NOTES, NewValue::Open("newest".to_owned()))
        .expect("the field is written");
    let after_edit = vault.versions(id);
    assert_eq!(after_edit.len(), 5);
    assert!(
        after_edit
            .last()
            .and_then(|version| version.modified)
            .is_some_and(|moment| moment > stamp(3)),
        "the version just written is not the newest one in the list"
    );

    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, BUILT_PASSWORD);
    let dates: Vec<_> = vault
        .versions(id)
        .into_iter()
        .map(|version| version.modified)
        .collect();
    assert_eq!(dates.len(), 5);
    assert_eq!(
        dates.iter().take(4).copied().collect::<Vec<_>>(),
        expected,
        "the saved order is not oldest first"
    );
    assert_eq!(
        vault
            .versions(id)
            .iter()
            .map(|version| version.index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4],
        "the save should have put the file's own order right as well"
    );
}

#[test]
fn pruning_drops_the_oldest_version_when_timestamps_tie() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "tied.kdbx", |db| {
        db.meta.history_max_items = Some(2);
        let id = db
            .root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "subject"))
            .id();

        // Two pairs, each pair sharing a modification time. Pruning to two has
        // to keep the later pair, and a tie-break that favoured the front of
        // the list would keep one from each.
        for round in 0..4 {
            let mut entry = db.entry_mut(id).expect("the entry is there");
            entry.times.last_modification = Some(stamp(round / 2));
            entry.edit_tracking(|entry| {
                entry.set(fields::NOTES, Value::unprotected(format!("v{round}")));
            });
        }
    });

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    assert_eq!(vault.versions(id).len(), 4);
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, BUILT_PASSWORD);
    assert_eq!(
        vault
            .versions(id)
            .into_iter()
            .map(|version| version.modified)
            .collect::<Vec<_>>(),
        vec![Some(stamp(1)), Some(stamp(1))],
        "a tie sent the wrong version to the bin"
    );
}

#[test]
fn editing_an_entry_that_is_not_there_says_so() {
    let (_scratch, database) = support::scratch(RICH);
    let (_other, elsewhere) = support::scratch("minimal-kdbx41.kdbx");

    // An identifier that is real, just not in this database.
    let absent = open(&elsewhere, SECRET).tree().entries[0].id;
    let mut vault = open(&database, SECRET);

    assert!(matches!(
        vault
            .set_field(absent, fields::NOTES, NewValue::Open("x".to_owned()))
            .expect_err("there is no such entry"),
        vault_core::VaultError::NoSuchEntry
    ));
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
            let mut entry = db.entry_mut(id).expect("the entry is there");
            entry.times.last_modification = Some(stamp(round));
            entry.edit_tracking(|entry| {
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

    // The three that survive have to be the three newest. A prune that dropped
    // from the wrong end would keep three versions too.
    let vault = open(&database, BUILT_PASSWORD);
    assert_eq!(
        vault
            .versions(id)
            .into_iter()
            .map(|version| version.modified)
            .collect::<Vec<_>>(),
        vec![Some(stamp(6)), Some(stamp(7)), Some(stamp(8))]
    );
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

/// Whether an entry expires, and when, is something the reader chose. An edit
/// that changed only that is an edit, and a restore that brought only that back
/// is a restore - the version it replaced has to be kept, and the date has to
/// be the one that was asked for.
#[test]
fn a_change_to_nothing_but_the_expiry_date_is_still_a_change() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let expires = chrono::NaiveDate::from_ymd_opt(2030, 1, 2)
        .and_then(|day| day.and_hms_opt(3, 4, 5))
        .expect("the moment is valid");

    let path = built(directory.path(), "expiry.kdbx", |database| {
        let id = database
            .root_mut()
            .add_entry()
            .edit(|entry| {
                entry.set_unprotected(fields::TITLE, "subject");
                entry.times.expires = Some(true);
                entry.times.expiry = Some(expires);
            })
            .id();

        // The one thing the next state changes is that it no longer expires,
        // which is the shape KeePassXC writes when somebody clears the date.
        database
            .entry_mut(id)
            .expect("the entry is there")
            .edit_tracking(|entry| {
                entry.times.expires = Some(false);
                entry.times.expiry = None;
            });
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    let id = entry_titled(&vault, "subject").id;
    assert_eq!(vault.versions(id).len(), 1, "the edit kept its version");
    assert_eq!(
        vault.entry(id).expect("the entry is there").times.expires,
        None
    );

    vault
        .restore_version(id, 0)
        .expect("the version is restored");

    assert_eq!(
        vault.entry(id).expect("the entry is there").times.expires,
        Some(expires),
        "the restore reported success and brought nothing back"
    );
    assert_eq!(
        vault.versions(id).len(),
        2,
        "the state the restore replaced was not kept"
    );
}

/// The whole reason history is in this application. `SPEC.md` puts it as "I
/// overwrote a password, saved, and noticed a week later", and attaching a scan
/// or a certificate to the entry used to be the end of that promise: the bytes
/// of the file were charged to every version that named it, one file past the
/// six-megabyte fallback weighed more than the limit on its own, and so every
/// version went and no new one could ever be kept.
#[test]
fn a_file_on_an_entry_does_not_take_the_versions_of_that_entry_with_it() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "documents.kdbx", |db| {
        db.meta.history_max_items = Some(100);
        // Left unstated on purpose: it is the six-megabyte fallback that a file
        // of a few megabytes used to break, not a limit anybody chose.
        db.meta.history_max_size = None;
        db.root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "Client VPN"));
    });

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;

    for round in 0..10 {
        vault
            .set_field(
                id,
                fields::PASSWORD,
                NewValue::Protected(zeroize::Zeroizing::new(format!("secret {round}"))),
            )
            .expect("the password is set");
        vault.save().expect("the database saves");
    }
    assert_eq!(vault.versions(id).len(), 10);

    vault
        .add_attachment(
            id,
            "client.p12",
            zeroize::Zeroizing::new(vec![0x5a; 5 * 1024 * 1024]),
        )
        .expect("the file attaches");
    vault.save().expect("the database saves");
    assert_eq!(
        vault.versions(id).len(),
        10,
        "attaching a file threw the entry's history away"
    );

    for round in 10..14 {
        vault
            .set_field(
                id,
                fields::PASSWORD,
                NewValue::Protected(zeroize::Zeroizing::new(format!("secret {round}"))),
            )
            .expect("the password is set");
        vault.save().expect("the database saves");
    }

    // A second file, larger on its own than the fallback limit. This is the
    // case that used to leave the entry with no history at all, for good.
    vault
        .add_attachment(
            id,
            "scan.tiff",
            zeroize::Zeroizing::new(vec![0x17; 7 * 1024 * 1024]),
        )
        .expect("the file attaches");
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, BUILT_PASSWORD);
    assert_eq!(
        vault.versions(id).len(),
        14,
        "a file larger than the limit left the entry with no history"
    );
}

/// The size limit still has to bite. A version's fields are what dropping it
/// gives back, and a run of large ones is still pruned by size with a file
/// sitting on the entry the whole time.
#[test]
fn the_size_limit_still_prunes_an_entry_that_carries_a_file() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "heavy-with-a-file.kdbx", |db| {
        db.meta.history_max_items = Some(100);
        db.meta.history_max_size = Some(4 * 1100);
        db.root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "subject"));
    });

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault
        .add_attachment(
            id,
            "report.pdf",
            zeroize::Zeroizing::new(vec![0x2b; 3 * 1024 * 1024]),
        )
        .expect("the file attaches");

    for round in 0..20 {
        vault
            .set_field(
                id,
                fields::NOTES,
                NewValue::Open(format!("{round}{}", "x".repeat(1024))),
            )
            .expect("the note is set");
        vault.save().expect("the database saves");
    }
    drop(vault);

    let vault = open(&database, BUILT_PASSWORD);
    let kept = vault.versions(id).len();
    assert!(
        (1..=5).contains(&kept),
        "the size limit kept {kept} versions of a kilobyte each under a limit of four"
    );
}

/// The bound the size limit used to provide by accident. A hundred saves of an
/// entry carrying a file leave a file the count limit holds down, and not one
/// that grew by a version every time.
#[test]
fn a_hundred_saves_of_an_entry_that_carries_a_file_leave_it_bounded() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "bounded.kdbx", |db| {
        db.root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "subject"));
    });

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault
        .add_attachment(
            id,
            "attached.bin",
            zeroize::Zeroizing::new(vec![0x3c; 3 * 1024]),
        )
        .expect("the file attaches");
    vault.save().expect("the database saves");

    let settled = std::fs::metadata(&database)
        .expect("the file is there")
        .len();

    for round in 0..100 {
        vault
            .set_field(id, fields::NOTES, NewValue::Open(format!("round {round}")))
            .expect("the note is set");
        vault.save().expect("the database saves");
    }
    drop(vault);

    let vault = open(&database, BUILT_PASSWORD);
    assert_eq!(
        vault.versions(id).len(),
        10,
        "the count limit is what bounds an entry whose files are no longer charged"
    );

    let grown = std::fs::metadata(&database)
        .expect("the file is there")
        .len();
    assert!(
        grown < settled + 64 * 1024,
        "a hundred saves grew the file from {settled} to {grown}"
    );
}
