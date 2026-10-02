//! The backups a save leaves: which one is which after later saves move them,
//! why a vault is read only, and a backup made the vault whole.
//!
//! "Nothing touched" is asked the same way every time: the vault's file holds
//! the bytes it held, the snapshots hold theirs, nothing is kept beside the
//! vault under a name of its own, and the backup is still open where it was.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use keepass::config::DatabaseVersion;
use keepass::db::fields;
use keepass::{Database, DatabaseKey};

use vault_core::kind::Kind;
use vault_core::storage::lock::{self, Lock, Outcome};
use vault_core::storage::{snapshot, unsaved};
use vault_core::{Adopted, EncryptedCopy, NewValue, ReadOnly, VaultError};

use crate::normalise::{canonical, differences};
use crate::support::{
    self, BUILT_PASSWORD, Frozen, RICH, SECRET, built_with, everything, notes, notes_of, open,
    permissions_apply, scratch, snapshots, somebody_else_writes, told, vault_with_an_entry,
};

/// What each save writes into the one entry's notes, oldest first. After all
/// three the vault holds the last, slot 1 the second, slot 2 the first, and
/// slot 3 the vault as it was built, with no notes at all.
const NOTES: [&str; 3] = ["first", "second", "third"];

/// A vault with one entry, saved once for every note in [`NOTES`], and the
/// backups those saves left beside it. The path is the one a vault opened from
/// it reports, links followed.
fn saved_three_times(directory: &Path, name: &str) -> PathBuf {
    let database = vault_with_an_entry(directory, name);
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    for note in NOTES {
        vault
            .set_field(id, fields::NOTES, NewValue::Open(note.into()))
            .expect("the note is written");
        vault.save().expect("the vault saves");
    }
    database.canonicalize().expect("the vault is there")
}

fn slot(database: &Path, index: u32) -> PathBuf {
    snapshot::slot(database, index).expect("a slot has a name")
}

/// Every name in a directory that the vault's file was kept under, which no
/// save removes.
fn kept_aside(directory: &Path) -> Vec<String> {
    std::fs::read_dir(directory)
        .expect("the directory reads")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".replaced-"))
        .collect()
}

/// Every lock file and every temporary file in a directory. A press that is
/// over and a vault that has closed leave none.
fn leftovers(directory: &Path) -> Vec<String> {
    std::fs::read_dir(directory)
        .expect("the directory reads")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".lock") || name.ends_with(".coffer-tmp"))
        .collect()
}

/// Damages the body of the database at `path` and leaves its header alone,
/// which is what a power cut part way through somebody else's write leaves.
/// Answers with the bytes it left.
fn garbled(path: &Path) -> Vec<u8> {
    let mut bytes = std::fs::read(path).expect("the vault reads");
    let length = bytes.len();
    for byte in bytes.iter_mut().skip(length - 64) {
        *byte ^= 0xff;
    }
    std::fs::write(path, &bytes).expect("the vault is damaged");
    bytes
}

fn opens(path: &Path, password: &str) -> bool {
    std::fs::read(path).is_ok_and(|read| {
        Database::parse(&read, DatabaseKey::new().with_password(password)).is_ok()
    })
}

/// The name the vault's file is kept under today, the `number`th time.
fn replaced(database: &Path, number: u32) -> PathBuf {
    let day = chrono::Local::now().date_naive();
    let name = database
        .file_name()
        .expect("the vault has a name")
        .to_string_lossy();
    database.with_file_name(match number {
        1 => format!("{name}.replaced-{day}.kdbx"),
        _ => format!("{name}.replaced-{day}-{number}.kdbx"),
    })
}

/// The name is the whole record of which vault a snapshot belongs to, so the
/// reading of it is pinned: the slot off the end, and nothing for a name that
/// is not a snapshot's, has nothing before the slot or a slot out of range.
#[test]
fn a_snapshot_names_the_vault_it_was_taken_beside() {
    let folder = Path::new("/Users/someone/Vault");
    for (name, vault) in [
        ("vault.kdbx.3.bak", Some("vault.kdbx")),
        ("vault.kdbx.10.bak", Some("vault.kdbx")),
        ("Work 2.kdbx.1.bak", Some("Work 2.kdbx")),
        // The backups of a lock's copy belong to the copy.
        (
            "vault.kdbx.unsaved.kdbx.2.bak",
            Some("vault.kdbx.unsaved.kdbx"),
        ),
        (".1.bak", None),
        ("vault.kdbx.11.bak", None),
        ("vault.kdbx.0.bak", None),
        ("vault.kdbx.x.bak", None),
        ("vault.kdbx", None),
        ("vault.kdbx.unsaved.kdbx", None),
        ("vault.kdbx.replaced-2026-10-01.kdbx", None),
    ] {
        assert_eq!(
            snapshot::taken_from(&folder.join(name)),
            vault.map(|vault| folder.join(vault)),
            "{name}"
        );
    }
}

/// A backup shown at one slot is the same file two saves later, two slots
/// further down, and that is where it is found. The slot it was shown at holds
/// something else by then, which is exactly the file a press by slot would
/// have opened.
#[test]
fn a_snapshot_shown_at_one_slot_is_found_at_the_next_after_a_save() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let listed = snapshot::taken(&database).expect("the chain reads");
    let shown = listed
        .iter()
        .find(|each| each.index == 2)
        .expect("slot 2 is there")
        .clone();
    let bytes = std::fs::read(&shown.path).expect("the backup reads");

    let mut vault = open(&database, BUILT_PASSWORD);
    vault.save().expect("the vault saves");
    vault.save().expect("the vault saves again");

    let found = snapshot::find(&database, &shown)
        .expect("the chain reads")
        .expect("the backup is still in the chain");
    assert_eq!(found.index, 4);
    assert_eq!(std::fs::read(&found.path).expect("the backup reads"), bytes);
    assert_ne!(
        std::fs::read(slot(&database, 2)).expect("the slot reads"),
        bytes,
        "the slot still holds what was shown at it"
    );
}

/// The oldest backup shown, and one save more pushes it off the end. It is
/// not found - and never the backup that moved into its slot.
#[test]
fn a_snapshot_pushed_out_of_the_chain_is_not_found() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    for _ in 0..7 {
        vault.save().expect("the vault saves");
    }
    let listed = snapshot::taken(&database).expect("the chain reads");
    let oldest = listed.last().expect("the chain is full").clone();
    assert_eq!(oldest.index, snapshot::SNAPSHOT_COUNT);

    vault.save().expect("the vault saves");

    assert!(
        slot(&database, snapshot::SNAPSHOT_COUNT).exists(),
        "the slot is empty, so this proves nothing"
    );
    assert_eq!(
        snapshot::find(&database, &oldest).expect("the chain reads"),
        None
    );
}

/// Each reason a vault is not written back is told apart, and only a format
/// refuses a copy somewhere else: a backup and a place are about where the
/// file is, and the format is about what its bytes can be trusted to hold.
#[test]
fn read_only_says_why_and_only_a_format_refuses_a_copy() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let elsewhere = tempfile::tempdir().expect("a second scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");

    let vault = open(&database, BUILT_PASSWORD);
    assert_eq!(vault.read_only(), None);
    assert!(vault.copyable());
    drop(vault);

    let backup = open(&slot(&database, 1), BUILT_PASSWORD);
    assert_eq!(backup.read_only(), Some(ReadOnly::Snapshot));
    assert!(backup.copyable());
    drop(backup);

    let (_kept, kdbx3) = support::scratch("rich-kdbx31.kdbx");
    let mut format = open(&kdbx3, SECRET);
    assert_eq!(format.read_only(), Some(ReadOnly::Kdbx3Attachments));
    assert!(!format.copyable());
    assert!(matches!(
        format
            .encrypt_copy(&elsewhere.path().join("copy.kdbx"))
            .and_then(EncryptedCopy::write),
        Err(VaultError::ReadOnlyKdbx3Attachments)
    ));
    assert!(!elsewhere.path().join("copy.kdbx").exists());

    if permissions_apply() {
        let _frozen = Frozen::over(scratch.path());
        let place = open(&database, BUILT_PASSWORD);
        assert_eq!(place.read_only(), Some(ReadOnly::Place));
        assert!(place.copyable());
    }
}

/// "Use this copy as my vault", the whole way. The vault's file as it stood is
/// the newest snapshot byte for byte, nothing is kept under a name of its own
/// for a file that opens, the backup's lock goes, and the vault holds what the
/// backup held and is the vault from then on: written, and every save after
/// it pushing the chain on from the vault.
#[test]
fn a_backup_made_the_vault_keeps_the_file_it_replaced_as_the_newest_snapshot() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let before = std::fs::read(&database).expect("the vault reads");
    let backup_lock = database.with_file_name("vault.kdbx.2.bak.lock");

    let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
    assert_eq!(vault.read_only(), Some(ReadOnly::Snapshot));
    assert!(
        backup_lock.exists(),
        "the open backup has no lock beside it"
    );

    let adopted = vault
        .adopt(told(&database))
        .expect("the backup becomes the vault");

    assert_eq!(adopted, Adopted::Kept(slot(&database, 1)));
    assert_eq!(
        std::fs::read(slot(&database, 1)).expect("the snapshot reads"),
        before,
        "the file that was replaced is not the newest snapshot"
    );
    assert_eq!(vault.path(), database);
    assert_eq!(vault.read_only(), None);
    assert_eq!(notes(&vault).as_deref(), Some("first"));
    assert!(!backup_lock.exists(), "the backup's lock outlived it");
    assert_eq!(kept_aside(scratch.path()), Vec::<String>::new());

    let id = vault.tree().entries[0].id;
    vault
        .set_field(id, fields::USERNAME, NewValue::Open("after".into()))
        .expect("the field is set");
    vault
        .save()
        .expect("the next save goes to the vault as its own");
    assert_eq!(
        std::fs::read(slot(&database, 2)).expect("the snapshot reads"),
        before,
        "the next save did not push the chain on"
    );
    drop(vault);
    assert_eq!(leftovers(scratch.path()), Vec::<String>::new());

    let reopened = open(&database, BUILT_PASSWORD);
    assert_eq!(notes(&reopened).as_deref(), Some("first"));
    assert_eq!(
        reopened.entry(id).expect("the entry is there").username(),
        "after"
    );
}

/// The vault's file will not open - a power cut part way through somebody
/// else's write. It is kept under a name of its own, dated and ending in
/// `.kdbx`, owner-only, byte for byte; and eleven saves later, when the chain
/// has long pushed it out, it is still there.
#[test]
fn a_backup_made_the_vault_over_a_file_that_will_not_open_keeps_that_file_for_good() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let damaged = garbled(&database);

    let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
    let adopted = vault
        .adopt(told(&database))
        .expect("the backup becomes the vault");

    let kept = replaced(&database, 1);
    assert_eq!(adopted, Adopted::SetAside(kept.clone()));
    assert!(kept.to_string_lossy().ends_with(".kdbx"));
    assert_eq!(std::fs::read(&kept).expect("the file reads"), damaged);
    assert_eq!(
        std::fs::read(slot(&database, 1)).expect("the snapshot reads"),
        damaged,
        "a file kept aside is not also the newest snapshot"
    );
    if permissions_apply() {
        let mode = std::fs::metadata(&kept)
            .expect("the file is there")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "the file was kept readable by somebody else");
    }

    for _ in 0..=snapshot::SNAPSHOT_COUNT {
        vault.save().expect("the vault saves");
    }
    drop(vault);

    assert!(
        snapshots(&database)
            .iter()
            .all(|(_, bytes)| *bytes != damaged),
        "the saves did not push the damaged file out of the chain"
    );
    assert_eq!(std::fs::read(&kept).expect("the file reads"), damaged);
    assert_eq!(notes_of(&database).as_deref(), Some("first"));
}

/// Another client gave the vault's file another password. It does not open
/// with the backup's, so it is kept aside - and it still opens with its own,
/// with everything in it.
#[test]
fn a_vault_file_under_another_password_is_set_aside_and_still_opens_with_it() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let theirs = built_with(scratch.path(), "theirs.kdbx", "their password", |db| {
        db.root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "written elsewhere"));
    });
    std::fs::rename(&theirs, &database).expect("their file takes the vault's name");

    let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
    let adopted = vault
        .adopt(told(&database))
        .expect("the backup becomes the vault");
    drop(vault);

    let Adopted::SetAside(kept) = adopted else {
        panic!("a file under another password was not kept aside: {adopted:?}");
    };
    let theirs = open(&kept, "their password");
    support::entry_titled(&theirs, "written elsewhere");
    drop(theirs);
    assert!(opens(&database, BUILT_PASSWORD));
    assert!(!opens(&database, "their password"));
}

/// Nothing a file at the vault's name can be - empty, or a megabyte of
/// something that is no database at all - is lost on the way.
#[test]
fn a_vault_file_that_is_not_a_database_is_set_aside_not_lost() {
    let noise: Vec<u8> = (0..1u32 << 20)
        .map(|index| u8::try_from(index.wrapping_mul(2_654_435_761) >> 24).unwrap_or_default())
        .collect();

    for (what, bytes) in [
        ("an empty file", Vec::new()),
        ("a megabyte of noise", noise),
    ] {
        let scratch = tempfile::tempdir().expect("a scratch directory");
        let database = saved_three_times(scratch.path(), "vault.kdbx");
        std::fs::write(&database, &bytes).expect("the vault is replaced");

        let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
        let adopted = vault.adopt(told(&database));
        drop(vault);

        let Ok(Adopted::SetAside(kept)) = adopted else {
            panic!("{what} was not kept aside: {adopted:?}");
        };
        assert_eq!(
            std::fs::read(&kept).expect("the file reads"),
            bytes,
            "{what}"
        );
        assert_eq!(notes_of(&database).as_deref(), Some("first"), "{what}");
    }
}

/// A name already taken is never written over: not a file somebody left, not
/// one an earlier press kept, and not a link that leads nowhere, which is
/// neither followed nor replaced. The next number is the one taken.
#[test]
fn a_set_aside_name_never_writes_over_a_file_already_there() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    garbled(&database);
    std::fs::write(replaced(&database, 1), "somebody's").expect("the file is written");
    std::fs::write(replaced(&database, 2), "an earlier press").expect("the file is written");
    let nowhere = scratch.path().join("nowhere");
    std::os::unix::fs::symlink(&nowhere, replaced(&database, 3)).expect("the link is made");

    let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
    let adopted = vault
        .adopt(told(&database))
        .expect("the backup becomes the vault");

    assert_eq!(adopted, Adopted::SetAside(replaced(&database, 4)));
    assert_eq!(
        std::fs::read_to_string(replaced(&database, 1)).expect("the file reads"),
        "somebody's"
    );
    assert_eq!(
        std::fs::read_to_string(replaced(&database, 2)).expect("the file reads"),
        "an earlier press"
    );
    assert!(
        replaced(&database, 3)
            .symlink_metadata()
            .is_ok_and(|about| about.file_type().is_symlink()),
        "the link was written over"
    );
    assert!(!nowhere.exists(), "the link was followed");
}

/// A vault whose own name fits and leaves no room for the longer name it
/// would be kept under. The press is refused before anything is written: the
/// vault's file, its snapshots and the backup are as they were, and the backup
/// is still open and still read only.
#[test]
fn a_vault_whose_name_leaves_no_room_for_a_set_aside_name_is_refused_unchanged() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let name = format!("{}.kdbx", "v".repeat(230));
    let database = saved_three_times(scratch.path(), &name);
    let damaged = garbled(&database);
    let before = snapshots(&database);

    let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
    let backup = vault.path().to_path_buf();

    let refused = vault.adopt(told(&database));

    assert!(matches!(refused, Err(VaultError::Io(_))), "{refused:?}");
    assert_eq!(std::fs::read(&database).expect("the vault reads"), damaged);
    assert_eq!(snapshots(&database), before);
    assert_eq!(kept_aside(scratch.path()), Vec::<String>::new());
    assert_eq!(vault.path(), backup);
    assert_eq!(vault.read_only(), Some(ReadOnly::Snapshot));
    assert!(
        !database.with_file_name(format!("{name}.lock")).exists(),
        "a refused press left a lock beside the vault"
    );
}

/// The vault's file has gone, and the backup takes its name. Nothing was there
/// to snapshot, so nothing in the chain moves.
#[test]
fn a_backup_made_the_vault_where_the_vault_has_gone_takes_its_name() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    std::fs::remove_file(&database).expect("the vault goes");
    let before = snapshots(&database);

    let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
    let adopted = vault
        .adopt(told(&database))
        .expect("the backup becomes the vault");
    drop(vault);

    assert_eq!(adopted, Adopted::Empty);
    assert_eq!(
        snapshots(&database),
        before,
        "something was pushed for nothing"
    );
    assert_eq!(notes_of(&database).as_deref(), Some("first"));
    assert_eq!(leftovers(scratch.path()), Vec::<String>::new());
}

/// The press goes over the file the reader was told about and no other.
/// Another client's save since, a vault that went, one that came back, and a
/// reader who was never told are each refused with nothing touched. Told
/// again, it goes through, and what the other client wrote is the newest
/// snapshot.
#[test]
fn a_backup_is_not_made_the_vault_over_a_file_the_reader_was_not_shown() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
    let backup = vault.path().to_path_buf();

    let before = told(&database);
    somebody_else_writes(&database);
    let theirs = std::fs::read(&database).expect("the vault reads");
    let chain_before = snapshots(&database);

    let gone = told(&database);
    let away = scratch.path().join("away");
    std::fs::rename(&database, &away).expect("the vault goes");
    let missing = told(&database);
    let refused = vault.adopt(gone);
    std::fs::rename(&away, &database).expect("the vault comes back");

    for (said, answer) in [
        ("another client's save", vault.adopt(before)),
        ("a vault that went", refused),
        ("a vault that came back", vault.adopt(missing)),
        ("nothing said at all", vault.adopt(None)),
    ] {
        assert!(
            matches!(answer, Err(VaultError::VaultFileChanged)),
            "{said}: {answer:?}"
        );
        assert_eq!(
            std::fs::read(&database).expect("the vault reads"),
            theirs,
            "{said}"
        );
        assert_eq!(snapshots(&database), chain_before, "{said}");
        assert_eq!(kept_aside(scratch.path()), Vec::<String>::new(), "{said}");
        assert_eq!(vault.path(), backup, "{said}");
        assert_eq!(vault.read_only(), Some(ReadOnly::Snapshot), "{said}");
    }

    let adopted = vault
        .adopt(told(&database))
        .expect("told again, the backup becomes the vault");
    assert_eq!(adopted, Adopted::Kept(slot(&database, 1)));
    assert_eq!(
        std::fs::read(slot(&database, 1)).expect("the snapshot reads"),
        theirs
    );
}

/// Hard rule 1 at the moment it is most at risk. A press refused on the way -
/// somebody holding the vault, a damaged vault file its owner made read only -
/// leaves the vault's file, its snapshots and the backup as they were, and
/// takes back the name it had just given the damaged file. Once the obstacle
/// goes, the same press goes through.
#[test]
fn a_backup_that_cannot_become_the_vault_leaves_everything_as_it_was() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let damaged = garbled(&database);
    let before = snapshots(&database);
    let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
    let backup = vault.path().to_path_buf();

    let Ok(Outcome::Taken(held)) = Lock::acquire(&database) else {
        panic!("the test takes the vault's lock");
    };
    assert!(matches!(
        vault.adopt(told(&database)),
        Err(VaultError::Locked(_))
    ));
    drop(held);

    if permissions_apply() {
        std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o400))
            .expect("the vault is made read only");
        let refused = vault.adopt(told(&database));
        assert!(refused.is_err(), "a read-only vault was replaced");
        assert_eq!(
            std::fs::metadata(&database)
                .expect("the vault is there")
                .permissions()
                .mode()
                & 0o777,
            0o400,
            "keeping the file aside made the vault writable"
        );
        std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o600))
            .expect("the vault is writable again");
    }

    assert_eq!(std::fs::read(&database).expect("the vault reads"), damaged);
    assert_eq!(snapshots(&database), before);
    assert_eq!(
        kept_aside(scratch.path()),
        Vec::<String>::new(),
        "a refused press left a name behind"
    );
    assert!(lock::inspect(&database).expect("the lock reads").is_none());
    assert_eq!(vault.path(), backup);
    assert_eq!(vault.read_only(), Some(ReadOnly::Snapshot));

    let adopted = vault
        .adopt(told(&database))
        .expect("with nothing in the way, the backup becomes the vault");
    assert_eq!(adopted, Adopted::SetAside(replaced(&database, 1)));
    drop(vault);
    assert_eq!(notes_of(&database).as_deref(), Some("first"));
}

/// A lock left beside the vault by a Coffer that is no longer running is no
/// reason to refuse: it describes a process that has gone.
#[test]
fn a_backup_is_made_the_vault_over_a_stale_lock_from_a_dead_process() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");

    // A process id that has certainly gone: a child that has already been
    // reaped. The host is the one Coffer itself writes into a lock.
    let mut child = std::process::Command::new("sh")
        .args(["-c", "exit 0"])
        .spawn()
        .expect("the child runs");
    let dead = child.id();
    child.wait().expect("the child is reaped");
    let host = {
        let _ours = Lock::acquire(&database).expect("the lock is taken");
        lock::inspect(&database)
            .expect("the lock reads")
            .expect("a lock is there")
            .host
    };
    std::fs::write(
        database.with_file_name("vault.kdbx.lock"),
        format!(
            "[Lock]\nTime=2020-01-01T00:00:00Z\nUserName=someone\nMachine={host}\nPID={dead}\n"
        ),
    )
    .expect("the stale lock is written");

    let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
    let adopted = vault
        .adopt(told(&database))
        .expect("a dead process's lock does not stand in the way");
    assert_eq!(adopted, Adopted::Kept(slot(&database, 1)));
    drop(vault);

    assert_eq!(notes_of(&database).as_deref(), Some("first"));
    assert_eq!(leftovers(scratch.path()), Vec::<String>::new());
}

/// The oldest backup is the one the write's own rotation pushes off the end of
/// the chain. What it held is in memory, and that is what goes over the vault.
#[test]
fn the_oldest_backup_made_the_vault_outlives_the_save_that_pushes_it_out() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    let id = vault.tree().entries[0].id;
    vault
        .set_field(id, fields::NOTES, NewValue::Open("oldest".into()))
        .expect("the note is written");
    vault.save().expect("the vault saves");
    for _ in 0..snapshot::SNAPSHOT_COUNT - 1 {
        vault.save().expect("the vault saves");
    }
    drop(vault);
    let oldest = slot(&database, snapshot::SNAPSHOT_COUNT);
    assert_eq!(notes_of(&oldest).as_deref(), Some("third"));

    let mut vault = open(&oldest, BUILT_PASSWORD);
    let adopted = vault
        .adopt(told(&database))
        .expect("the oldest backup becomes the vault");
    assert_eq!(adopted, Adopted::Kept(slot(&database, 1)));
    drop(vault);

    assert_eq!(notes_of(&database).as_deref(), Some("third"));
}

/// Only a file at one of Coffer's snapshot names becomes a vault this way. A
/// vault, the copy a lock left and a file at a snapshot's name in a format
/// Coffer does not write are each refused before anything is touched - the
/// last for its format, which is what the reader has to hear.
#[test]
fn only_a_backup_can_be_made_the_vault_this_way() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let bytes = std::fs::read(&database).expect("the vault reads");
    let before = snapshots(&database);

    let mut plain = open(&database, BUILT_PASSWORD);
    assert!(matches!(
        plain.adopt(told(&database)),
        Err(VaultError::NotASnapshot)
    ));
    drop(plain);

    let copy = unsaved::beside(&database).expect("a sibling path");
    std::fs::copy(&database, &copy).expect("the copy is left beside it");
    let mut copied = open(&copy, BUILT_PASSWORD);
    assert!(matches!(
        copied.adopt(told(&database)),
        Err(VaultError::NotASnapshot)
    ));
    drop(copied);

    let foreign = slot(&database, 5);
    std::fs::copy(support::fixture("rich-kdbx31.kdbx"), &foreign).expect("the file is copied");
    let mut format = open(&foreign, SECRET);
    assert!(matches!(
        format.adopt(told(&database)),
        Err(VaultError::ReadOnlyKdbx3Attachments)
    ));
    drop(format);

    assert_eq!(std::fs::read(&database).expect("the vault reads"), bytes);
    let mut after = snapshots(&database);
    after.retain(|(index, _)| *index != 5);
    assert_eq!(after, before);
    assert_eq!(kept_aside(scratch.path()), Vec::<String>::new());
}

/// Hard rule 1. A backup KeePassXC wrote, made the vault by Coffer, comes out
/// with every field, every value and every file it held - read back by Coffer,
/// and exported by KeePassXC and compared with an export of the backup.
#[test]
fn a_backup_made_the_vault_loses_no_field() {
    let (_directory, database) = scratch(RICH);
    let database = database.canonicalize().expect("the vault is there");
    let mut vault = open(&database, SECRET);
    vault.save().expect("the vault saves");
    vault.save().expect("the vault saves again");
    drop(vault);

    let backup = slot(&database, 2);
    let tool = support::keepassxc_cli();
    let exported = tool
        .as_deref()
        .map(|tool| support::export(tool, &backup, SECRET, None));

    let mut vault = open(&backup, SECRET);
    let held = everything(&vault);
    let adopted = vault
        .adopt(told(&database))
        .expect("the backup becomes the vault");
    assert_eq!(adopted, Adopted::Kept(slot(&database, 1)));
    drop(vault);

    let reopened = open(&database, SECRET);
    assert!(
        everything(&reopened) == held,
        "the backup lost or altered a field on its way to being the vault"
    );
    drop(reopened);

    if let (Some(tool), Some(exported)) = (tool, exported) {
        let after = support::export(&tool, &database, SECRET, None);
        let lost = differences(&canonical(&exported, false), &canonical(&after, false));
        assert!(
            lost.is_empty(),
            "the backup made the vault lost or altered a field:\n  {}",
            lost.join("\n  ")
        );
    }
}

/// A backup a save took of a KDBX 3.1 file is that file, in its format. Made
/// the vault, it is written back as KDBX 4.1, the only format Coffer writes,
/// with what it held.
#[test]
fn a_backup_of_an_older_format_becomes_a_kdbx41_vault() {
    let (_directory, database) = scratch("empty-kdbx31.kdbx");
    let database = database.canonicalize().expect("the vault is there");
    let mut vault = open(&database, SECRET);
    let root = vault.tree().id;
    let id = vault
        .create_entry(root, Kind::Login)
        .expect("the entry is made");
    vault
        .set_field(id, fields::TITLE, NewValue::Open("added since".into()))
        .expect("the title is set");
    vault.save().expect("the vault saves");
    drop(vault);

    let parsed = |path: &Path| {
        let read = std::fs::read(path).expect("the file reads");
        Database::parse(&read, DatabaseKey::new().with_password(SECRET)).expect("the file opens")
    };
    let backup = slot(&database, 1);
    assert_eq!(parsed(&backup).config.version, DatabaseVersion::KDB3(1));

    let mut vault = open(&backup, SECRET);
    assert_eq!(vault.read_only(), Some(ReadOnly::Snapshot));
    let held = everything(&vault);
    let adopted = vault
        .adopt(told(&database))
        .expect("the backup becomes the vault");
    drop(vault);

    assert_eq!(adopted, Adopted::Kept(slot(&database, 1)));
    assert_eq!(parsed(&database).config.version, DatabaseVersion::KDB4(1));
    let reopened = open(&database, SECRET);
    assert!(everything(&reopened) == held);
    assert!(support::all_entries(&reopened).is_empty());
}

/// A backup opens with the password the vault had when it was taken, which
/// after a change is not the vault's. Made the vault, it brings that password
/// back: the vault opens with the old one only. The file it replaced, under
/// the new one, does not open with the old, so it is kept aside, and it still
/// opens with the new one.
#[test]
fn a_backup_taken_before_a_new_password_makes_the_vault_open_with_the_old_one() {
    const NEW: &str = "a password chosen after the backup";

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let mut vault = open(&database, BUILT_PASSWORD);
    vault
        .change_master_password(
            BUILT_PASSWORD.as_bytes(),
            zeroize::Zeroizing::new(NEW.as_bytes().to_vec()),
        )
        .expect("the password changes");
    drop(vault);

    let mut backup = open(&slot(&database, 1), BUILT_PASSWORD);
    let adopted = backup
        .adopt(told(&database))
        .expect("the backup becomes the vault");
    drop(backup);

    assert!(opens(&database, BUILT_PASSWORD));
    assert!(
        !opens(&database, NEW),
        "the new password still opens the vault"
    );
    let Adopted::SetAside(kept) = adopted else {
        panic!("the file under the new password was not kept aside: {adopted:?}");
    };
    assert!(opens(&kept, NEW));
    assert_eq!(notes_of(&database).as_deref(), Some("third"));
}

/// "Save a copy as…" from a backup opens its panel in the vault's own folder,
/// where every file that must not go ends in `.kdbx`. Aimed at each of them -
/// the vault, the copy a lock left, a file a backup made the vault kept
/// aside, a link that leads nowhere - it is refused with all of them as they
/// were, and so is a snapshot's name that nothing holds yet. Aimed at a name
/// nothing holds, it is written whole.
#[test]
fn a_copy_is_never_written_over_a_file_that_is_there() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let copy = unsaved::beside(&database).expect("a sibling path");
    std::fs::copy(&database, &copy).expect("a lock's copy is left beside it");
    let aside = replaced(&database, 1);
    std::fs::write(&aside, "kept aside").expect("the file is written");
    let nowhere = scratch.path().join("nowhere.kdbx");
    let link = scratch.path().join("link.kdbx");
    std::os::unix::fs::symlink(&nowhere, &link).expect("the link is made");
    let unheld = slot(&database, 9);

    let vault_bytes = std::fs::read(&database).expect("the vault reads");
    let copy_bytes = std::fs::read(&copy).expect("the copy reads");
    let before = snapshots(&database);

    let mut backup = open(&slot(&database, 2), BUILT_PASSWORD);
    for (what, target, wanted) in [
        ("the vault", &database, VaultError::DatabaseExists),
        ("the copy a lock left", &copy, VaultError::ReservedName),
        ("a file kept aside", &aside, VaultError::DatabaseExists),
        (
            "a link that leads nowhere",
            &link,
            VaultError::DatabaseExists,
        ),
        ("a snapshot's name", &unheld, VaultError::ReservedName),
    ] {
        let answer = backup.encrypt_copy(target).and_then(EncryptedCopy::write);
        assert!(
            matches!(&answer, Err(error) if std::mem::discriminant(error) == std::mem::discriminant(&wanted)),
            "{what}: {answer:?}"
        );
    }

    assert_eq!(
        std::fs::read(&database).expect("the vault reads"),
        vault_bytes
    );
    assert_eq!(std::fs::read(&copy).expect("the copy reads"), copy_bytes);
    assert_eq!(
        std::fs::read_to_string(&aside).expect("the file reads"),
        "kept aside"
    );
    assert!(
        link.symlink_metadata()
            .is_ok_and(|about| about.file_type().is_symlink()),
        "the link was written over"
    );
    assert!(!nowhere.exists(), "the link was followed");
    assert!(!unheld.exists(), "a copy took a snapshot's name");
    assert_eq!(snapshots(&database), before);

    let elsewhere = scratch.path().join("the backup.kdbx");
    backup
        .encrypt_copy(&elsewhere)
        .and_then(EncryptedCopy::write)
        .expect("a name nothing holds takes the copy");
    drop(backup);
    assert_eq!(leftovers(scratch.path()), Vec::<String>::new());
    assert_eq!(notes_of(&elsewhere).as_deref(), Some("first"));
    assert_eq!(open(&elsewhere, BUILT_PASSWORD).read_only(), None);
}

/// The backups of the copy a lock left are the copy's own. Made the vault,
/// one would go over the copy, which holds the only version of work its vault
/// has not got - so it is refused with the copy as it was.
#[test]
fn a_backup_of_a_lock_s_copy_never_goes_over_the_copy() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let copy = unsaved::beside(&database).expect("a sibling path");
    std::fs::copy(&database, &copy).expect("a lock's copy is left beside it");
    let mut opened = open(&copy, BUILT_PASSWORD);
    opened.save().expect("the copy saves");
    opened.save().expect("the copy saves again");
    drop(opened);
    let copy_bytes = std::fs::read(&copy).expect("the copy reads");
    let before = snapshots(&copy);

    let mut backup = open(&slot(&copy, 1), BUILT_PASSWORD);
    assert_eq!(backup.read_only(), Some(ReadOnly::Snapshot));
    let answer = backup.adopt(told(&copy));

    assert!(
        matches!(answer, Err(VaultError::NotASnapshot)),
        "{answer:?}"
    );
    assert_eq!(std::fs::read(&copy).expect("the copy reads"), copy_bytes);
    assert_eq!(snapshots(&copy), before);
    assert_eq!(kept_aside(scratch.path()), Vec::<String>::new());
    assert_eq!(backup.path(), slot(&copy, 1));
}

/// A backup opened from a folder that takes no file opens as a backup - the
/// name is asked about before the place - and making it the vault is refused
/// for the place, with nothing touched and the backup still open.
#[test]
fn a_backup_in_a_folder_that_takes_no_file_is_refused_for_the_place() {
    if !permissions_apply() {
        return;
    }
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let damaged = garbled(&database);
    let before = snapshots(&database);

    let _frozen = Frozen::over(scratch.path());
    let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
    assert_eq!(vault.read_only(), Some(ReadOnly::Snapshot));
    let backup = vault.path().to_path_buf();

    let answer = vault.adopt(told(&database));

    assert!(
        matches!(answer, Err(VaultError::ReadOnlyPlace)),
        "{answer:?}"
    );
    assert_eq!(std::fs::read(&database).expect("the vault reads"), damaged);
    assert_eq!(snapshots(&database), before);
    assert_eq!(kept_aside(scratch.path()), Vec::<String>::new());
    assert_eq!(vault.path(), backup);
    assert_eq!(vault.read_only(), Some(ReadOnly::Snapshot));
}

/// A file at the vault's name whose header asks for four gigabytes of Argon2
/// memory. Whether it opens with the backup's key is answered by the header's
/// own check, before anything is derived, so the press does not wait on it:
/// the file is kept aside whole, and the backup becomes the vault.
#[test]
fn a_vault_file_with_absurd_key_derivation_is_set_aside_without_deriving_it() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let argon2 = built_with(scratch.path(), "argon2.kdbx", BUILT_PASSWORD, |db| {
        db.config.kdf_config = keepass::config::KdfConfig::Argon2id {
            iterations: 1,
            memory: 8 * 1024,
            parallelism: 1,
            version: argon2::Version::Version13,
        };
        db.root_mut().add_entry();
    });
    let mut forged = std::fs::read(&argon2).expect("the database reads");
    assert!(
        support::forge_kdf_value(&mut forged, b'M', 4 * 1024 * 1024 * 1024),
        "the header should carry an Argon2 memory request to forge"
    );
    std::fs::remove_file(&argon2).expect("the scratch file goes");
    std::fs::write(&database, &forged).expect("the forged file takes the vault's name");

    let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
    let started = std::time::Instant::now();
    let adopted = vault
        .adopt(told(&database))
        .expect("the backup becomes the vault");
    let took = started.elapsed();
    drop(vault);

    assert!(
        took < std::time::Duration::from_secs(10),
        "the forged key derivation was run: {took:?}"
    );
    assert_eq!(adopted, Adopted::SetAside(replaced(&database, 1)));
    assert_eq!(
        std::fs::read(replaced(&database, 1)).expect("the file reads"),
        forged
    );
    assert_eq!(notes_of(&database).as_deref(), Some("first"));
}

/// A link somebody left at the vault's name, to a damaged file outside the
/// folder. The link is what is kept aside, as a link; the file it leads to
/// keeps every byte and its mode, because nothing Coffer does reaches through
/// it; and the vault's name holds a file of its own afterwards.
#[test]
fn a_backup_made_the_vault_over_a_name_that_is_a_link_touches_nothing_it_points_at() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let elsewhere = tempfile::tempdir().expect("a second scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    let target = elsewhere.path().join("vault.kdbx");
    std::fs::rename(&database, &target).expect("the vault's file moves away");
    let damaged = garbled(&target);
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644))
        .expect("the file is readable by others");
    std::os::unix::fs::symlink(&target, &database).expect("the link is made");

    let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
    let adopted = vault
        .adopt(told(&database))
        .expect("the backup becomes the vault");
    drop(vault);

    let kept = replaced(&database, 1);
    assert_eq!(adopted, Adopted::SetAside(kept.clone()));
    assert_eq!(
        std::fs::read_link(&kept).expect("the link was kept as a link"),
        target
    );
    assert_eq!(std::fs::read(&target).expect("the file reads"), damaged);
    assert_eq!(
        std::fs::metadata(&target)
            .expect("the file is there")
            .permissions()
            .mode()
            & 0o777,
        0o644,
        "something reached through the link and changed the file it leads to"
    );
    assert!(
        database
            .symlink_metadata()
            .is_ok_and(|about| about.file_type().is_file()),
        "the vault's name is still a link"
    );
    assert_eq!(notes_of(&database).as_deref(), Some("first"));
}

/// A link at the vault's name that leads nowhere - a vault on a disk that is
/// not plugged in. Nothing is there to snapshot, so the chain does not move;
/// the link is kept aside as it is, still leading where it led; and the
/// backup takes the vault's name. Pressed again it would not be refused for a
/// file that never changed.
#[test]
fn a_backup_made_the_vault_over_a_link_that_leads_nowhere_keeps_the_link() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = saved_three_times(scratch.path(), "vault.kdbx");
    std::fs::remove_file(&database).expect("the vault goes");
    let nowhere = Path::new("/Volumes/a stick that is not plugged in/vault.kdbx");
    std::os::unix::fs::symlink(nowhere, &database).expect("the link is made");
    let before = snapshots(&database);

    let mut vault = open(&slot(&database, 2), BUILT_PASSWORD);
    let adopted = vault
        .adopt(told(&database))
        .expect("a link that leads nowhere is no reason to refuse");
    drop(vault);

    let kept = replaced(&database, 1);
    assert_eq!(adopted, Adopted::SetAside(kept.clone()));
    assert_eq!(
        std::fs::read_link(&kept).expect("the link was kept as a link"),
        nowhere
    );
    assert_eq!(
        snapshots(&database),
        before,
        "something was pushed for nothing"
    );
    assert_eq!(notes_of(&database).as_deref(), Some("first"));
    assert_eq!(leftovers(scratch.path()), Vec::<String>::new());
}
