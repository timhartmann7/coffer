//! What a lock does with work the file has not got.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use vault_core::storage::lock::{self, Lock, Outcome};
use vault_core::storage::{self, OnDisk, snapshot, unsaved};
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

    vault
        .remove_field(id, "PIN", false)
        .expect("the field is removed");
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

/// A vault whose lock could not save, and so left its work beside it: the
/// vault holds the other client's change, the copy holds `note`.
fn rescued(directory: &Path, name: &str, note: &str) -> (PathBuf, PathBuf) {
    let database = vault_with_an_entry(directory, name);
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault
        .set_field(id, keepass::db::fields::NOTES, NewValue::Open(note.into()))
        .expect("the field is set");
    somebody_else_writes(&database);
    assert_eq!(vault.rescue(), Rescue::Kept);
    drop(vault);

    let copy = unsaved::beside(&database).expect("a sibling path");
    (database, copy)
}

fn notes_of(path: &Path) -> Option<String> {
    let vault = open(path, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault.entry(id).and_then(|entry| {
        entry
            .field(keepass::db::fields::NOTES)
            .and_then(|field| field.value.open().map(str::to_owned))
    })
}

/// Every lock file and every temporary file in a directory. A move that is
/// over leaves none of either behind.
fn leftovers(directory: &Path) -> Vec<String> {
    std::fs::read_dir(directory)
        .expect("the directory reads")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".lock") || name.ends_with(".coffer-tmp"))
        .collect()
}

/// The name is the whole record of which vault a copy belongs to, so the
/// reading of it is pinned: one suffix off the end, and nothing for a name
/// that is not a copy's or is nothing but the suffix.
#[test]
fn the_vault_a_copy_belongs_to_is_read_off_its_name() {
    let folder = Path::new("/Users/someone/Vault");
    for (copy, vault) in [
        ("vault.kdbx.unsaved.kdbx", Some("vault.kdbx")),
        ("Work 2.kdbx.unsaved.kdbx", Some("Work 2.kdbx")),
        // A copy of a copy belongs to the copy it was taken from.
        (
            "vault.kdbx.unsaved.kdbx.unsaved.kdbx",
            Some("vault.kdbx.unsaved.kdbx"),
        ),
        (".unsaved.kdbx", None),
        ("vault.kdbx", None),
        ("vault.kdbx.1.bak", None),
        ("vault.unsaved.kdbx.bak", None),
    ] {
        assert_eq!(
            unsaved::taken_from(&folder.join(copy)),
            vault.map(|vault| folder.join(vault)),
            "{copy}"
        );
    }
}

/// The vault's file went and its copy is all there is. It goes back into the
/// vault's name whole, owner-only, without a password, and leaves no copy, no
/// note and no temporary file behind it.
#[test]
fn a_copy_whose_vault_has_gone_is_put_back_in_its_place() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (database, copy) = rescued(scratch.path(), "gone.kdbx", "mine");
    std::fs::remove_file(&database).expect("the vault goes");
    let bytes = std::fs::read(&copy).expect("the copy reads");

    unsaved::put_back(&database).expect("the copy goes back");

    assert_eq!(std::fs::read(&database).expect("the vault reads"), bytes);
    assert!(!copy.exists(), "the copy is still beside the vault");
    assert_eq!(notes_of(&database).as_deref(), Some("mine"));
    assert_eq!(leftovers(scratch.path()), Vec::<String>::new());
    if permissions_apply() {
        let mode = std::fs::metadata(&database)
            .expect("the vault is there")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "the vault came back readable by somebody else");
    }
}

/// The one rule a password-free move has to keep: it never goes over a file.
/// A vault that is there, or came back while the reader was looking at the
/// screen, stays exactly as it is, and so does the copy. So does a link that
/// leads nowhere, which is something at the name however gone its target is.
#[test]
fn putting_the_copy_back_never_writes_over_what_is_at_the_vaults_name() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (database, copy) = rescued(scratch.path(), "back.kdbx", "mine");
    let theirs = std::fs::read(&database).expect("the vault reads");
    let ours = std::fs::read(&copy).expect("the copy reads");

    assert!(matches!(
        unsaved::put_back(&database),
        Err(VaultError::DatabaseExists)
    ));
    assert_eq!(std::fs::read(&database).expect("the vault reads"), theirs);
    assert_eq!(std::fs::read(&copy).expect("the copy reads"), ours);
    assert_eq!(leftovers(scratch.path()), Vec::<String>::new());

    std::fs::remove_file(&database).expect("the vault goes");
    let nowhere = scratch.path().join("nowhere.kdbx");
    std::os::unix::fs::symlink(&nowhere, &database).expect("the link is made");
    assert!(matches!(
        unsaved::put_back(&database),
        Err(VaultError::DatabaseExists)
    ));
    assert!(!nowhere.exists(), "the move went through the link");
    assert!(
        database
            .symlink_metadata()
            .is_ok_and(|about| about.file_type().is_symlink()),
        "the link was written over"
    );
    assert_eq!(std::fs::read(&copy).expect("the copy reads"), ours);
}

/// A copy that is not there any more - removed in the Finder while the unlock
/// screen still offered it - puts nothing at the vault's name either: an empty
/// file there would be a vault that does not open.
#[test]
fn putting_back_a_copy_that_is_not_there_leaves_the_vaults_name_empty() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = scratch.path().join("never.kdbx");

    assert!(matches!(
        unsaved::put_back(&database),
        Err(VaultError::DatabaseGone)
    ));
    assert!(
        database.symlink_metadata().is_err(),
        "a file was left behind"
    );
    assert_eq!(leftovers(scratch.path()), Vec::<String>::new());
}

/// Another Coffer with the copy or the vault's name open is told, not moved
/// out from under, and the note it left is left alone.
#[test]
fn a_copy_or_a_vault_somebody_holds_is_not_moved() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (database, copy) = rescued(scratch.path(), "held.kdbx", "mine");
    std::fs::remove_file(&database).expect("the vault goes");

    for held in [&copy, &database] {
        let Ok(Outcome::Taken(lock)) = Lock::acquire(held) else {
            panic!("the test takes the lock beside {}", held.display());
        };
        assert!(
            matches!(unsaved::put_back(&database), Err(VaultError::Locked(_))),
            "{}",
            held.display()
        );
        assert!(
            copy.exists(),
            "the copy moved while {} was held",
            held.display()
        );
        assert!(!database.exists());
        assert!(
            lock::inspect(held).expect("the lock reads").is_some(),
            "somebody else's lock was taken away"
        );
        drop(lock);
    }

    unsaved::put_back(&database).expect("once nobody holds either, the copy goes back");
    assert_eq!(notes_of(&database).as_deref(), Some("mine"));
}

/// A folder that will not take a file will not take the move: the copy stays,
/// and nothing is at the vault's name.
#[test]
fn a_copy_in_a_folder_that_takes_nothing_stays_where_it_is() {
    if !permissions_apply() {
        return;
    }

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (database, copy) = rescued(scratch.path(), "frozen.kdbx", "mine");
    std::fs::remove_file(&database).expect("the vault goes");
    let ours = std::fs::read(&copy).expect("the copy reads");

    let frozen = support::Frozen::over(scratch.path());
    assert!(unsaved::put_back(&database).is_err());
    drop(frozen);

    assert_eq!(std::fs::read(&copy).expect("the copy reads"), ours);
    assert!(!database.exists());
    assert_eq!(leftovers(scratch.path()), Vec::<String>::new());
}

/// "Make this my vault", the whole way. The vault's file as it stood is the
/// newest snapshot, byte for byte; the copy and its note are gone; what was
/// edited inside the copy is in the vault; and every save after this goes to
/// the vault and pushes the chain on from there.
#[test]
fn a_copy_made_the_vault_keeps_the_file_it_replaced_as_the_newest_snapshot() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (database, copy) = rescued(scratch.path(), "promoted.kdbx", "mine");
    let theirs = std::fs::read(&database).expect("the vault reads");
    let copy_lock = copy.with_file_name("promoted.kdbx.unsaved.kdbx.lock");
    let vault_lock = database.with_file_name("promoted.kdbx.lock");

    let mut vault = open(&copy, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault
        .set_field(
            id,
            keepass::db::fields::TITLE,
            NewValue::Open("edited in the copy".into()),
        )
        .expect("the field is set");
    assert!(copy_lock.exists(), "the open copy has no lock beside it");

    vault.promote().expect("the copy becomes the vault");

    assert_eq!(
        vault.path(),
        database.canonicalize().expect("the vault is there")
    );
    assert!(!vault.is_read_only());
    assert!(!copy.exists(), "the copy is still there");
    assert!(!copy_lock.exists(), "the copy's lock outlived it");
    assert!(
        vault_lock.exists(),
        "the vault is open with no lock beside it"
    );
    let first = snapshot::slot(&database, 1).expect("a slot has a name");
    assert_eq!(
        std::fs::read(&first).expect("the snapshot reads"),
        theirs,
        "the file that was replaced is not the newest snapshot"
    );

    vault
        .set_field(
            id,
            keepass::db::fields::USERNAME,
            NewValue::Open("after".into()),
        )
        .expect("the field is set");
    vault
        .save()
        .expect("the next save goes to the vault as its own");
    assert_eq!(
        std::fs::read(snapshot::slot(&database, 2).expect("a slot has a name"))
            .expect("the snapshot reads"),
        theirs,
        "the next save did not push the chain on"
    );
    drop(vault);
    assert_eq!(leftovers(scratch.path()), Vec::<String>::new());

    let reopened = open(&database, BUILT_PASSWORD);
    let entry = reopened.entry(id).expect("the entry is there");
    assert_eq!(
        entry
            .field(keepass::db::fields::TITLE)
            .and_then(|field| field.value.open()),
        Some("edited in the copy")
    );
    assert_eq!(entry.username(), "after");
    drop(reopened);
    assert_eq!(notes_of(&database).as_deref(), Some("mine"));
}

/// The vault's file has gone as well, and the copy with the password in hand
/// goes into its name: nothing is there to snapshot, and nothing is pushed.
#[test]
fn a_copy_made_the_vault_where_the_vault_has_gone_takes_its_name() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (database, copy) = rescued(scratch.path(), "missing.kdbx", "mine");
    std::fs::remove_file(&database).expect("the vault goes");
    let chain = snapshots(&database);

    let mut vault = open(&copy, BUILT_PASSWORD);
    vault.promote().expect("the copy becomes the vault");
    drop(vault);

    assert!(!copy.exists());
    assert_eq!(
        snapshots(&database),
        chain,
        "something was pushed for nothing"
    );
    assert_eq!(notes_of(&database).as_deref(), Some("mine"));
}

/// Hard rule 1 at the moment it is most at risk. A write refused on the way -
/// the vault's file read only, or somebody holding it - leaves both files and
/// the snapshots exactly as they were, and the vault still the copy, still
/// saving into the copy. Once the obstacle goes, the same press goes through.
#[test]
fn a_copy_that_cannot_become_the_vault_leaves_both_files_as_they_were() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (database, copy) = rescued(scratch.path(), "stuck.kdbx", "mine");
    let theirs = std::fs::read(&database).expect("the vault reads");
    let chain = snapshots(&database);
    let vault_lock = database.with_file_name("stuck.kdbx.lock");

    let mut vault = open(&copy, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    let copy_path = vault.path().to_path_buf();

    let Ok(Outcome::Taken(theirs_open)) = Lock::acquire(&database) else {
        panic!("the test takes the vault's lock");
    };
    assert!(matches!(vault.promote(), Err(VaultError::Locked(_))));
    drop(theirs_open);

    if permissions_apply() {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o400))
            .expect("the vault is made read only");
        assert!(vault.promote().is_err(), "a read-only vault was replaced");
        std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o600))
            .expect("the vault is writable again");
    }

    assert_eq!(std::fs::read(&database).expect("the vault reads"), theirs);
    assert!(copy.exists(), "the copy went with the vault untouched");
    assert_eq!(snapshots(&database), chain);
    assert!(
        !vault_lock.exists(),
        "a refused write left a lock beside the vault"
    );
    assert_eq!(vault.path(), copy_path);

    vault
        .set_field(
            id,
            keepass::db::fields::USERNAME,
            NewValue::Open("still the copy".into()),
        )
        .expect("the field is set");
    vault.save().expect("the copy still saves as itself");
    assert_eq!(std::fs::read(&database).expect("the vault reads"), theirs);

    vault
        .promote()
        .expect("with nothing in the way, the copy becomes the vault");
    drop(vault);
    let reopened = open(&database, BUILT_PASSWORD);
    assert_eq!(
        reopened.entry(id).expect("the entry is there").username(),
        "still the copy"
    );
}

/// Only a copy a lock left names a vault. Anything else asked to become one is
/// refused before anything on disk is touched.
#[test]
fn only_a_copy_a_lock_left_can_become_a_vault() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = vault_with_an_entry(scratch.path(), "plain.kdbx");
    let before = std::fs::read(&database).expect("the vault reads");

    let mut vault = open(&database, BUILT_PASSWORD);
    assert!(matches!(vault.promote(), Err(VaultError::NotACopy)));
    assert_eq!(std::fs::read(&database).expect("the vault reads"), before);
    assert!(snapshots(&database).is_empty());
}

/// What the unlock screen says about a vault's file: gone only when nothing at
/// all is at its name, and written at the time the filesystem keeps.
#[test]
fn a_file_is_gone_only_when_nothing_is_at_its_name() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = vault_with_an_entry(scratch.path(), "standing.kdbx");

    let modified = std::fs::metadata(&database)
        .and_then(|about| about.modified())
        .ok();
    assert_eq!(storage::on_disk(&database), OnDisk::Written(modified));

    let link = scratch.path().join("link.kdbx");
    std::os::unix::fs::symlink(scratch.path().join("nowhere.kdbx"), &link)
        .expect("the link is made");
    assert_eq!(storage::on_disk(&link), OnDisk::Written(None));

    std::fs::remove_file(&database).expect("the vault goes");
    assert_eq!(storage::on_disk(&database), OnDisk::Gone);
}
