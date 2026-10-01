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

    support::attach(&mut vault, id, "client.p12", &vec![0x5a; 5 * 1024 * 1024]);
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
    support::attach(&mut vault, id, "scan.tiff", &vec![0x17; 7 * 1024 * 1024]);
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
    support::attach(&mut vault, id, "report.pdf", &vec![0x2b; 3 * 1024 * 1024]);

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
    support::attach(&mut vault, id, "attached.bin", &vec![0x3c; 3 * 1024]);
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

/// The one shape where the size limit is the only bound there is: a database
/// that states no item limit at all. Before the bytes of a file stopped being
/// charged to every version that named one, an entry carrying a five-megabyte
/// file kept nothing here whatever the reader did.
#[test]
fn an_entry_with_a_file_and_no_item_limit_is_bounded_by_size_and_not_emptied() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "unlimited.kdbx", |db| {
        // Negative is how KeePass writes "no limit", so size is all that bounds
        // this database.
        db.meta.history_max_items = Some(-1);
        db.meta.history_max_size = None;
        db.root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "subject"));
    });

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    support::attach(&mut vault, id, "client.p12", &vec![0x5a; 5 * 1024 * 1024]);

    // Sixty kilobytes a version, just inside what a field may hold, so that a
    // hundred and fifty of them are half again the six-megabyte fallback and the
    // limit has something to bite on.
    let rounds = 150;
    for round in 0..rounds {
        vault
            .set_field(
                id,
                fields::NOTES,
                NewValue::Open(format!("{round}{}", "x".repeat(60 * 1024))),
            )
            .expect("the note is set");
    }
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, BUILT_PASSWORD);
    let kept = vault.versions(id).len();
    assert!(kept > 0, "an entry carrying a file kept no history at all");
    assert!(
        kept < rounds,
        "the size limit bounded nothing: {kept} versions of sixty kilobytes survived 6 MB"
    );
}

/// An entry with a protected field of the reader's own, and one version behind
/// it, under whatever limits `limits` sets.
fn with_a_pin(
    directory: &std::path::Path,
    limits: impl FnOnce(&mut keepass::Database),
) -> std::path::PathBuf {
    with_a_pin_dated(directory, stamp(0), limits)
}

/// [`with_a_pin`], with the one version behind the entry dated `dated`.
fn with_a_pin_dated(
    directory: &std::path::Path,
    dated: chrono::NaiveDateTime,
    limits: impl FnOnce(&mut keepass::Database),
) -> std::path::PathBuf {
    built(directory, "pin.kdbx", |db| {
        limits(db);
        let id = db
            .root_mut()
            .add_entry()
            .edit(|entry| {
                entry.set_unprotected(fields::TITLE, "bank");
                entry.set_protected("PIN", "4321");
                entry.set_unprotected(fields::NOTES, "then");
            })
            .id();

        let mut entry = db.entry_mut(id).expect("the entry is there");
        entry.times.last_modification = Some(dated);
        entry.edit_tracking(|entry| entry.set(fields::NOTES, Value::unprotected("now")));
    })
}

/// The first moment of a year, for versions another client dated far from now.
fn new_year(year: i32) -> chrono::NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(year, 1, 1)
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .expect("the moment is valid")
}

fn pin(vault: &vault_core::Vault, id: keepass::db::EntryId) -> Option<FieldValue> {
    vault
        .entry(id)
        .expect("the entry is there")
        .field("PIN")
        .map(|field| field.value.clone())
}

/// What an undo of a removal is: the version the removal wrote, restored, and
/// the entry exactly as it was - the value, its protection and every other
/// field - after a save and a fresh open.
#[test]
fn a_removal_is_taken_back_by_the_version_it_wrote() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = with_a_pin(scratch.path(), |_| {});
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = entry_titled(&vault, "bank").id;

    assert_eq!(
        vault.before_removal(id, "PIN"),
        None,
        "a field the entry still has was offered back"
    );

    vault.remove_field(id, "PIN").expect("the field comes off");
    vault.save().expect("the database saves");

    let index = vault
        .before_removal(id, "PIN")
        .expect("the removal can be taken back");
    assert_eq!(
        vault.versions(id).last().map(|version| version.index),
        Some(index),
        "the version offered is not the newest one"
    );

    vault
        .restore_version(id, index)
        .expect("the version is restored");
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, BUILT_PASSWORD);
    assert_eq!(
        pin(&vault, id),
        Some(FieldValue::Protected { empty: false })
    );
    assert_eq!(
        vault
            .reveal(id, "PIN")
            .expect("the value is back")
            .expose_str(),
        Some("4321")
    );
    assert_eq!(
        vault
            .reveal(id, fields::NOTES)
            .expect("the notes are there")
            .expose_str(),
        Some("now"),
        "the undo took back more than the field"
    );
}

/// A database that keeps no versions at all prunes the one a removal wrote on
/// the very next save. There is nothing left to take the removal back with, and
/// the answer has to say so rather than name some other position.
#[test]
fn a_removal_whose_version_the_save_pruned_cannot_be_taken_back() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = with_a_pin(scratch.path(), |db| db.meta.history_max_items = Some(0));
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = entry_titled(&vault, "bank").id;

    vault.remove_field(id, "PIN").expect("the field comes off");
    assert!(
        vault.before_removal(id, "PIN").is_some(),
        "the version is there until the save"
    );

    vault.save().expect("the database saves");
    assert!(vault.versions(id).is_empty(), "the limit kept a version");
    assert_eq!(vault.before_removal(id, "PIN"), None);
}

/// A size limit one version does not fit drops every version, the removal's
/// with the rest.
#[test]
fn a_removal_whose_version_is_over_the_size_limit_cannot_be_taken_back() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = with_a_pin(scratch.path(), |db| db.meta.history_max_size = Some(8));
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = entry_titled(&vault, "bank").id;

    vault.remove_field(id, "PIN").expect("the field comes off");
    vault.save().expect("the database saves");

    assert_eq!(vault.before_removal(id, "PIN"), None);
    assert_eq!(pin(&vault, id), None, "the field came back on its own");
}

/// The newest version in the list is not always the newest one written. A
/// version another client left undated sorts after every dated one, so it is
/// the one a prune keeps - and this one holds the field. Restoring it would
/// bring the field back and take the notes back to what they were before that,
/// which is a restore of something older dressed as an undo.
#[test]
fn an_older_version_holding_the_field_is_never_offered_in_its_place() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "undated.kdbx", |db| {
        db.meta.history_max_items = Some(1);
        let id = db
            .root_mut()
            .add_entry()
            .edit(|entry| {
                entry.set_unprotected(fields::TITLE, "bank");
                entry.set_protected("PIN", "4321");
                entry.set_unprotected(fields::NOTES, "then");
            })
            .id();

        let mut entry = db.entry_mut(id).expect("the entry is there");
        entry.times.last_modification = None;
        entry.edit_tracking(|entry| entry.set(fields::NOTES, Value::unprotected("now")));
    });

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = entry_titled(&vault, "bank").id;
    vault.remove_field(id, "PIN").expect("the field comes off");
    vault.save().expect("the database saves");

    let survivor = vault
        .versions(id)
        .last()
        .map(|version| version.index)
        .expect("the undated version survived the prune");
    assert!(
        vault.reveal_version(id, survivor, "PIN").is_some(),
        "the premise is a surviving version that holds the field"
    );
    assert_eq!(vault.before_removal(id, "PIN"), None);
}

/// Anything done after the removal writes a newer version, and restoring the
/// removal's version from there would undo that as well.
#[test]
fn a_change_after_a_removal_means_it_can_no_longer_be_taken_back() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = with_a_pin(scratch.path(), |_| {});
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = entry_titled(&vault, "bank").id;

    vault.remove_field(id, "PIN").expect("the field comes off");
    vault
        .set_field(id, fields::NOTES, NewValue::Open("later".to_owned()))
        .expect("the notes change");

    assert_eq!(vault.before_removal(id, "PIN"), None);

    // Nor is it an undo of the other field: the name decides what is asked.
    vault
        .remove_field(id, fields::NOTES)
        .expect("the notes come off");
    assert_eq!(vault.before_removal(id, "PIN"), None);
    assert!(vault.before_removal(id, fields::NOTES).is_some());
}

/// A name the entry never had and an entry that is not in this database are
/// both nothing to take back, and neither is a failure worth reporting.
#[test]
fn nothing_is_offered_back_for_a_field_or_an_entry_that_was_never_there() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = with_a_pin(scratch.path(), |_| {});
    let (_other, elsewhere) = support::scratch("minimal-kdbx41.kdbx");
    let absent = open(&elsewhere, SECRET).tree().entries[0].id;

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = entry_titled(&vault, "bank").id;
    vault.remove_field(id, "PIN").expect("the field comes off");

    assert_eq!(vault.before_removal(id, "no such field"), None);
    assert_eq!(vault.before_removal(absent, "PIN"), None);
}

/// A version another client dated in the year 3000 is the newest one in every
/// order Coffer puts versions in, so it stands where the removal's version
/// would. It holds the field, and restoring it would take the notes back as
/// well, so nothing is offered back, before the save or after it.
///
/// Whether the removal's own version survives the save is for the limits to
/// decide. The default keeps it, and the save moves it in front of the version
/// from the future, which renumbers both. A limit of one prunes it first,
/// because next to the year 3000 it is the older of the two.
#[test]
fn a_version_from_the_far_future_is_never_taken_for_the_removals() {
    for (limit, kept) in [(None, 2), (Some(1), 1)] {
        let scratch = tempfile::tempdir().expect("a scratch directory");
        let database = with_a_pin_dated(scratch.path(), new_year(3000), |db| {
            db.meta.history_max_items = limit;
        });
        let mut vault = open(&database, BUILT_PASSWORD);
        let id = entry_titled(&vault, "bank").id;

        vault.remove_field(id, "PIN").expect("the field comes off");
        assert_eq!(
            vault
                .versions(id)
                .iter()
                .map(|version| version.index)
                .collect::<Vec<_>>(),
            vec![1, 0],
            "the removal's version is not listed before the one from the future"
        );
        assert_eq!(vault.before_removal(id, "PIN"), None, "limit {limit:?}");

        vault.save().expect("the database saves");
        assert_eq!(vault.before_removal(id, "PIN"), None, "limit {limit:?}");
        assert_eq!(pin(&vault, id), None, "the field came back on its own");

        let listed = vault.versions(id);
        assert_eq!(listed.len(), kept, "limit {limit:?}");
        assert_eq!(
            listed
                .last()
                .map(|version| (version.index, version.modified)),
            Some((kept - 1, Some(new_year(3000)))),
            "the version from the future went, or kept its old position"
        );
        if kept == 2 {
            assert_eq!(
                vault
                    .reveal_version(id, 0, fields::NOTES)
                    .and_then(|notes| notes.expose_str().map(str::to_owned)),
                Some("now".to_owned()),
                "the removal's version is not where the save put it"
            );
        }
    }
}

/// A version dated 1600 is older than anything Coffer writes, and changes
/// nothing about which version a removal wrote. It keeps its date through a
/// save and a fresh open.
#[test]
fn a_version_from_the_distant_past_leaves_a_removal_its_undo() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = with_a_pin_dated(scratch.path(), new_year(1600), |_| {});
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = entry_titled(&vault, "bank").id;

    vault.remove_field(id, "PIN").expect("the field comes off");
    assert_eq!(vault.before_removal(id, "PIN"), Some(1));

    vault.save().expect("the database saves");
    let index = vault
        .before_removal(id, "PIN")
        .expect("the removal can be taken back");
    assert_eq!(
        vault.versions(id).last().map(|version| version.index),
        Some(index)
    );
    drop(vault);

    let mut vault = open(&database, BUILT_PASSWORD);
    assert_eq!(
        vault
            .versions(id)
            .first()
            .and_then(|version| version.modified),
        Some(new_year(1600)),
        "the date did not survive the file"
    );
    vault
        .restore_version(id, index)
        .expect("the version is restored");
    assert_eq!(
        pin(&vault, id),
        Some(FieldValue::Protected { empty: false })
    );
    assert_eq!(
        vault
            .reveal(id, fields::NOTES)
            .and_then(|notes| notes.expose_str().map(str::to_owned)),
        Some("now".to_owned()),
        "the undo took back more than the field"
    );
}

/// Versions written in one second share a date, because the format keeps no
/// finer one. Here every version of the entry holds the field, the entry and
/// all three versions behind it were last changed in the same second, and the
/// removal writes a version dated that second too. Only the last one written
/// is the removal's, and it is the only one whose restore takes back nothing
/// but the field - before the save and after it.
#[test]
fn edits_and_a_removal_in_one_second_are_taken_back_by_the_removals_version() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "second.kdbx", |db| {
        let id = db
            .root_mut()
            .add_entry()
            .edit(|entry| {
                entry.set_unprotected(fields::TITLE, "bank");
                entry.set_protected("PIN", "4321");
                entry.set_unprotected(fields::NOTES, "one");
            })
            .id();
        for notes in ["two", "three", "now"] {
            let mut entry = db.entry_mut(id).expect("the entry is there");
            entry.times.last_modification = Some(stamp(0));
            entry.edit_tracking(|entry| entry.set(fields::NOTES, Value::unprotected(notes)));
        }
        db.entry_mut(id)
            .expect("the entry is there")
            .times
            .last_modification = Some(stamp(0));
    });
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = entry_titled(&vault, "bank").id;

    vault.remove_field(id, "PIN").expect("the field comes off");
    assert!(
        vault
            .versions(id)
            .iter()
            .all(|version| version.modified == Some(stamp(0))),
        "the premise is four versions dated the same second"
    );
    assert_eq!(vault.before_removal(id, "PIN"), Some(3));

    vault.save().expect("the database saves");
    assert_eq!(vault.before_removal(id, "PIN"), Some(3));
    vault
        .restore_version(id, 3)
        .expect("the version is restored");
    assert_eq!(
        vault
            .reveal(id, fields::NOTES)
            .and_then(|notes| notes.expose_str().map(str::to_owned)),
        Some("now".to_owned()),
        "the undo took back an edit made in the same second"
    );
    assert_eq!(
        pin(&vault, id),
        Some(FieldValue::Protected { empty: false })
    );
}

/// The same thing done the way a reader does it: two edits and a removal in
/// Coffer, one after the other. They land in one second nearly every time, and
/// the answer is the removal's version whether they do or not.
#[test]
fn two_edits_and_a_removal_are_taken_back_to_just_before_the_removal() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = with_a_pin(scratch.path(), |_| {});
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = entry_titled(&vault, "bank").id;

    for notes in ["first", "second"] {
        vault
            .set_field(id, fields::NOTES, NewValue::Open(notes.to_owned()))
            .expect("the notes change");
    }
    vault.remove_field(id, "PIN").expect("the field comes off");
    vault.save().expect("the database saves");

    let index = vault
        .before_removal(id, "PIN")
        .expect("the removal can be taken back");
    vault
        .restore_version(id, index)
        .expect("the version is restored");
    assert_eq!(
        vault
            .reveal(id, fields::NOTES)
            .and_then(|notes| notes.expose_str().map(str::to_owned)),
        Some("second".to_owned()),
        "the undo took back one of the edits before the removal"
    );
    assert_eq!(
        pin(&vault, id),
        Some(FieldValue::Protected { empty: false })
    );
}
