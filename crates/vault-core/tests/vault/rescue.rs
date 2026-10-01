//! What a lock does with work the file has not got.

use std::os::unix::fs::PermissionsExt;

use vault_core::storage::unsaved;
use vault_core::{NewValue, Rescue, VaultError};

use crate::support::{self, BUILT_PASSWORD, built, open, password, permissions_apply};

/// Somebody else's client writing the file while Coffer holds it open, which is
/// what `SPEC.md` section 11 asks the reader to do for the first month.
///
/// A real second write rather than bytes appended to the file: what the first
/// vault has to find is another version of its database, not a broken one, and
/// the tests that open the file afterwards need it to still be a database.
fn somebody_else_writes(database: &std::path::Path) {
    let mut theirs = vault_core::Vault::open(
        database,
        password(BUILT_PASSWORD),
        vault_core::LockPolicy::TakeOver,
    )
    .expect("the other client opens it");
    let id = theirs.tree().entries[0].id;
    theirs
        .set_field(
            id,
            keepass::db::fields::URL,
            NewValue::Open("https://theirs.example".into()),
        )
        .expect("their change is applied");
    theirs.save().expect("their save goes through");
}

/// Every snapshot beside a database, by slot and by content.
fn snapshots(database: &std::path::Path) -> Vec<(u32, Vec<u8>)> {
    (1..=10)
        .filter_map(|index| {
            let path = vault_core::storage::snapshot::slot(database, index).ok()?;
            Some((index, std::fs::read(path).ok()?))
        })
        .collect()
}

/// One entry, saved and closed, so that the vault under test comes back the way
/// a vault from disk comes back.
fn vault_with_an_entry(directory: &std::path::Path, name: &str) -> std::path::PathBuf {
    built(directory, name, |db| {
        db.root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(keepass::db::fields::TITLE, "subject"));
    })
}

/// The plain course of things, and the three states of a vault in one test:
/// nothing to write, something to write, and nothing again once it is written.
#[test]
fn a_lock_with_somewhere_to_save_saves_where_a_save_would_have_gone() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = vault_with_an_entry(scratch.path(), "ordinary.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;

    assert_eq!(
        vault.rescue(),
        Rescue::Nothing,
        "an untouched vault is written"
    );

    vault
        .set_field(
            id,
            keepass::db::fields::USERNAME,
            NewValue::Open("who".into()),
        )
        .expect("the field is set");
    assert_eq!(vault.rescue(), Rescue::Saved);
    assert_eq!(
        vault.rescue(),
        Rescue::Nothing,
        "a vault that has just been saved is written a second time"
    );
    drop(vault);

    assert!(
        !unsaved::beside(&database).expect("a sibling path").exists(),
        "a save that went through still left a copy behind"
    );

    let again = open(&database, BUILT_PASSWORD);
    let entry = again.entry(id).expect("the entry is there");
    assert_eq!(
        entry
            .field(keepass::db::fields::USERNAME)
            .and_then(|held| held.value.open()),
        Some("who")
    );
}

/// The defect this exists for. Somebody else writes the file, the save is
/// refused, and the reader walks away: the idle timer must not be what destroys
/// the work.
#[test]
fn a_lock_that_cannot_save_leaves_the_work_in_a_file_beside_the_vault() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = vault_with_an_entry(scratch.path(), "contested.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;

    vault
        .set_field(
            id,
            keepass::db::fields::PASSWORD,
            NewValue::Protected(zeroize::Zeroizing::new("the new one".to_owned())),
        )
        .expect("the field is set");

    // KeePassXC saves underneath, which is what SPEC section 11 asks the reader
    // to do for the first month.
    somebody_else_writes(&database);
    assert!(matches!(vault.save(), Err(VaultError::ExternalChange)));

    assert_eq!(vault.rescue(), Rescue::Kept);
    drop(vault);

    let kept = unsaved::beside(&database).expect("a sibling path");
    let rescued = open(&kept, BUILT_PASSWORD);
    assert_eq!(
        rescued
            .reveal(id, keepass::db::fields::PASSWORD)
            .as_ref()
            .and_then(vault_core::SecretValue::expose_str),
        Some("the new one"),
        "the copy does not hold the change the vault could not save"
    );
}

/// Hard rule 1 cuts both ways. The copy holds the work, and the file the reader
/// still believes in is exactly as it was found: the same bytes, and the same
/// chain of snapshots behind it.
#[test]
fn a_rescue_leaves_the_file_it_could_not_save_exactly_as_it_found_it() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = vault_with_an_entry(scratch.path(), "untouched.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;

    vault
        .set_field(
            id,
            keepass::db::fields::NOTES,
            NewValue::Open("mine".into()),
        )
        .expect("the field is set");
    somebody_else_writes(&database);

    let before = std::fs::read(&database).expect("the database reads");
    let chain = snapshots(&database);

    assert_eq!(vault.rescue(), Rescue::Kept);
    drop(vault);

    assert_eq!(
        std::fs::read(&database).expect("the database reads"),
        before,
        "the rescue wrote over the file it was refused"
    );
    assert_eq!(
        snapshots(&database),
        chain,
        "the rescue pushed a generation through the snapshot chain"
    );
}

/// The volume was unplugged, or the file was renamed out from under the window.
/// There is nowhere to save and the work still has to land somewhere.
#[test]
fn a_vault_whose_file_is_gone_still_gets_its_work_out() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = vault_with_an_entry(scratch.path(), "vanishing.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;

    vault
        .set_field(
            id,
            keepass::db::fields::NOTES,
            NewValue::Open("kept".into()),
        )
        .expect("the field is set");
    std::fs::remove_file(&database).expect("the database goes");

    assert_eq!(vault.rescue(), Rescue::Kept);
    drop(vault);

    let kept = unsaved::beside(&database).expect("a sibling path");
    assert_eq!(open(&kept, BUILT_PASSWORD).tree().entries.len(), 1);
}

/// Nowhere to write at all. The answer says so and the caller wipes the vault
/// regardless: a vault left unlocked because it had unsaved work would be the
/// whole of the locking gone.
#[test]
fn nowhere_to_write_is_said_plainly_and_leaves_nothing_behind() {
    if !permissions_apply() {
        return;
    }

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = vault_with_an_entry(scratch.path(), "frozen.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;

    vault
        .set_field(
            id,
            keepass::db::fields::NOTES,
            NewValue::Open("lost".into()),
        )
        .expect("the field is set");

    let _frozen = support::Frozen::over(scratch.path());
    assert_eq!(vault.rescue(), Rescue::Lost);

    assert!(
        !unsaved::beside(&database).expect("a sibling path").exists(),
        "a rescue that could not be written left half a file behind"
    );
}

/// The copy holds the reader's passwords, so it is born owner-only like every
/// other file Coffer writes. A copy anybody on the machine can read would be a
/// worse defect than the one the rescue exists to fix.
#[test]
fn an_unsaved_copy_is_readable_only_by_its_owner() {
    if !permissions_apply() {
        return;
    }

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = vault_with_an_entry(scratch.path(), "modes.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;

    vault
        .set_field(
            id,
            keepass::db::fields::NOTES,
            NewValue::Open("mine".into()),
        )
        .expect("the field is set");
    somebody_else_writes(&database);
    assert_eq!(vault.rescue(), Rescue::Kept);

    let kept = unsaved::beside(&database).expect("a sibling path");
    let mode = std::fs::metadata(&kept)
        .expect("the copy is there")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "the copy is readable by somebody else");
}

/// A second lock with the same trouble replaces the first copy rather than
/// leaving a fan of them. There is one file, and it holds the newest work.
#[test]
fn a_second_rescue_replaces_the_first() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = vault_with_an_entry(scratch.path(), "twice.kdbx");
    let kept = unsaved::beside(&database).expect("a sibling path");

    for round in ["first", "second"] {
        let mut vault = open(&database, BUILT_PASSWORD);
        let id = vault.tree().entries[0].id;
        vault
            .set_field(id, keepass::db::fields::NOTES, NewValue::Open(round.into()))
            .expect("the field is set");
        somebody_else_writes(&database);
        assert_eq!(vault.rescue(), Rescue::Kept);
    }

    let rescued = open(&kept, BUILT_PASSWORD);
    let id = rescued.tree().entries[0].id;
    let entry = rescued.entry(id).expect("the entry is there");
    assert_eq!(
        entry
            .field(keepass::db::fields::NOTES)
            .and_then(|held| held.value.open()),
        Some("second")
    );
}

/// A vault cannot be made at the name, because the next lock with something to
/// keep would write over it without a word.
#[test]
fn a_name_belonging_to_an_unsaved_copy_is_refused() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let taken = unsaved::beside(&scratch.path().join("vault.kdbx")).expect("a sibling path");

    let refused = vault_core::Vault::create(
        &taken,
        password(BUILT_PASSWORD),
        &vault_core::Recipe {
            name: "Work",
            work: vault_core::kdf::Work::at(1),
        },
    )
    .expect_err("the name is refused");
    assert!(matches!(refused, VaultError::ReservedName));
}

/// One entry with a field of the reader's own beside the five, protected the
/// way Coffer makes one, and no notes at all.
fn vault_for_typing(directory: &std::path::Path) -> std::path::PathBuf {
    built(directory, "typing.kdbx", |db| {
        db.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(keepass::db::fields::TITLE, "subject");
            entry.set_unprotected(keepass::db::fields::USERNAME, "alice");
            entry.set_protected("PIN", "1234");
        });
    })
}

fn value(vault: &vault_core::Vault, id: vault_core::model::EntryId, field: &str) -> Option<String> {
    vault
        .reveal(id, field)
        .as_ref()
        .and_then(vault_core::SecretValue::expose_str)
        .map(str::to_owned)
}

/// Text a lock finds in a field is written the way leaving the field would
/// have written it: the entry's previous state is kept as a version, and the
/// vault has something to save.
#[test]
fn typing_a_lock_finishes_is_an_edit_like_any_other() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = vault_for_typing(scratch.path());
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;

    for (field, typed) in [
        (keepass::db::fields::USERNAME, NewValue::Open("alic".into())),
        (
            "PIN",
            NewValue::Protected(zeroize::Zeroizing::new("98".to_owned())),
        ),
        // A standard field the entry never had is drawn all the same, and
        // written into like any other.
        (
            keepass::db::fields::NOTES,
            NewValue::Open("half a note".into()),
        ),
    ] {
        assert_eq!(
            vault.set_typed(id, field, typed).ok(),
            Some(true),
            "{field}"
        );
    }

    assert_eq!(vault.versions(id).len(), 3);
    assert_eq!(vault.rescue(), Rescue::Saved);
    drop(vault);

    let file = open(&database, BUILT_PASSWORD);
    let entry = file.entry(id).expect("the entry is there");
    assert_eq!(
        value(&file, id, keepass::db::fields::USERNAME).as_deref(),
        Some("alic")
    );
    assert_eq!(value(&file, id, "PIN").as_deref(), Some("98"));
    assert!(
        entry
            .field("PIN")
            .is_some_and(|pin| pin.value.open().is_none()),
        "a protected field was written in the open"
    );
    assert_eq!(
        value(&file, id, keepass::db::fields::NOTES).as_deref(),
        Some("half a note")
    );
}

/// Typing that came back to what the field holds is not an edit: no version,
/// no modification time, nothing for the lock to save. The same text under
/// another protection is one, because the protection is part of what the file
/// says about the field.
#[test]
fn typing_that_changes_nothing_writes_nothing() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = vault_for_typing(scratch.path());
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    let modified = vault.entry(id).expect("the entry is there").times.modified;

    for (field, typed) in [
        (
            keepass::db::fields::USERNAME,
            NewValue::Open("alice".into()),
        ),
        (
            "PIN",
            NewValue::Protected(zeroize::Zeroizing::new("1234".to_owned())),
        ),
        (keepass::db::fields::NOTES, NewValue::Open(String::new())),
    ] {
        assert_eq!(
            vault.set_typed(id, field, typed).ok(),
            Some(false),
            "{field}"
        );
    }
    assert!(vault.versions(id).is_empty());
    assert_eq!(
        vault.entry(id).expect("the entry is there").times.modified,
        modified
    );
    assert_eq!(vault.rescue(), Rescue::Nothing);

    assert_eq!(
        vault
            .set_typed(id, "PIN", NewValue::Open("1234".into()))
            .ok(),
        Some(true),
        "a protected value going into the open is not nothing"
    );
}

/// What was typed into something that has gone is refused and changes
/// nothing: a field of the reader's own removed while its text was on the way
/// is not made again, an entry that is not there is not made up, and text a
/// KeePass file cannot hold is refused the way a commit of it would be.
#[test]
fn typing_into_what_has_gone_is_refused_and_changes_nothing() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = vault_for_typing(scratch.path());
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;

    vault.remove_field(id, "PIN").expect("the field is removed");
    let versions = vault.versions(id).len();
    let stranger = open(
        &vault_with_an_entry(scratch.path(), "other.kdbx"),
        BUILT_PASSWORD,
    )
    .tree()
    .entries[0]
        .id;

    assert!(matches!(
        vault.set_typed(
            id,
            "PIN",
            NewValue::Protected(zeroize::Zeroizing::new("5678".to_owned()))
        ),
        Err(VaultError::NoSuchField)
    ));
    assert!(matches!(
        vault.set_typed(
            stranger,
            keepass::db::fields::TITLE,
            NewValue::Open("nobody".into())
        ),
        Err(VaultError::NoSuchEntry)
    ));
    assert!(
        vault
            .set_typed(
                id,
                keepass::db::fields::USERNAME,
                NewValue::Open("null\0byte".into())
            )
            .is_err()
    );

    let entry = vault.entry(id).expect("the entry is there");
    assert!(entry.field("PIN").is_none(), "a removed field came back");
    assert_eq!(entry.username(), "alice");
    assert_eq!(vault.versions(id).len(), versions);
}
