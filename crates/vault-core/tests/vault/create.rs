//! Making a vault where there is none.
//!
//! The one operation in Coffer that writes to a path nothing is at yet, which
//! makes it the one that could put a file where somebody's vault used to be.
//! Most of what is below is about the refusals rather than the writing.

use std::path::{Path, PathBuf};

use keepass::config::KdfConfig;
use vault_core::kdf::{MEMORY_BYTES, PARALLELISM, Work};
use vault_core::{LockPolicy, MasterKey, Recipe, Vault, VaultError};

use crate::support::{fixture, open, password};

/// The cheapest derivation the format will accept. Every test here is about
/// what is written rather than about how long it takes to write it, and a
/// calibrated second per creation would make this file the slowest in the
/// suite.
fn cheap() -> Work {
    Work::at(1)
}

fn make(path: &Path, secret: &str, name: &str) -> Result<Vault, VaultError> {
    Vault::create(
        path,
        password(secret),
        &Recipe {
            name,
            work: cheap(),
        },
    )
}

fn scratch() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let path = directory.path().join("new.kdbx");
    (directory, path)
}

const SECRET: &str = "a password nobody has to remember";

/// A database Coffer made opens again with the same password, and comes back
/// with the shape it was given.
#[test]
fn a_vault_that_was_just_made_opens_again() {
    let (_scratch, path) = scratch();
    let mut made = make(&path, SECRET, "Work").expect("the vault is made");
    assert_eq!(made.count(), 0);
    assert_eq!(made.tree().name, "Work");
    assert!(!made.is_read_only());
    assert_eq!(made.rescue(), vault_core::Rescue::Nothing);
    drop(made);

    let reopened = open(&path, SECRET);
    assert_eq!(reopened.tree().name, "Work");
    assert_eq!(reopened.count(), 0);
}

/// The whole point of the calibration reaching the file. A test that only
/// asserted the database opens would pass with the Argon2**d** at one megabyte
/// that the library hands out by default, which is spelt almost the same way
/// and is not what the spec asks for.
#[test]
fn the_header_says_argon2id_at_the_memory_the_spec_fixes() {
    let (_scratch, path) = scratch();
    drop(make(&path, SECRET, "Work").expect("the vault is made"));

    let bytes = std::fs::read(&path).expect("the file is there");
    let database = keepass::Database::parse(
        &bytes[..],
        keepass::DatabaseKey::new().with_password(SECRET),
    )
    .expect("the file opens");

    assert_eq!(
        database.config.kdf_config,
        KdfConfig::Argon2id {
            iterations: 1,
            memory: MEMORY_BYTES,
            parallelism: PARALLELISM,
            version: argon2::Version::Version13,
        }
    );
}

/// A new database says what it is rather than leaving other clients to fill the
/// gaps in with their own defaults. Asserted against the file, never against a
/// keepassxc-cli export: an export substitutes those defaults on both sides of
/// a comparison and reports no difference.
#[test]
fn a_new_vault_describes_itself_in_its_own_header() {
    let (_scratch, path) = scratch();
    drop(make(&path, SECRET, "Personal").expect("the vault is made"));

    let bytes = std::fs::read(&path).expect("the file is there");
    let database = keepass::Database::parse(
        &bytes[..],
        keepass::DatabaseKey::new().with_password(SECRET),
    )
    .expect("the file opens");

    assert_eq!(database.meta.database_name.as_deref(), Some("Personal"));
    assert_eq!(database.meta.generator.as_deref(), Some("Coffer"));
    assert_eq!(database.meta.history_max_items, Some(10));
    assert_eq!(database.meta.history_max_size, Some(6 * 1024 * 1024));
    assert_eq!(database.meta.maintenance_history_days, Some(365));
    assert_eq!(database.meta.recyclebin_enabled, Some(true));
    // Asked for and not made. A UUID naming a group that is not in the file is
    // how a database ends up with two recycle bins.
    assert_eq!(database.meta.recyclebin_uuid, None);
    assert!(database.meta.memory_protection.is_some());
    assert!(database.meta.master_key_changed.is_some());
    assert_eq!(database.root().name, "Personal");
}

/// The one way this slice could destroy a vault. A staged write renames over
/// whatever is at the target, and a creation rotates no snapshot, so what was
/// there would be gone with nothing to go back to.
#[test]
fn a_vault_is_never_written_over_a_file_that_is_already_there() {
    let (_scratch, path) = scratch();
    let existing = std::fs::read(fixture("rich-kdbx41.kdbx")).expect("the fixture reads");
    std::fs::write(&path, &existing).expect("it lands at the target");

    let refused = make(&path, SECRET, "Work").expect_err("an existing file is refused");
    assert!(matches!(refused, VaultError::DatabaseExists));
    assert_eq!(
        std::fs::read(&path).expect("it is still there"),
        existing,
        "the file that was there was disturbed"
    );

    // A directory is a file that is already there as much as anything else is.
    let directory = path.with_file_name("a directory.kdbx");
    std::fs::create_dir(&directory).expect("the directory is made");
    assert!(matches!(
        make(&directory, SECRET, "Work"),
        Err(VaultError::DatabaseExists)
    ));
}

/// Two Coffers making a vault at one name, at once.
///
/// The window between asking whether a file is there and writing one is the
/// whole of key derivation - a calibrated second. Both used to pass the
/// question, and the second rename destroyed the first one's vault with no
/// snapshot behind it, which is the one way this application can lose one.
#[test]
fn two_creations_at_one_name_leave_one_whole_vault() {
    let (_scratch, path) = scratch();

    // Heavy enough that the two are inside key derivation together.
    let recipe = || Recipe {
        name: "Work",
        work: Work::at(40),
    };

    let (first, second) = std::thread::scope(|scope| {
        let racing = path.clone();
        let other = scope.spawn(move || {
            Vault::create(&racing, password(SECRET), &recipe()).map(|made| {
                drop(made);
            })
        });
        let mine = Vault::create(&path, password(SECRET), &recipe()).map(|made| {
            drop(made);
        });
        (mine, other.join().expect("the racing thread finishes"))
    });

    // Exactly one of them made it, and what is on the disk is that one's vault
    // rather than half of each.
    assert_ne!(
        first.is_ok(),
        second.is_ok(),
        "both creations claimed the same name"
    );
    for outcome in [first, second] {
        if let Err(refused) = outcome {
            assert!(matches!(refused, VaultError::DatabaseExists), "{refused:?}");
        }
    }
    assert_eq!(open(&path, SECRET).tree().name, "Work");
}

/// A creation that cannot have the database leaves nothing behind, so the next
/// attempt at the same name is not refused as a file that is already there.
#[test]
fn a_creation_that_cannot_take_the_lock_leaves_the_name_free() {
    let (_scratch, path) = scratch();
    let held = make(&path, SECRET, "Work").expect("the vault is made");

    let beside = path.with_file_name("second.kdbx");
    let holder = vault_core::storage::lock::inspect(&path)
        .expect("the lock file reads")
        .expect("the vault is held");
    let _ = holder;

    // A lock beside a name nothing has written yet: the file is reserved, the
    // lock is refused, and the reservation has to go with it.
    std::fs::copy(
        path.with_file_name("new.kdbx.lock"),
        beside.with_file_name("second.kdbx.lock"),
    )
    .expect("the lock file copies");

    let refused = make(&beside, SECRET, "Work").expect_err("a held database is refused");
    assert!(matches!(refused, VaultError::Locked(_)));
    assert!(
        !beside.exists(),
        "a creation that was refused left a file at the name"
    );

    drop(held);
}

/// A name of the shape Coffer gives its own snapshots opens like any other
/// database and then refuses every save, for good.
#[test]
fn a_name_belonging_to_a_snapshot_is_refused() {
    let (scratch, _) = scratch();
    for name in ["vault.1.bak", "vault.10.bak", "personal.7.bak"] {
        let path = scratch.path().join(name);
        assert!(
            matches!(make(&path, SECRET, "Work"), Err(VaultError::ReservedName)),
            "{name}"
        );
        assert!(!path.exists(), "{name} was written anyway");
    }

    // The shape is what matters and not the number: eleven is not a slot.
    let ordinary = scratch.path().join("vault.11.bak");
    assert!(make(&ordinary, SECRET, "Work").is_ok());
}

/// A vault anybody can open is not a vault. The library writes one happily, and
/// the failure it gives for a key with nothing in it at all reads as a wrong
/// password, which is nonsense as the answer to making a database.
#[test]
fn a_vault_with_no_master_password_is_refused() {
    let (_scratch, path) = scratch();
    assert!(matches!(
        make(&path, "", "Work"),
        Err(VaultError::EmptyMasterPassword)
    ));
    assert!(!path.exists());

    // Reading one that already has an empty password is a different question,
    // and the answer is still yes.
    let already = path.with_file_name("already.kdbx");
    let mut writing = keepass::Database::new();
    writing.config.kdf_config = KdfConfig::Aes { rounds: 16 };
    let mut file = std::fs::File::create(&already).expect("the file is created");
    writing
        .save(&mut file, keepass::DatabaseKey::new().with_password(""))
        .expect("a database with no password saves");
    drop(file);

    assert!(
        Vault::open(
            &already,
            MasterKey::from_password(zeroize::Zeroizing::new(Vec::new())),
            LockPolicy::Respect
        )
        .is_ok()
    );
}

/// Passwords nobody would type, and one nobody could.
#[test]
fn passwords_at_the_edges_still_make_a_vault_that_opens() {
    let (scratch, _) = scratch();

    let long = "a".repeat(10 * 1024 * 1024);
    let combining = "\u{301}\u{301}\u{301}\u{301}";
    for (index, secret) in [long.as_str(), combining, "ユニコード 🔐", " "]
        .into_iter()
        .enumerate()
    {
        let path = scratch.path().join(format!("edge{index}.kdbx"));
        drop(make(&path, secret, "Work").expect("the vault is made"));
        assert_eq!(open(&path, secret).tree().name, "Work");
    }
}

/// A master password that is not text at all. KeePass hashes passwords as
/// UTF-8, so there is nothing to hash, and nothing is written.
#[test]
fn a_master_password_that_is_not_text_is_refused_before_anything_is_written() {
    let (_scratch, path) = scratch();
    let refused = Vault::create(
        &path,
        MasterKey::from_password(zeroize::Zeroizing::new(vec![0xff, 0xfe])),
        &Recipe {
            name: "Work",
            work: cheap(),
        },
    )
    .expect_err("bytes that are not text are refused");

    assert!(matches!(refused, VaultError::PasswordNotUtf8));
    assert!(!path.exists());
}

/// A name is the user's text, and it lands in two places in the file. Anything
/// XML cannot carry would make a database no reader could open again.
#[test]
fn a_name_a_kdbx_file_cannot_hold_is_refused_and_nothing_is_written() {
    let (scratch, path) = scratch();
    for name in ["a\u{0}b", "\u{7f}", "a\u{1b}[0m", "\u{fffe}"] {
        assert!(
            matches!(make(&path, SECRET, name), Err(VaultError::UnwritableText)),
            "{name:?}"
        );
    }
    assert_eq!(
        std::fs::read_dir(scratch.path())
            .expect("the directory reads")
            .count(),
        0,
        "something was written for a name that cannot be written"
    );

    // Text that is merely surprising is text all the same.
    for (index, name) in ["", "  ", "\u{202e}drawkcab", &"n".repeat(1024 * 1024)]
        .into_iter()
        .enumerate()
    {
        let path = scratch.path().join(format!("odd{index}.kdbx"));
        drop(make(&path, SECRET, name).expect("the vault is made"));
        assert_eq!(open(&path, SECRET).tree().name, name);
    }
}

/// Key derivation the pre-flight would refuse on the way back in is refused on
/// the way out, so a vault Coffer makes is never one Coffer cannot open.
#[test]
fn work_the_preflight_would_refuse_is_never_written() {
    let (scratch, _) = scratch();
    let absurd = [
        Work {
            iterations: 0,
            memory: MEMORY_BYTES,
            parallelism: PARALLELISM,
        },
        Work {
            iterations: 1,
            memory: MEMORY_BYTES,
            parallelism: 0,
        },
        Work {
            iterations: 1,
            memory: u64::MAX,
            parallelism: PARALLELISM,
        },
        // One past the product ceiling at this memory, which is where a fast
        // machine's calibration would land without its clamp.
        Work {
            iterations: 1025,
            memory: MEMORY_BYTES,
            parallelism: PARALLELISM,
        },
    ];

    for (index, work) in absurd.into_iter().enumerate() {
        let path = scratch.path().join(format!("absurd{index}.kdbx"));
        let refused = Vault::create(&path, password(SECRET), &Recipe { name: "Work", work })
            .expect_err("absurd work is refused");
        assert!(
            matches!(refused, VaultError::AbsurdKeyDerivation),
            "{work:?}"
        );
        assert!(!path.exists());
    }

    // That the count just under the ceiling is *accepted* is asserted where it
    // costs nothing: `kdf`'s own suite runs the calibration against a machine
    // fast enough to be clamped and checks the answer through the same
    // pre-flight. Writing a file at a thousand and twenty-four passes here
    // would be ten seconds of real Argon2 for the same fact.
}

/// Nowhere to put it. Each of these leaves the directory as it found it.
#[test]
fn a_target_that_cannot_be_written_leaves_nothing_behind() {
    use std::os::unix::fs::PermissionsExt as _;

    let (scratch, _) = scratch();

    // A directory that is not there.
    let missing = scratch.path().join("not here/new.kdbx");
    assert!(make(&missing, SECRET, "Work").is_err());
    assert!(!missing.exists());

    // A path that names no file at all.
    assert!(make(Path::new("/"), SECRET, "Work").is_err());

    if !crate::support::permissions_apply() {
        return;
    }

    let closed = scratch.path().join("closed");
    std::fs::create_dir(&closed).expect("the directory is made");
    std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o500))
        .expect("the directory is closed");

    let refused = closed.join("new.kdbx");
    assert!(make(&refused, SECRET, "Work").is_err());
    assert!(!refused.exists());

    std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o700))
        .expect("the directory is opened again");
    assert_eq!(
        std::fs::read_dir(&closed)
            .expect("the directory reads")
            .count(),
        0,
        "a staged file was left behind"
    );
}

/// A vault that has just been made is held, the way one that has just been
/// opened is. Two creations racing at one path end with one file and one
/// refusal.
#[test]
fn a_new_vault_is_held_by_the_process_that_made_it() {
    let (_scratch, path) = scratch();
    let held = make(&path, SECRET, "Work").expect("the vault is made");

    assert!(
        vault_core::storage::lock::inspect(&path)
            .expect("the lock file reads")
            .is_some()
    );

    let second = make(&path, SECRET, "Work").expect_err("the second creation is refused");
    assert!(matches!(second, VaultError::DatabaseExists));

    drop(held);
    assert!(
        vault_core::storage::lock::inspect(&path)
            .expect("the lock file reads")
            .is_none()
    );
}

/// Made, filled in, saved, and read back. The point of creating a database is
/// that it becomes an ordinary one the moment it exists.
#[test]
fn a_vault_that_was_made_takes_a_change_and_keeps_it() {
    let (_scratch, path) = scratch();
    let mut made = make(&path, SECRET, "Work").expect("the vault is made");

    let root = made.tree().id;
    let folder = made
        .create_group(root, "Clients")
        .expect("the folder is made");
    let entry = made.create_entry(folder).expect("the entry is made");
    made.set_field(
        entry,
        keepass::db::fields::PASSWORD,
        vault_core::NewValue::Protected(zeroize::Zeroizing::new("hunter2".to_owned())),
    )
    .expect("the password is written");
    made.save().expect("the vault saves");
    drop(made);

    let reopened = open(&path, SECRET);
    let found = crate::support::all_entries(&reopened);
    assert_eq!(found.len(), 1);
    let secret = reopened
        .reveal(found[0].id, keepass::db::fields::PASSWORD)
        .expect("the password comes back");
    assert_eq!(secret.expose_str(), Some("hunter2"));
}

/// A date the file leaves out is a date every client puts its own value in, so
/// two readings of the same file disagree and neither of them is what Coffer
/// wrote. Everything Coffer makes carries its own.
#[test]
fn everything_coffer_makes_carries_its_own_dates() {
    let (_scratch, path) = scratch();
    let mut made = make(&path, SECRET, "Work").expect("the vault is made");

    let root = made.tree().id;
    let folder = made
        .create_group(root, "Clients")
        .expect("the folder is made");
    made.create_entry(folder).expect("the entry is made");
    made.save().expect("the vault saves");
    drop(made);

    let bytes = std::fs::read(&path).expect("the file is there");
    let database = keepass::Database::parse(
        &bytes[..],
        keepass::DatabaseKey::new().with_password(SECRET),
    )
    .expect("the file opens");

    for group in database.iter_all_groups() {
        assert!(group.times.expiry.is_some(), "a folder has no expiry date");
        assert_eq!(group.times.expires, Some(false));
    }
    for entry in database.iter_all_entries() {
        assert!(entry.times.expiry.is_some(), "an entry has no expiry date");
        assert_eq!(entry.times.expires, Some(false));
    }

    let meta = &database.meta;
    for (name, held) in [
        ("DatabaseNameChanged", meta.database_name_changed),
        (
            "DatabaseDescriptionChanged",
            meta.database_description_changed,
        ),
        ("DefaultUserNameChanged", meta.default_username_changed),
        ("MasterKeyChanged", meta.master_key_changed),
        ("RecycleBinChanged", meta.recyclebin_changed),
        (
            "EntryTemplatesGroupChanged",
            meta.entry_templates_group_changed,
        ),
        ("SettingsChanged", meta.settings_changed),
    ] {
        assert!(held.is_some(), "{name} is not in the file");
    }
}

/// An external implementation opens what Coffer made, and what Coffer then
/// saves over its own file is still whole.
///
/// The round-trip suite proves Coffer does not lose what somebody else wrote.
/// This is the other direction: that a file Coffer wrote from nothing is one
/// KeePassXC reads, and that Coffer's own second write does not disturb it.
#[test]
fn keepassxc_reads_a_vault_coffer_made_from_nothing() {
    let Some(tool) = crate::support::keepassxc_cli() else {
        return;
    };

    let (_scratch, path) = scratch();
    let mut made = make(&path, SECRET, "Work").expect("the vault is made");
    let root = made.tree().id;
    let folder = made
        .create_group(root, "Clients")
        .expect("the folder is made");
    let entry = made.create_entry(folder).expect("the entry is made");
    made.set_field(
        entry,
        keepass::db::fields::TITLE,
        vault_core::NewValue::Open("deploy".to_owned()),
    )
    .expect("the title is written");
    made.set_field(
        entry,
        keepass::db::fields::PASSWORD,
        vault_core::NewValue::Protected(zeroize::Zeroizing::new("hunter2".to_owned())),
    )
    .expect("the password is written");
    made.save().expect("the vault saves");
    drop(made);

    let listed = crate::support::entry_paths(&tool, &path, SECRET, None);
    assert_eq!(listed, vec!["Clients/deploy".to_owned()]);

    let before = crate::support::export(&tool, &path, SECRET, None);

    let mut again = open(&path, SECRET);
    again.save().expect("the vault saves again");
    drop(again);

    let after = crate::support::export(&tool, &path, SECRET, None);
    let differences = crate::normalise::differences(
        &crate::normalise::canonical(&before, false),
        &crate::normalise::canonical(&after, false),
    );
    assert!(
        differences.is_empty(),
        "a save over a vault Coffer made lost or altered a field:\n  {}",
        differences.join("\n  ")
    );
}
