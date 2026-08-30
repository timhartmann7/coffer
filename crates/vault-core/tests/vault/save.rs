//! Writing a database back: what is checked before anything is disturbed, and
//! what the directory looks like afterwards.

use std::os::unix::fs::PermissionsExt;

use keepass::db::fields;
use vault_core::storage::snapshot;
use vault_core::{NewValue, VaultError};

use crate::support::{self, BUILT_PASSWORD, RICH, SECRET, built, entry_titled, open};

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
fn a_rotation_that_died_half_way_recovers_on_the_next_save() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "cheap.kdbx", |db| {
        db.root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "one"));
    });

    // What a process killed between the shift and the capture leaves behind:
    // slot 1 still occupied by the generation that should have moved on.
    let first = snapshot::slot(&database, 1).expect("the slot has a path");
    std::fs::write(&first, b"a snapshot from a rotation that never finished")
        .expect("the slot is seeded");

    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault
        .set_field(id, fields::NOTES, NewValue::Open("after".to_owned()))
        .expect("the field is written");
    vault.save().expect("the database saves");

    // Slot 1 holds a database again, and the leftover moved down rather than
    // being lost.
    open(&first, BUILT_PASSWORD);
    assert_eq!(
        std::fs::read(snapshot::slot(&database, 2).expect("the slot has a path"))
            .expect("the second slot is there"),
        b"a snapshot from a rotation that never finished"
    );
}

/// Text no compressor can shrink, built from a hash chain so that it is the
/// same on every run.
fn incompressible(bytes: usize) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};

    let mut text = String::with_capacity(bytes + 16);
    let mut seed: u64 = 0x5eed;
    while text.len() < bytes {
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        seed = hasher.finish();
        let _ = std::fmt::Write::write_fmt(&mut text, format_args!("{seed:016x}"));
    }
    text
}

/// Set in the child process the out-of-space test spawns.
const CHILD_FULL_VOLUME: &str = "COFFER_CHILD_FULL_VOLUME_PATH";

#[test]
fn a_save_that_runs_out_of_room_leaves_the_database_and_its_snapshots_alone() {
    if let Some(path) = std::env::var_os(CHILD_FULL_VOLUME) {
        // This process is the child. A file size limit smaller than the
        // database makes the write fail part way through, which is what a full
        // volume looks like from inside a save and what no check before the
        // write can predict.
        //
        // SAFETY: both calls set properties of this process and touch nothing
        // else. SIGXFSZ would otherwise kill it before the write could report
        // the failure.
        unsafe {
            libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
            let limit = libc::rlimit {
                rlim_cur: 1024,
                rlim_max: 1024,
            };
            assert_eq!(
                libc::setrlimit(libc::RLIMIT_FSIZE, &raw const limit),
                0,
                "the file size limit is set"
            );
        }

        let path = std::path::PathBuf::from(path);
        let mut vault = open(&path, BUILT_PASSWORD);
        let id = vault.tree().entries[0].id;
        // Incompressible on purpose: the payload is gzipped before it is
        // written, and a field of repeated characters would fit inside the
        // limit however long it was.
        vault
            .set_field(id, fields::NOTES, NewValue::Open(incompressible(200_000)))
            .expect("the field is written");

        // The save must fail, and it must fail without disturbing anything.
        assert!(
            vault.save().is_err(),
            "the write should have run out of room"
        );
        return;
    }

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "cramped.kdbx", |db| {
        db.root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "one"));
    });

    // Three generations of real history in the snapshot chain.
    for round in 0..3 {
        let mut vault = open(&database, BUILT_PASSWORD);
        let id = vault.tree().entries[0].id;
        vault
            .set_field(id, fields::NOTES, NewValue::Open(format!("round {round}")))
            .expect("the field is written");
        vault.save().expect("the database saves");
    }

    let before = std::fs::read(&database).expect("the database reads");
    let chain: Vec<Vec<u8>> = (1..=3)
        .map(|index| {
            std::fs::read(snapshot::slot(&database, index).expect("the slot has a path"))
                .expect("the slot is there")
        })
        .collect();

    let status = std::process::Command::new(
        std::env::current_exe().expect("the test binary knows its own path"),
    )
    .args([
        "--exact",
        "save::a_save_that_runs_out_of_room_leaves_the_database_and_its_snapshots_alone",
    ])
    .env(CHILD_FULL_VOLUME, &database)
    .output()
    .expect("the child runs");
    assert!(
        status.status.success(),
        "the child's own assertions failed:\n{}",
        String::from_utf8_lossy(&status.stdout)
    );

    assert_eq!(
        std::fs::read(&database).expect("the database is still there"),
        before,
        "a save that ran out of room changed the database"
    );
    for (index, expected) in chain.into_iter().enumerate() {
        let slot = snapshot::slot(&database, index as u32 + 1).expect("the slot has a path");
        assert_eq!(
            std::fs::read(&slot).expect("the slot is still there"),
            expected,
            "a save that ran out of room rotated snapshot slot {}",
            index + 1
        );
    }

    // And the vault is not wedged: a save that can succeed still does.
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault
        .set_field(id, fields::NOTES, NewValue::Open("afterwards".to_owned()))
        .expect("the field is written");
    vault.save().expect("the vault still saves");
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
fn the_vault_can_be_asked_whether_the_file_changed_without_saving() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);

    assert_eq!(
        vault.external_change().expect("the file is checked"),
        vault_core::storage::watch::Change::None
    );

    let (_other, replacement) = support::scratch("minimal-kdbx41.kdbx");
    std::fs::rename(&replacement, &database).expect("the file is replaced");

    assert_eq!(
        vault.external_change().expect("the file is checked"),
        vault_core::storage::watch::Change::Modified
    );

    std::fs::remove_file(&database).expect("the file goes");
    assert_eq!(
        vault.external_change().expect("the file is checked"),
        vault_core::storage::watch::Change::Gone
    );
}

#[test]
fn saving_records_the_new_state_so_the_next_save_is_not_a_conflict() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);

    vault.save().expect("the first save succeeds");
    vault
        .save()
        .expect("the second save is not treated as a conflict");
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

    let (id, before) = {
        let vault = open(&database, SECRET);
        let entry = entry_titled(&vault, "attachments");
        let blob = vault
            .attachment(entry.id, "nested/path/name.txt")
            .expect("the attachment is there");
        assert_eq!(
            blob.expose().len(),
            3072,
            "the fixture carries three kilobytes"
        );
        (entry.id, blob.expose().to_vec())
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
        "ten edits grew the file by {growth} bytes"
    );

    // The size on its own proves less than it looks. The payload is gzipped as
    // a whole, and ten identical copies of a blob compress to barely more than
    // one, so a writer that did copy the attachment into every version would
    // slip under any bound this test could set. What the criterion is really
    // about is that the versions share the blob, and that it is still the blob
    // the fixture had.
    let vault = open(&database, SECRET);
    let entry = vault.entry(id).expect("the entry is there");
    assert_eq!(entry.versions, 10, "ten edits should be ten versions");
    assert_eq!(
        entry.attachments.len(),
        4,
        "the entry gained or lost an attachment"
    );
    assert_eq!(
        vault
            .attachment(id, "nested/path/name.txt")
            .expect("the attachment is still there")
            .expose(),
        before,
        "the attachment came back changed"
    );
}

/// macOS writes `com.apple.macl` and `com.apple.provenance` onto a file after an
/// application has touched it through a save panel, and an extended attribute
/// moves change time without moving a byte of content. A vault that took that
/// for another client's edit would raise the conflict dialog on every save the
/// reader made, which is what happened on a real vault in `~/Documents`.
#[test]
fn an_attribute_the_system_writes_is_not_somebody_elses_edit() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "attributed.kdbx", |_| {});
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let id = vault.create_entry(root).expect("an entry is made");

    vault
        .set_field(id, fields::NOTES, NewValue::Open("first".to_owned()))
        .expect("the note is written");
    vault.save().expect("the first save lands");

    // What the system does, in the one way a test can do it: nothing about the
    // file changes except its change time.
    let before = std::fs::metadata(&path).expect("the vault is there");
    support::set_attribute(&path);
    let after = std::fs::metadata(&path).expect("the vault is still there");

    assert_eq!(before.len(), after.len(), "the attribute changed the size");
    assert_eq!(
        before.modified().ok(),
        after.modified().ok(),
        "the attribute changed the modification time"
    );

    vault
        .set_field(id, fields::NOTES, NewValue::Open("second".to_owned()))
        .expect("the note is written");
    vault
        .save()
        .expect("the vault refused its own file over an attribute the system wrote");
}
