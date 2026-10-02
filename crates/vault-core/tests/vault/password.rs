//! Giving a vault a new master password, and the snapshots that still open
//! with the old one.
//!
//! "Nothing written" is asked the same four ways every time: the file holds
//! the bytes it held, no snapshot was taken, the vault counted no change, and
//! the old password still opens the file.

use std::path::{Path, PathBuf};

use keepass::config::{DatabaseVersion, KdfConfig};
use keepass::{Database, DatabaseKey};
use zeroize::Zeroizing;

use vault_core::kdf::Work;
use vault_core::kind::Kind;
use vault_core::model::fields;
use vault_core::storage::watch::Change;
use vault_core::storage::{Seen, snapshot, unsaved};
use vault_core::{LockPolicy, NewValue, Recipe, Vault, VaultError};

use crate::normalise::{canonical, differences};
use crate::support::{
    self, BUILT_PASSWORD, Frozen, RICH, SECRET, built, built_with, everything, fixture, open,
    permissions_apply, scratch,
};

const NEW: &str = "a password nobody saw typed";

fn bytes(password: &str) -> Zeroizing<Vec<u8>> {
    Zeroizing::new(password.as_bytes().to_vec())
}

/// A database nobody has opened, under the password every `built` one has.
fn vault(directory: &Path) -> PathBuf {
    built(directory, "vault.kdbx", |_| {})
}

/// Whether the file opens with `password`, and with `key_file` beside it when
/// there is one. Asked of the file through the library rather than through a
/// vault, so that the vault a test holds open can stay open while it is asked.
fn opens_with(path: &Path, password: Option<&str>, key_file: Option<&str>) -> bool {
    let read = std::fs::read(path).expect("the file reads");
    let mut key = DatabaseKey::new();
    if let Some(password) = password {
        key = key.with_password(password);
    }
    if let Some(name) = key_file {
        let file = std::fs::read(fixture(name)).expect("the key file reads");
        key = key
            .with_keyfile(&mut file.as_slice())
            .expect("the key file is taken");
    }
    Database::parse(&read, key).is_ok()
}

fn opens(path: &Path, password: &str) -> bool {
    opens_with(path, Some(password), None)
}

fn snapshots(database: &Path) -> Vec<snapshot::Taken> {
    snapshot::taken(database).expect("the snapshots are listed")
}

/// Removes the old snapshots, expecting every one of them to go, and answers
/// with how many went.
fn removed(vault: &mut Vault) -> usize {
    let removal = vault.remove_old_snapshots();
    assert!(removal.refused.is_none(), "{:?}", removal.refused);
    assert_eq!(removal.left, 0, "an old snapshot was left");
    removal.gone
}

/// How a vault and its file stood before something was asked of it.
struct Before {
    file: Vec<u8>,
    snapshots: usize,
    edits: u64,
}

impl Before {
    fn of(vault: &Vault) -> Before {
        Before {
            file: std::fs::read(vault.path()).expect("the file reads"),
            snapshots: snapshots(vault.path()).len(),
            edits: vault.edits(),
        }
    }

    fn nothing_written(&self, vault: &Vault, old: &str, what: &str) {
        assert_eq!(
            std::fs::read(vault.path()).expect("the file reads"),
            self.file,
            "{what}: the file was written"
        );
        assert_eq!(
            snapshots(vault.path()).len(),
            self.snapshots,
            "{what}: a snapshot was taken"
        );
        assert_eq!(
            vault.edits(),
            self.edits,
            "{what}: the vault counted a change"
        );
        assert!(
            opens(vault.path(), old),
            "{what}: the old password no longer opens the file"
        );
    }
}

#[test]
fn a_changed_password_opens_the_vault_and_the_old_one_does_not() {
    let (_scratch, database) = scratch(RICH);
    let mut vault = open(&database, SECRET);
    let before = everything(&vault);

    vault
        .change_master_password(SECRET.as_bytes(), bytes(NEW))
        .expect("the password is changed");
    drop(vault);

    let refused = Vault::open(&database, support::password(SECRET), LockPolicy::Respect)
        .expect_err("the old password still opens the vault");
    assert!(matches!(refused, VaultError::WrongCredentials));

    let again = open(&database, NEW);
    assert!(
        everything(&again) == before,
        "the vault lost or changed something on the way to a new password"
    );
}

/// The one check between a window left unlocked and a vault somebody else has
/// given a password of their own. Anything but the password itself is refused,
/// however close.
#[test]
fn the_current_password_has_to_be_the_one_the_vault_holds() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let composed = "café au lait";
    let database = built_with(directory.path(), "vault.kdbx", composed, |_| {});
    let mut vault = open(&database, composed);
    let before = Before::of(&vault);

    let mut one_off = composed.as_bytes().to_vec();
    if let Some(last) = one_off.last_mut() {
        *last ^= 1;
    }
    let mut one_more = composed.as_bytes().to_vec();
    one_more.push(b'!');
    let mut long = composed.as_bytes().to_vec();
    long.resize(10 * 1024 * 1024, b'x');
    let decomposed = "cafe\u{301} au lait";

    for (what, typed) in [
        ("nothing", Vec::new()),
        ("a prefix", composed.as_bytes()[..4].to_vec()),
        ("the password and one byte more", one_more),
        ("the same length with one byte off", one_off),
        (
            "the same letters decomposed",
            decomposed.as_bytes().to_vec(),
        ),
        ("ten megabytes starting with it", long),
    ] {
        let refused = vault
            .change_master_password(&typed, bytes(NEW))
            .expect_err("a wrong current password changed the vault's");
        assert!(
            matches!(refused, VaultError::NotTheCurrentPassword),
            "{what}: {refused}"
        );
        before.nothing_written(&vault, composed, what);
    }
}

#[test]
fn an_empty_new_password_is_refused_and_nothing_is_written() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let mut vault = open(&vault(directory.path()), BUILT_PASSWORD);
    let before = Before::of(&vault);

    let refused = vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), Zeroizing::new(Vec::new()))
        .expect_err("a vault was given no password at all");
    assert!(matches!(refused, VaultError::EmptyMasterPassword));
    before.nothing_written(&vault, BUILT_PASSWORD, "an empty password");
}

/// KeePass hashes a password as UTF-8 text, so bytes that are not text could
/// never be typed again. Refused before the write, not by the key half way
/// through it.
#[test]
fn a_new_password_that_is_not_text_is_refused_before_anything_is_written() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let mut vault = open(&vault(directory.path()), BUILT_PASSWORD);
    let before = Before::of(&vault);

    for (what, typed) in [
        ("two bytes no text starts with", vec![0xff, 0xfe]),
        (
            "a lone surrogate, as CESU-8 writes one",
            vec![b'a', 0xed, 0xa0, 0x80],
        ),
    ] {
        let refused = vault
            .change_master_password(BUILT_PASSWORD.as_bytes(), Zeroizing::new(typed))
            .expect_err("bytes that are not text became the password");
        assert!(matches!(refused, VaultError::PasswordNotUtf8), "{what}");
        before.nothing_written(&vault, BUILT_PASSWORD, what);
    }
}

#[test]
fn the_password_the_vault_already_has_is_not_a_change() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let mut vault = open(&vault(directory.path()), BUILT_PASSWORD);
    let before = Before::of(&vault);

    let refused = vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(BUILT_PASSWORD))
        .expect_err("the same password was written as a new one");
    assert!(matches!(refused, VaultError::SamePassword));
    before.nothing_written(&vault, BUILT_PASSWORD, "the same password");
}

#[test]
fn a_snapshot_or_a_place_that_takes_no_write_refuses_a_new_password() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    open(&database, BUILT_PASSWORD)
        .save()
        .expect("the vault saves");

    let first = snapshot::slot(&database, 1).expect("the slot has a path");
    let mut kept = open(&first, BUILT_PASSWORD);
    let before = Before::of(&kept);
    let refused = kept
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect_err("a snapshot was given a new password");
    assert!(matches!(refused, VaultError::ReadOnlySnapshot));
    before.nothing_written(&kept, BUILT_PASSWORD, "a snapshot");
    drop(kept);

    // Written back, a KDBX 3 file would be KDBX 4 under the new key, with its
    // attachments collapsed onto one by the reader on the way in.
    let (_kdbx3, attached) = scratch("rich-kdbx31.kdbx");
    let mut old_format = open(&attached, SECRET);
    let before = Before::of(&old_format);
    let refused = old_format
        .change_master_password(SECRET.as_bytes(), bytes(NEW))
        .expect_err("a KDBX 3 file with attachments was given a new password");
    assert!(matches!(refused, VaultError::ReadOnlyKdbx3Attachments));
    before.nothing_written(&old_format, SECRET, "a KDBX 3 file with attachments");
    drop(old_format);

    if !permissions_apply() {
        return;
    }
    let elsewhere = tempfile::tempdir().expect("a scratch directory");
    let database = vault(elsewhere.path());
    let _frozen = Frozen::over(elsewhere.path());
    let mut read_only = open(&database, BUILT_PASSWORD);
    let before = Before::of(&read_only);
    let refused = read_only
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect_err("a vault on a medium that takes no write was given a new password");
    assert!(matches!(refused, VaultError::ReadOnlyPlace));
    before.nothing_written(&read_only, BUILT_PASSWORD, "a read-only place");
}

/// The copy a lock left opens and saves like the vault, and is not it. A new
/// password there would be the copy's alone - the vault and every snapshot of
/// it would go on opening with the old one, and the count could only be the
/// copy's own snapshots - so it is refused before anything else is asked.
/// Made the vault, the copy takes a change that reaches the vault's whole
/// chain.
#[test]
fn a_lock_s_copy_is_given_no_new_password_until_it_is_made_the_vault() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    saved(&database, BUILT_PASSWORD, 3);
    let copy = unsaved::beside(&database).expect("the copy has a path");
    std::fs::copy(&database, &copy).expect("the copy is left beside it");
    let vault_file = std::fs::read(&database).expect("the vault reads");

    let mut opened = open(&copy, BUILT_PASSWORD);
    assert_eq!(
        opened.read_only(),
        None,
        "a lock's copy could not be written"
    );
    let before = Before::of(&opened);
    for (what, current) in [
        ("the copy's password", BUILT_PASSWORD.as_bytes()),
        ("a wrong one", &b"not it"[..]),
    ] {
        let refused = opened
            .change_master_password(current, bytes(NEW))
            .expect_err("a lock's copy was given a new password");
        assert!(
            matches!(refused, VaultError::PasswordOfACopy),
            "{what}: {refused}"
        );
        before.nothing_written(&opened, BUILT_PASSWORD, what);
    }
    assert_eq!(
        std::fs::read(&database).expect("the vault reads"),
        vault_file,
        "the vault was written"
    );
    assert_eq!(snapshots(&database).len(), 3, "the vault's chain moved");

    opened
        .promote(Some(Seen::of(&database)))
        .expect("the copy becomes the vault");
    let old = opened
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect("the password is changed");
    let chain = snapshots(&database);
    assert_eq!(old, chain.len(), "the vault's own chain was not counted");
    for each in &chain {
        assert!(opens(&each.path, BUILT_PASSWORD), "slot {}", each.index);
    }
    assert_eq!(removed(&mut opened), old);
    assert!(snapshots(&database).is_empty());
}

/// The copy a lock left beside the vault opens with the password the vault
/// has now, and every sentence about it says so. Beside it, a change is
/// refused whatever was typed, until the copy is out of the way: changed, the
/// vault would open with one password and the copy with the old one, and
/// making the copy the vault would bring the old one back with nothing said.
#[test]
fn a_vault_with_a_lock_s_copy_beside_it_is_given_no_new_password_until_the_copy_goes() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    saved(&database, BUILT_PASSWORD, 2);
    let copy = unsaved::beside(&database).expect("the copy has a path");
    std::fs::copy(&database, &copy).expect("the copy is left beside it");
    let left = std::fs::read(&copy).expect("the copy reads");

    let mut opened = open(&database, BUILT_PASSWORD);
    let before = Before::of(&opened);
    for (what, current) in [
        ("the vault's password", BUILT_PASSWORD.as_bytes()),
        ("a wrong one", &b"not it"[..]),
    ] {
        let refused = opened
            .change_master_password(current, bytes(NEW))
            .expect_err("a vault was given a new password beside a lock's copy");
        assert!(
            matches!(refused, VaultError::PasswordBesideACopy),
            "{what}: {refused}"
        );
        before.nothing_written(&opened, BUILT_PASSWORD, what);
    }
    assert_eq!(
        std::fs::read(&copy).expect("the copy reads"),
        left,
        "the copy was written"
    );

    unsaved::discard(&database).expect("the copy is removed");
    opened
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect("the password is changed once the copy is gone");
    drop(opened);
    assert!(opens(&database, NEW));
    assert!(!opens(&database, BUILT_PASSWORD));
}

/// A date nobody would write today, so that the one the change sets cannot be
/// mistaken for it.
fn long_ago() -> chrono::NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2001, 2, 3)
        .and_then(|day| day.and_hms_opt(4, 5, 6))
        .expect("the date exists")
}

fn master_key_changed(path: &Path, password: &str) -> Option<chrono::NaiveDateTime> {
    let read = std::fs::read(path).expect("the file reads");
    Database::parse(&read, DatabaseKey::new().with_password(password))
        .expect("the file opens")
        .meta
        .master_key_changed
}

/// Another client wrote the file after it was opened. The change stops where a
/// save stops, and the vault goes on holding the password the reader was told
/// is still the one - which is what the conflict's way out then writes.
#[test]
fn a_change_stopped_by_another_client_s_write_keeps_the_old_password() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = built(directory.path(), "vault.kdbx", |database| {
        database.meta.master_key_changed = Some(long_ago());
    });
    let mut vault = open(&database, BUILT_PASSWORD);

    let theirs = built(directory.path(), "theirs.kdbx", |_| {});
    std::fs::copy(&theirs, &database).expect("the other client's write lands");

    let refused = vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect_err("a change went over somebody else's write");
    assert!(matches!(refused, VaultError::ExternalChange));

    vault.save_over().expect("the vault is written over theirs");
    drop(vault);

    assert!(opens(&database, BUILT_PASSWORD));
    assert!(
        !opens(&database, NEW),
        "a change that was refused reached the file"
    );
    assert_eq!(
        master_key_changed(&database, BUILT_PASSWORD),
        Some(long_ago()),
        "a change that was refused dated the key"
    );
}

/// Another client gave the file the very password the reader then asks Coffer
/// for, and wrote other changes with it. The file opens with the new key, and
/// it is still somebody else's: the change stops at the guard, the conflict
/// stays to be settled, and nothing of theirs is taken for this change's own
/// write - which would leave the next save free to write over their work.
#[test]
fn a_change_refused_over_a_file_another_client_wrote_under_the_same_new_password_raises_the_conflict()
 {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    let mut vault = open(&database, BUILT_PASSWORD);

    let theirs = built_with(directory.path(), "theirs.kdbx", NEW, |database| {
        database.meta.database_name = Some("theirs".to_owned());
        database.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(keepass::db::fields::TITLE, "written by the other client");
        });
    });
    std::fs::copy(&theirs, &database).expect("the other client's write lands");
    let written = std::fs::read(&database).expect("the file reads");

    let refused = vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect_err("a change took another client's file for its own");
    assert!(matches!(refused, VaultError::ExternalChange), "{refused}");
    assert!(
        matches!(
            vault.external_change().expect("the file is asked about"),
            Change::Modified
        ),
        "the vault took the other client's file for the one it agrees with"
    );
    let refused = vault
        .save()
        .expect_err("a save went over the other client's write");
    assert!(matches!(refused, VaultError::ExternalChange), "{refused}");
    assert_eq!(
        std::fs::read(&database).expect("the file reads"),
        written,
        "the other client's file was written over"
    );

    vault.save_over().expect("the vault is written over theirs");
    drop(vault);
    assert!(opens(&database, BUILT_PASSWORD));
    assert!(
        !opens(&database, NEW),
        "the key the reader was told did not take is the one the file has"
    );
}

/// The vault's file went after it was opened. The change stops where a save
/// stops, before anything is written, and puts nothing where the file was; the
/// vault goes on holding the old password and its date, which is what writing
/// the file back then writes.
#[test]
fn a_change_after_the_file_went_keeps_the_old_password() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = built(directory.path(), "vault.kdbx", |database| {
        database.meta.master_key_changed = Some(long_ago());
    });
    let mut vault = open(&database, BUILT_PASSWORD);
    std::fs::remove_file(&database).expect("the file goes");

    let refused = vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect_err("a change wrote a vault whose file had gone");
    assert!(matches!(refused, VaultError::DatabaseGone), "{refused}");
    assert!(
        database.symlink_metadata().is_err(),
        "the change put something where the file was"
    );
    assert!(snapshots(&database).is_empty());

    vault.save_over().expect("the vault is written back");
    drop(vault);
    assert!(opens(&database, BUILT_PASSWORD));
    assert!(
        !opens(&database, NEW),
        "a change that was refused reached the file"
    );
    assert_eq!(
        master_key_changed(&database, BUILT_PASSWORD),
        Some(long_ago()),
        "a change that was refused dated the key"
    );
}

/// Asks for a change while `fault` stands in the way of its write, with the
/// vault's file left as it was, and saves once the fault is `cleared`. The old
/// password went back into the vault, so that save writes the file under the
/// password it already has rather than one the reader was told did not take.
fn a_failed_change_keeps_the_old_password<T>(
    database: &Path,
    fault: impl FnOnce() -> T,
    cleared: impl FnOnce(T),
) {
    let mut vault = open(database, BUILT_PASSWORD);
    let standing = fault();
    let refused = vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect_err("the change went through");
    assert!(matches!(refused, VaultError::Io(_)), "{refused}");

    cleared(standing);
    vault
        .save()
        .expect("the vault saves once the place takes it");
    drop(vault);

    assert!(
        opens(database, BUILT_PASSWORD),
        "the old password no longer opens it"
    );
    assert!(
        !opens(database, NEW),
        "the save after a failed change wrote the new password"
    );
}

/// The directory takes no new file, so the write fails before anything is
/// staged.
#[test]
fn a_change_whose_write_fails_keeps_the_old_password() {
    if !permissions_apply() {
        return;
    }
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());

    a_failed_change_keeps_the_old_password(&database, || Frozen::over(directory.path()), drop);
}

/// The rotation fails after the whole file was encrypted under the new key,
/// and as root as well as anybody: a directory where the oldest snapshot goes.
#[test]
fn a_change_that_cannot_rotate_its_snapshots_keeps_the_old_password() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    let oldest = snapshot::slot(&database, snapshot::SNAPSHOT_COUNT).expect("the slot has a path");

    a_failed_change_keeps_the_old_password(
        &database,
        || std::fs::create_dir(&oldest).expect("the directory is made"),
        |()| std::fs::remove_dir(&oldest).expect("the directory goes"),
    );
}

/// The file itself takes no write, in a directory that does: renaming over it
/// would need no permission on it, so the write is refused before anything is
/// staged, and the save once it takes one again is under the old password.
#[test]
fn a_vault_file_that_is_read_only_refuses_a_change_and_keeps_the_old_password() {
    use std::os::unix::fs::PermissionsExt;

    if !permissions_apply() {
        return;
    }
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    saved(&database, BUILT_PASSWORD, 1);
    let mut vault = open(&database, BUILT_PASSWORD);
    let mode = |bits| {
        std::fs::set_permissions(&database, std::fs::Permissions::from_mode(bits))
            .expect("the mode is set");
    };

    mode(0o400);
    let file = std::fs::read(&database).expect("the file reads");
    let refused = vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect_err("a file that takes no write was given a new password");
    assert!(matches!(refused, VaultError::Io(_)), "{refused}");
    assert_eq!(std::fs::read(&database).expect("the file reads"), file);
    assert_eq!(snapshots(&database).len(), 1, "a snapshot was taken");
    assert!(opens(&database, BUILT_PASSWORD));

    mode(0o600);
    vault
        .save()
        .expect("the vault saves once the file takes it");
    drop(vault);
    assert!(opens(&database, BUILT_PASSWORD));
    assert!(
        !opens(&database, NEW),
        "the save after a refused change wrote the new password"
    );
}

#[test]
fn a_key_file_stays_part_of_the_key_after_the_password_changes() {
    let (_scratch, database) = scratch("keyfile-kdbx41.kdbx");
    let key = support::password("coffer-keyfile")
        .with_key_file(&fixture("keyfile.key"))
        .expect("the key file reads");
    let mut vault = Vault::open(&database, key, LockPolicy::Respect).expect("the vault opens");

    vault
        .change_master_password(b"coffer-keyfile", bytes(NEW))
        .expect("the password is changed");
    drop(vault);

    assert!(opens_with(&database, Some(NEW), Some("keyfile.key")));
    assert!(
        !opens_with(&database, Some(NEW), None),
        "the new password alone opens a vault that needed a key file"
    );
    assert!(
        !opens_with(&database, None, Some("keyfile.key")),
        "the key file alone opens it"
    );
    assert!(!opens_with(
        &database,
        Some("coffer-keyfile"),
        Some("keyfile.key")
    ));
}

/// A vault opened by its key file holds no password, so the current one typed
/// is nothing at all. From the change on, the key file needs the password
/// beside it, and the fallback that opens a key file alone opens nothing.
#[test]
fn a_vault_opened_by_its_key_file_alone_gets_a_password_beside_it() {
    let (_scratch, database) = scratch("keyfile-only-kdbx41.kdbx");
    let alone = || {
        support::password("")
            .with_key_file(&fixture("keyfile-only.key"))
            .expect("the key file reads")
    };
    let mut vault = Vault::open(&database, alone(), LockPolicy::Respect).expect("the vault opens");

    let refused = vault
        .change_master_password(b"anything", bytes(NEW))
        .expect_err("a password was taken for a vault that has none");
    assert!(matches!(refused, VaultError::NotTheCurrentPassword));
    vault
        .change_master_password(b"", bytes(NEW))
        .expect("the password is changed");
    drop(vault);

    let refused = Vault::open(&database, alone(), LockPolicy::Respect)
        .expect_err("the key file alone still opens it");
    assert!(matches!(refused, VaultError::WrongCredentials));
    let both = support::password(NEW)
        .with_key_file(&fixture("keyfile-only.key"))
        .expect("the key file reads");
    Vault::open(&database, both, LockPolicy::Respect)
        .expect("the password and the key file open it");
}

/// A vault made with an empty password and no key file has a password all
/// the same, and nothing is what is typed as the current one. The reader most
/// likely to give a vault a password is the one who never set one.
#[test]
fn a_vault_whose_password_is_empty_and_has_no_key_file_can_be_given_one() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = built_with(directory.path(), "vault.kdbx", "", |_| {});
    let mut vault = open(&database, "");

    let refused = vault
        .change_master_password(b" ", bytes(NEW))
        .expect_err("a space was taken for an empty password");
    assert!(matches!(refused, VaultError::NotTheCurrentPassword));
    vault
        .change_master_password(b"", bytes(NEW))
        .expect("the password is changed");
    drop(vault);

    assert!(opens(&database, NEW));
    assert!(
        !opens(&database, ""),
        "the vault still opens with no password"
    );
}

/// A KDBX 3.1 file with no attachments is written back as KDBX 4.1, and the
/// write that changes its key is the one that changes its format as well.
/// Everything in it, and everything the window added before the change, opens
/// under the new password and only under it.
#[test]
fn a_kdbx31_vault_given_a_new_password_opens_with_it_only_and_keeps_every_field() {
    let (_scratch, database) = scratch("empty-kdbx31.kdbx");
    let mut vault = open(&database, SECRET);
    assert_eq!(
        vault.read_only(),
        None,
        "no attachments, so it can be written"
    );

    let root = vault.tree().id;
    let group = vault
        .create_group(root, "added before the change")
        .expect("the folder is made");
    let id = vault
        .create_entry(group, Kind::Login)
        .expect("the entry is made");
    for (field, value) in [
        (
            fields::TITLE,
            NewValue::Open("in a KDBX 3.1 file".to_owned()),
        ),
        (fields::USERNAME, NewValue::Open("somebody".to_owned())),
        (
            fields::PASSWORD,
            NewValue::Protected(Zeroizing::new("a secret <&> \u{202e}".to_owned())),
        ),
        ("A field of its own", NewValue::Open("its value".to_owned())),
    ] {
        vault.set_field(id, field, value).expect("the field is set");
    }
    let before = everything(&vault);

    vault
        .change_master_password(SECRET.as_bytes(), bytes(NEW))
        .expect("the password is changed");
    drop(vault);

    let read = std::fs::read(&database).expect("the file reads");
    let written = Database::parse(&read, DatabaseKey::new().with_password(NEW))
        .expect("the new password opens it");
    assert_eq!(written.config.version, DatabaseVersion::KDB4(1));
    assert!(!opens(&database, SECRET), "the old password still opens it");

    let again = open(&database, NEW);
    assert!(
        everything(&again) == before,
        "the vault lost or changed something on the way to KDBX 4.1 and a new password"
    );
}

fn kdf_of(path: &Path, password: &str) -> KdfConfig {
    let read = std::fs::read(path).expect("the file reads");
    Database::parse(&read, DatabaseKey::new().with_password(password))
        .expect("the file opens")
        .config
        .kdf_config
}

/// A change is a save under another key, and nothing more: what it costs to
/// open the vault is whatever the vault already asked, whoever chose it.
#[test]
fn a_change_keeps_the_key_derivation_the_vault_had() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let cheap = vault(directory.path());
    let (_rich, rich) = scratch(RICH);
    let made = directory.path().join("made.kdbx");
    drop(
        Vault::create(
            &made,
            support::password(SECRET),
            &Recipe {
                name: "made",
                work: Work::at(1),
            },
        )
        .expect("the vault is made"),
    );

    for (database, password) in [(&cheap, BUILT_PASSWORD), (&rich, SECRET), (&made, SECRET)] {
        let derived = kdf_of(database, password);
        let asked = keepass::db::Times::now();

        open(database, password)
            .change_master_password(password.as_bytes(), bytes(NEW))
            .expect("the password is changed");

        assert_eq!(kdf_of(database, NEW), derived, "{}", database.display());
        assert!(
            master_key_changed(database, NEW).is_some_and(|changed| changed >= asked),
            "{}: the key's date did not move",
            database.display()
        );
    }
}

/// Saves the vault at `database` `times` times, which leaves as many
/// snapshots beside it.
fn saved(database: &Path, password: &str, times: usize) {
    let mut vault = open(database, password);
    for _ in 0..times {
        vault.save().expect("the vault saves");
    }
}

#[test]
fn the_snapshots_older_than_the_change_are_counted_and_open_with_the_old_password() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    saved(&database, BUILT_PASSWORD, 3);

    let old = open(&database, BUILT_PASSWORD)
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect("the password is changed");
    assert_eq!(old, 4, "three saves and the change's own snapshot");

    for each in snapshots(&database) {
        assert!(opens(&each.path, BUILT_PASSWORD), "slot {}", each.index);
        assert!(!opens(&each.path, NEW), "slot {}", each.index);
    }
}

/// The question is answered after later saves, which move every old snapshot
/// down a slot and put new ones in front. What goes is the files the change
/// counted, wherever they are now, and nothing else.
#[test]
fn only_the_snapshots_older_than_the_change_are_removed_wherever_saves_moved_them() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    saved(&database, BUILT_PASSWORD, 3);

    let mut vault = open(&database, BUILT_PASSWORD);
    let old = vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect("the password is changed");
    assert_eq!(old, 4);
    vault.save().expect("the vault saves");
    vault.save().expect("the vault saves");

    let lock = PathBuf::from(format!("{}.lock", database.display()));
    let file = std::fs::read(&database).expect("the vault reads");
    let held = std::fs::read(&lock).expect("the lock reads");

    assert_eq!(removed(&mut vault), 4);
    let left: Vec<u32> = snapshots(&database).iter().map(|each| each.index).collect();
    assert_eq!(
        left,
        vec![1, 2],
        "the snapshots under the new password went"
    );
    for each in snapshots(&database) {
        assert!(opens(&each.path, NEW), "slot {}", each.index);
    }

    assert_eq!(removed(&mut vault), 0);
    assert_eq!(std::fs::read(&database).expect("the vault reads"), file);
    assert_eq!(std::fs::read(&lock).expect("the lock reads"), held);
}

#[test]
fn ten_saves_after_a_change_leave_no_old_snapshot_to_remove() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    saved(&database, BUILT_PASSWORD, 5);

    let mut vault = open(&database, BUILT_PASSWORD);
    vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect("the password is changed");
    for _ in 0..snapshot::SNAPSHOT_COUNT {
        vault.save().expect("the vault saves");
    }

    assert_eq!(removed(&mut vault), 0);
    assert_eq!(snapshots(&database).len(), 10);
}

/// The second change makes every snapshot old, the ones written under the
/// first new password included.
#[test]
fn a_second_change_makes_the_whole_chain_old() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    let mut vault = open(&database, BUILT_PASSWORD);

    vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes("second"))
        .expect("the password is changed");
    vault.save().expect("the vault saves");
    let old = vault
        .change_master_password(b"second", bytes("third"))
        .expect("the password is changed again");

    assert_eq!(old, snapshots(&database).len());
    assert_eq!(removed(&mut vault), old);
    assert!(snapshots(&database).is_empty());
    drop(vault);
    assert!(opens(&database, "third"));
}

/// A second change that was refused, or whose write failed, changed nothing,
/// so the old snapshots the first change counted are still the ones there are,
/// and still the ones a removal takes.
#[test]
fn a_refused_or_failed_second_change_keeps_the_first_change_s_old_snapshots() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    saved(&database, BUILT_PASSWORD, 2);
    let mut vault = open(&database, BUILT_PASSWORD);
    let old = vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect("the password is changed");

    for (what, current, next) in [
        ("the password it has", NEW, NEW),
        ("a wrong current password", BUILT_PASSWORD, "third"),
        ("an empty new one", NEW, ""),
    ] {
        vault
            .change_master_password(current.as_bytes(), bytes(next))
            .expect_err(what);
    }
    if permissions_apply() {
        let _frozen = Frozen::over(directory.path());
        vault
            .change_master_password(NEW.as_bytes(), bytes("third"))
            .expect_err("a change went through a directory that takes no file");
    }

    assert_eq!(removed(&mut vault), old);
    assert!(snapshots(&database).is_empty());
    drop(vault);
    assert!(opens(&database, NEW));
}

#[test]
fn a_snapshot_that_will_not_go_is_remembered_and_removed_once_it_can_be() {
    if !permissions_apply() {
        return;
    }
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    saved(&database, BUILT_PASSWORD, 2);
    let mut vault = open(&database, BUILT_PASSWORD);
    let old = vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect("the password is changed");

    let frozen = Frozen::over(directory.path());
    let refused = vault.remove_old_snapshots();
    assert!(
        refused.refused.is_some(),
        "a directory that takes nothing away gave up a snapshot"
    );
    assert_eq!((refused.gone, refused.left), (0, old));
    assert_eq!(snapshots(&database).len(), old, "a snapshot went");

    drop(frozen);
    assert_eq!(removed(&mut vault), old);
    assert!(snapshots(&database).is_empty());
}

/// A link somebody left at a slot that leads round in a circle cannot be asked
/// about. It hides none of the old snapshots beside it - from the count, or
/// from the removal - and it stays where it is: it is not one of them.
#[test]
fn a_slot_nobody_can_read_hides_no_old_snapshot() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    saved(&database, BUILT_PASSWORD, 2);
    let mut vault = open(&database, BUILT_PASSWORD);
    let old = vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect("the password is changed");

    let circle = snapshot::slot(&database, snapshot::SNAPSHOT_COUNT).expect("the slot has a path");
    std::os::unix::fs::symlink(&circle, &circle).expect("the link is made");
    assert_eq!(snapshots(&database).len(), old, "the circle hid the chain");

    assert_eq!(removed(&mut vault), old);
    assert!(snapshots(&database).is_empty());
    assert!(
        circle.symlink_metadata().is_ok(),
        "something that was not an old snapshot went"
    );
}

/// A file nothing may remove until the flag that says so is taken off it,
/// which happens when this goes. A Mac has the flag for any owner; Linux keeps
/// its own for root, so the test that needs one runs on a Mac.
#[cfg(target_os = "macos")]
struct Undeletable(std::ffi::CString);

#[cfg(target_os = "macos")]
impl Undeletable {
    fn over(path: &Path) -> Undeletable {
        use std::os::unix::ffi::OsStrExt;
        let name =
            std::ffi::CString::new(path.as_os_str().as_bytes()).expect("no null in the path");
        // SAFETY: the name is a C string that outlives the call.
        let flagged = unsafe { libc::chflags(name.as_ptr(), libc::UF_IMMUTABLE) };
        assert_eq!(flagged, 0, "{}", std::io::Error::last_os_error());
        Undeletable(name)
    }
}

#[cfg(target_os = "macos")]
impl Drop for Undeletable {
    fn drop(&mut self) {
        // SAFETY: as above.
        unsafe { libc::chflags(self.0.as_ptr(), 0) };
    }
}

/// One old snapshot will not go and the rest do. The answer says how many went
/// and how many are left - the one the window goes on asking about - and the
/// one that stayed is remembered, so the next press takes it and nothing else.
#[cfg(target_os = "macos")]
#[test]
fn a_removal_that_could_not_take_one_says_how_many_are_left_and_takes_it_later() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    saved(&database, BUILT_PASSWORD, 3);
    let mut vault = open(&database, BUILT_PASSWORD);
    let old = vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect("the password is changed");
    assert_eq!(old, 4);

    let stuck = snapshot::slot(&database, 3).expect("the slot has a path");
    let held = Undeletable::over(&stuck);
    let removal = vault.remove_old_snapshots();
    assert_eq!((removal.gone, removal.left), (3, 1));
    assert!(removal.refused.is_some(), "the refusal was not said");
    let left: Vec<u32> = snapshots(&database).iter().map(|each| each.index).collect();
    assert_eq!(left, vec![3]);
    assert!(opens(&stuck, BUILT_PASSWORD));

    drop(held);
    vault.save().expect("the vault saves");
    assert_eq!(removed(&mut vault), 1, "the one that stayed was forgotten");
    let left: Vec<u32> = snapshots(&database).iter().map(|each| each.index).collect();
    assert_eq!(left, vec![1], "the snapshot under the new password went");
}

/// Coffer never puts a link in the chain, and one somebody else put there may
/// lead to the vault itself. It goes as a link, the vault stays, and the
/// snapshot a later save makes of the vault is not taken for it.
#[test]
fn a_link_among_the_snapshots_is_removed_as_a_link_and_the_vault_stays() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    saved(&database, BUILT_PASSWORD, 1);
    let linked = snapshot::slot(&database, 2).expect("the slot has a path");
    std::os::unix::fs::symlink(&database, &linked).expect("the link is made");

    let mut vault = open(&database, BUILT_PASSWORD);
    let old = vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect("the password is changed");
    assert_eq!(old, 3, "two snapshots and the link");
    vault.save().expect("the vault saves");

    assert_eq!(removed(&mut vault), 3);
    let link = snapshot::slot(&database, 4).expect("the slot has a path");
    assert!(link.symlink_metadata().is_err(), "the link is still there");
    let left: Vec<u32> = snapshots(&database).iter().map(|each| each.index).collect();
    assert_eq!(
        left,
        vec![1],
        "the snapshot of the vault under its new password went"
    );
    drop(vault);
    assert!(opens(&database, NEW), "the vault went with the link");
}

/// A copy a lock leaves after the change is under the new password, and the
/// removal leaves it where it is: it holds the only copy of work the vault
/// has not got.
#[test]
fn removing_old_snapshots_never_touches_a_rescue_copy() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    saved(&database, BUILT_PASSWORD, 2);

    let mut vault = open(&database, BUILT_PASSWORD);
    vault
        .change_master_password(BUILT_PASSWORD.as_bytes(), bytes(NEW))
        .expect("the password is changed");
    let copy = unsaved::beside(&database).expect("the copy has a path");
    std::fs::copy(&database, &copy).expect("the copy is left beside it");
    removed(&mut vault);

    assert!(opens(&copy, NEW), "the copy a lock left went");
}

#[test]
fn removing_old_snapshots_without_a_change_removes_nothing() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    saved(&database, BUILT_PASSWORD, 12);

    let mut vault = open(&database, BUILT_PASSWORD);
    assert_eq!(removed(&mut vault), 0);
    assert_eq!(snapshots(&database).len(), 10);
}

/// Whatever the reader can type is a password, and the vault opens with it
/// afterwards exactly as typed.
#[test]
fn passwords_at_the_edges_change_and_open_again() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());

    let mut current = BUILT_PASSWORD.to_owned();
    for (what, next) in [
        ("ten megabytes", "x".repeat(10 * 1024 * 1024)),
        ("combining marks and nothing else", "\u{301}".repeat(1000)),
        ("a right-to-left override", "pass\u{202e}drow".to_owned()),
        ("a null in the middle", "a\u{0}b".to_owned()),
        ("characters outside the basic plane", "🔐𝔘𝔫𝔦𝔠𝔬𝔡𝔢".to_owned()),
    ] {
        let mut vault = open(&database, &current);
        vault
            .change_master_password(current.as_bytes(), bytes(&next))
            .unwrap_or_else(|error| panic!("{what}: {error}"));
        drop(vault);

        let refused = Vault::open(&database, support::password(&current), LockPolicy::Respect)
            .expect_err("the old password still opens it");
        assert!(matches!(refused, VaultError::WrongCredentials), "{what}");
        drop(open(&database, &next));
        current = next;
    }
}

#[test]
fn twenty_changes_keep_ten_snapshots_and_the_last_password() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = vault(directory.path());
    let mut vault = open(&database, BUILT_PASSWORD);

    let mut current = BUILT_PASSWORD.to_owned();
    for round in 0..20 {
        let next = format!("password {round}");
        vault
            .change_master_password(current.as_bytes(), bytes(&next))
            .expect("the password is changed");
        assert!(snapshots(&database).len() <= 10, "the chain grew past ten");
        current = next;
    }
    drop(vault);

    assert!(opens(&database, &current));
    assert!(!opens(&database, "password 18"));
    assert!(!opens(&database, BUILT_PASSWORD));
}

/// The round trip, with the one change it is allowed: the date the key
/// changed. Everything else KeePassXC finds in the file is what it found
/// before, under the new password.
#[test]
fn keepassxc_opens_a_vault_whose_master_password_coffer_changed_and_finds_every_field() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };

    let (_scratch, database) = scratch(RICH);
    let before = support::export(&tool, &database, SECRET, None);

    open(&database, SECRET)
        .change_master_password(SECRET.as_bytes(), bytes(NEW))
        .expect("the password is changed");

    let after = support::export(&tool, &database, NEW, None);
    let moved = differences(&canonical(&before, false), &canonical(&after, false));
    assert_eq!(moved.len(), 1, "{}", moved.join("\n"));
    assert!(
        moved.iter().all(|line| line.contains("MasterKeyChanged")),
        "{}",
        moved.join("\n")
    );
}
