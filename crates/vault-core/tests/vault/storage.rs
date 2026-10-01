//! The filesystem side: atomic writes, snapshots, the lock file, and noticing
//! that somebody else wrote the database.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use vault_core::storage::atomic::write_atomic;
use vault_core::storage::lock::{Lock, Outcome};
use vault_core::storage::watch::{Change, Stamp};
use vault_core::storage::{snapshot, watch};

use crate::support::{self, BUILT_PASSWORD, SECRET, built, open, permissions_apply};

/// Set in the child process the kill test spawns; holds the path to write.
const CHILD_ABORT: &str = "COFFER_CHILD_ABORT_PATH";

fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("the file is written");
}

#[test]
fn an_atomic_write_replaces_the_file_and_leaves_no_temporary_behind() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let target = scratch.path().join("db.kdbx");
    write(&target, "before");

    write_atomic::<std::io::Error, _>(&target, |writer: &mut dyn Write| writer.write_all(b"after"))
        .expect("the write succeeds");

    assert_eq!(std::fs::read_to_string(&target).expect("it reads"), "after");
    assert_eq!(
        std::fs::read_dir(scratch.path())
            .expect("the directory reads")
            .count(),
        1
    );
    assert_eq!(
        std::fs::metadata(&target)
            .expect("it is there")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn a_write_that_fails_part_way_leaves_the_original_untouched() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let target = scratch.path().join("db.kdbx");
    write(&target, "before");

    let outcome = write_atomic::<std::io::Error, _>(&target, |writer: &mut dyn Write| {
        writer.write_all(b"half a database")?;
        // What a full volume looks like from inside the writer.
        Err(std::io::Error::from_raw_os_error(libc::ENOSPC))
    });

    assert!(outcome.is_err());
    assert_eq!(
        std::fs::read_to_string(&target).expect("it reads"),
        "before"
    );
    assert_eq!(
        std::fs::read_dir(scratch.path())
            .expect("the directory reads")
            .count(),
        1,
        "a temporary file survived a failed write"
    );
}

#[test]
fn killing_the_process_between_the_temporary_file_and_the_rename_leaves_the_original() {
    if let Some(path) = std::env::var_os(CHILD_ABORT) {
        // This process is the child. It fills the temporary file and dies
        // before the rename, which is the moment the criterion is about.
        let path = PathBuf::from(path);
        let _ = write_atomic::<std::io::Error, _>(&path, |writer: &mut dyn Write| {
            writer.write_all(b"half a database")?;
            writer.flush()?;
            std::process::abort();
        });
        unreachable!("the child aborts inside the writer");
    }

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let target = scratch.path().join("db.kdbx");
    write(&target, "the original database");

    let status = std::process::Command::new(
        std::env::current_exe().expect("the test binary knows its own path"),
    )
    .args([
        "--exact",
        "storage::killing_the_process_between_the_temporary_file_and_the_rename_leaves_the_original",
    ])
    .env(CHILD_ABORT, &target)
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status()
    .expect("the child runs");

    assert!(!status.success(), "the child was supposed to die");

    assert_eq!(
        std::fs::read_to_string(&target).expect("the original is still there"),
        "the original database"
    );

    let leftovers: Vec<PathBuf> = std::fs::read_dir(scratch.path())
        .expect("the directory reads")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path != &target)
        .collect();
    assert_eq!(
        leftovers.len(),
        1,
        "the killed write should leave exactly its temporary file behind"
    );

    // The next successful write sweeps what the dead process left.
    write_atomic::<std::io::Error, _>(&target, |writer: &mut dyn Write| {
        writer.write_all(b"a whole database")
    })
    .expect("the write succeeds");

    assert_eq!(
        std::fs::read_dir(scratch.path())
            .expect("the directory reads")
            .count(),
        1,
        "the abandoned temporary file was not swept"
    );
}

#[test]
fn the_lock_names_this_process_and_goes_when_the_vault_does() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "locked.kdbx", |_| {});
    let lock_path = scratch.path().join("locked.kdbx.lock");

    {
        let _vault = open(&database, BUILT_PASSWORD);
        assert!(lock_path.exists(), "no lock was written");

        let holder = vault_core::storage::lock::inspect(&database)
            .expect("the lock reads")
            .expect("a lock is there");
        assert_eq!(holder.pid, Some(std::process::id()));
        assert!(!holder.host.is_empty(), "the lock should name the machine");
        assert!(
            !holder.time.is_empty(),
            "the lock should say when it was taken"
        );
        // Not the environment recomputed: which variable wins, and what the
        // engine falls back to when neither is set, is the engine's rule, and a
        // test holding its own copy would agree with itself. Under a container,
        // which sets neither, that copy was `""` and this compared nothing to
        // nothing.
        assert!(
            !holder.user.is_empty(),
            "the lock does not name the account holding it"
        );
    }

    assert!(!lock_path.exists(), "the lock outlived the vault");
}

#[test]
fn a_lock_held_by_a_living_process_is_reported_and_left_alone() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "contested.kdbx", |_| {});

    let held = Lock::acquire(&database).expect("the lock is taken");
    assert!(matches!(held, Outcome::Taken(_)));

    match Lock::acquire(&database).expect("the second attempt reads the lock") {
        Outcome::Held(holder) => {
            assert_eq!(holder.pid, Some(std::process::id()));
            // The point of a lock file is to say who is holding the database
            // and since when, so all three have to survive the trip.
            let described = format!("{holder:?}");
            assert!(described.contains(&holder.time), "{described}");
            assert!(described.contains(&holder.host), "{described}");
        }
        Outcome::Taken(_) => panic!("the lock was taken twice"),
        Outcome::Unwritable => panic!("a writable scratch directory refused the lock"),
    }

    let error = vault_core::Vault::open(
        &database,
        support::password(BUILT_PASSWORD),
        vault_core::LockPolicy::Respect,
    )
    .expect_err("opening respects the lock");
    assert!(matches!(error, vault_core::VaultError::Locked(_)));
}

#[test]
fn a_lock_left_by_a_process_that_no_longer_exists_is_replaced() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "stale.kdbx", |_| {});
    let lock_path = scratch.path().join("stale.kdbx.lock");

    // A process id that has certainly gone: a child that has already been
    // reaped. The host has to match for staleness to be decidable, and the only
    // honest source for it is the lock Coffer itself writes.
    let mut child = std::process::Command::new("sh")
        .args(["-c", "exit 0"])
        .spawn()
        .expect("the child runs");
    let dead = child.id();
    child.wait().expect("the child is reaped");

    let host = {
        let _ours = Lock::acquire(&database).expect("the lock is taken");
        vault_core::storage::lock::inspect(&database)
            .expect("the lock reads")
            .expect("a lock is there")
            .host
    };
    write(
        &lock_path,
        &format!(
            "[Lock]\nTime=2020-01-01T00:00:00Z\nUserName=someone\nMachine={host}\nPID={dead}\n"
        ),
    );

    match Lock::acquire(&database).expect("the stale lock is cleared") {
        Outcome::Taken(_) => {}
        Outcome::Held(holder) => panic!("a dead process still holds the lock: {holder:?}"),
        Outcome::Unwritable => panic!("a writable scratch directory refused the lock"),
    }
}

#[test]
fn dropping_one_lock_leaves_another_in_the_same_process_alone() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let first = built(scratch.path(), "first.kdbx", |_| {});
    let second = built(scratch.path(), "second.kdbx", |_| {});

    let held = Lock::acquire(&second).expect("the second lock is taken");
    assert!(matches!(held, Outcome::Taken(_)));

    {
        let _passing = Lock::acquire(&first).expect("the first lock is taken");
    }

    assert!(
        vault_core::storage::lock::inspect(&second)
            .expect("the lock reads")
            .is_some(),
        "dropping one lock removed another one this process still holds"
    );
}

#[test]
fn a_lock_from_another_machine_is_believed_whatever_its_process_id_says() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "remote.kdbx", |_| {});
    let lock_path = scratch.path().join("remote.kdbx.lock");

    write(
        &lock_path,
        "[Lock]\nTime=2020-01-01T00:00:00Z\nUserName=someone\nMachine=another-machine\nPID=0\n",
    );

    match Lock::acquire(&database).expect("the lock reads") {
        Outcome::Held(holder) => assert_eq!(holder.host, "another-machine"),
        Outcome::Taken(_) => panic!("a lock from another machine was cleared"),
        Outcome::Unwritable => panic!("a writable scratch directory refused the lock"),
    }
}

#[test]
fn taking_over_opens_a_database_somebody_else_has_open() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "shared.kdbx", |_| {});

    let _held = Lock::acquire(&database).expect("the lock is taken");

    vault_core::Vault::open(
        &database,
        support::password(BUILT_PASSWORD),
        vault_core::LockPolicy::TakeOver,
    )
    .expect("taking over opens it anyway");
}

#[test]
fn the_change_watcher_tells_the_three_states_apart() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let target = scratch.path().join("db.kdbx");
    write(&target, "one");

    let stamp = Stamp::of(&target).expect("the stamp reads");
    let content = watch::digest(b"one");
    assert_eq!(
        watch::since(&target, stamp, content).expect("it compares"),
        Change::None
    );

    write(&target, "two");
    assert_eq!(
        watch::since(&target, stamp, content).expect("it compares"),
        Change::Modified
    );

    std::fs::remove_file(&target).expect("the file goes");
    assert_eq!(
        watch::since(&target, stamp, content).expect("it compares"),
        Change::Gone
    );
}

/// The two writes a stamp cannot tell apart from the outside, and the bytes can.
///
/// An extended attribute the system wrote leaves the file's content alone; a
/// client that rewrote it in place changed the content and can put every
/// timestamp back. On Linux it does not even have to: both times come from a
/// clock that moves once a timer tick, so a rewrite of the same length inside
/// one tick is invisible in metadata. Neither case may be decided by a clock.
#[test]
fn only_the_bytes_tell_an_attribute_from_a_write_that_hid_itself() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let target = scratch.path().join("db.kdbx");
    write(&target, "one");

    let stamp = Stamp::of(&target).expect("the stamp reads");
    let content = watch::digest(b"one");

    support::set_attribute(&target);
    assert_eq!(
        watch::since(&target, stamp, content).expect("it compares"),
        Change::None,
        "an attribute the system wrote was taken for somebody else's edit"
    );

    // The same shape from the outside, and different bytes: a client that wrote
    // in place and put the modification time back where it found it. A stamp
    // carries no timestamp today, so putting it back decides nothing on its
    // own; it stays here so that a stamp which ever grows one again is still
    // caught by the bytes rather than passing on a clock.
    let reference = scratch.path().join("when");
    write(&reference, "");
    support::copy_modification_time(&target, &reference);
    write(&target, "two");
    support::copy_modification_time(&reference, &target);

    // Both writes are three bytes on purpose, and this is what says so. A
    // second write of another length would be caught by the stamp, the
    // assertion below would still pass, and the digest this test exists for
    // would never be reached.
    assert_eq!(
        Stamp::of(&target).expect("the stamp reads"),
        stamp,
        "the second write changed the file's shape, so the bytes were never asked"
    );

    assert_eq!(
        watch::since(&target, stamp, content).expect("it compares"),
        Change::Modified,
        "a write that changed the bytes and nothing else went unnoticed"
    );
}

#[test]
fn a_replacement_of_the_same_size_is_still_a_change() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let target = scratch.path().join("db.kdbx");
    write(&target, "one");
    let stamp = Stamp::of(&target).expect("the stamp reads");

    // Another client's atomic save: same bytes, same size, a new inode.
    let replacement = scratch.path().join("other");
    write(&replacement, "one");
    std::fs::rename(&replacement, &target).expect("the file is replaced");

    assert_eq!(
        watch::since(&target, stamp, watch::digest(b"one")).expect("it compares"),
        Change::Modified,
        "a replacement that kept the size and the contents went unnoticed"
    );
}

#[test]
fn a_database_reached_through_a_symbolic_link_is_saved_through_it() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let real = built(scratch.path(), "real.kdbx", |_| {});
    let link = scratch.path().join("link.kdbx");
    std::os::unix::fs::symlink(&real, &link).expect("the link is made");

    let mut vault = open(&link, BUILT_PASSWORD);
    assert_eq!(
        vault.path(),
        real.canonicalize().expect("the real path resolves"),
        "the vault should work on the file the link names"
    );
    vault.save().expect("the database saves");
    drop(vault);

    assert!(
        std::fs::symlink_metadata(&link)
            .expect("the link is there")
            .file_type()
            .is_symlink(),
        "the save replaced the symbolic link instead of writing through it"
    );
    open(&link, BUILT_PASSWORD);
}

#[test]
fn a_directory_nobody_can_write_to_stops_the_save() {
    if !permissions_apply() {
        return;
    }

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "frozen.kdbx", |_| {});
    let mut vault = open(&database, BUILT_PASSWORD);

    std::fs::set_permissions(scratch.path(), std::fs::Permissions::from_mode(0o500))
        .expect("the directory is frozen");

    let outcome = vault.save();

    std::fs::set_permissions(scratch.path(), std::fs::Permissions::from_mode(0o700))
        .expect("the directory thaws");

    assert!(
        matches!(outcome, Err(vault_core::VaultError::Io(_))),
        "a directory that cannot be written should stop the save"
    );
    assert!(
        !snapshot::slot(&database, 1)
            .expect("the slot has a path")
            .exists()
    );
}

#[test]
fn a_path_that_names_a_directory_is_refused() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let error = vault_core::Vault::open(
        scratch.path(),
        support::password("anything"),
        vault_core::LockPolicy::Respect,
    )
    .expect_err("a directory is not a database");

    assert!(matches!(error, vault_core::VaultError::Io(_)));
}

#[test]
fn a_database_that_was_never_saved_has_no_snapshots() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = scratch.path().join("db.kdbx");
    write(&database, "a file");

    assert_eq!(snapshot::taken(&database).expect("the chain reads"), vec![]);
}

#[test]
fn every_snapshot_is_offered_newest_first() {
    let (_scratch, database) = support::scratch(support::RICH);
    let mut vault = open(&database, support::SECRET);

    for _ in 0..3 {
        vault.save().expect("the database saves");
    }

    let taken = snapshot::taken(&database).expect("the chain reads");
    assert_eq!(
        taken
            .iter()
            .map(|snapshot| snapshot.index)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    for snapshot in &taken {
        assert!(snapshot.path.exists());
        assert!(snapshot.taken.is_some());
    }
}

/// The chain is written from the oldest slot towards the newest, so a process
/// killed part way through can leave a hole in it. A hole is not a reason to
/// stop offering the snapshots that are there.
#[test]
fn a_hole_in_the_chain_does_not_hide_the_rest_of_it() {
    let (_scratch, database) = support::scratch(support::RICH);
    let mut vault = open(&database, support::SECRET);

    for _ in 0..3 {
        vault.save().expect("the database saves");
    }

    std::fs::remove_file(snapshot::slot(&database, 2).expect("the slot has a path"))
        .expect("the snapshot is removed");

    let taken = snapshot::taken(&database).expect("the chain reads");
    assert_eq!(
        taken
            .iter()
            .map(|snapshot| snapshot.index)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
}

/// Somebody can put anything beside a database, and a directory named like a
/// snapshot is not one.
#[test]
fn something_that_is_not_a_file_is_not_a_snapshot() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = scratch.path().join("db.kdbx");
    write(&database, "a file");
    std::fs::create_dir(snapshot::slot(&database, 1).expect("the slot has a path"))
        .expect("the directory is made");

    assert_eq!(snapshot::taken(&database).expect("the chain reads"), vec![]);
}

/// Reading a database needs no write, and a vault kept somewhere that will not
/// take one used to be a vault nobody could open: a read-only disk image, a
/// Time Machine snapshot, a stick macOS mounted read-only, a share the reader
/// may only read. The advisory note beside the file is not worth the file.
#[test]
fn a_vault_where_no_lock_file_can_be_written_opens_to_be_read() {
    if !permissions_apply() {
        return;
    }

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "elsewhere.kdbx", |db| {
        db.root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(keepass::db::fields::TITLE, "kept"));
    });

    let _frozen = support::Frozen::over(scratch.path());

    let mut vault = open(&database, BUILT_PASSWORD);
    assert_eq!(
        vault.tree().entries.len(),
        1,
        "the entries are not readable"
    );
    assert!(vault.is_read_only());
    assert!(
        matches!(vault.save(), Err(vault_core::VaultError::ReadOnlyPlace)),
        "a save aimed at a place that refuses writes said something else"
    );
    assert!(
        !std::path::Path::new(&format!("{}.lock", database.display())).exists(),
        "a lock file was written where nothing can be"
    );
}

/// The way off the medium. A copy goes somewhere the reader can write, and it
/// is the whole reason opening a read-only vault is worth anything.
#[test]
fn a_vault_that_cannot_be_locked_still_writes_a_copy_somewhere_it_can() {
    if !permissions_apply() {
        return;
    }

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let elsewhere = tempfile::tempdir().expect("a second scratch directory");
    let database = built(scratch.path(), "readonly.kdbx", |db| {
        db.root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(keepass::db::fields::TITLE, "kept"));
    });

    let _frozen = support::Frozen::over(scratch.path());

    let mut vault = open(&database, BUILT_PASSWORD);
    let copy = elsewhere.path().join("rescued.kdbx");
    vault.save_copy(&copy).expect("the copy is written");
    drop(vault);

    let again = open(&copy, BUILT_PASSWORD);
    assert_eq!(again.tree().entries.len(), 1);
    assert!(
        !again.is_read_only(),
        "the copy inherited the medium the original was on"
    );
}

/// A lock somebody is holding still wins over the medium. The order of the two
/// answers inside `acquire` is what decides this, and getting it the other way
/// round would let a second Coffer read a vault the first has open and report
/// nothing about it.
#[test]
fn a_lock_somebody_is_holding_is_believed_where_nothing_can_be_written() {
    if !permissions_apply() {
        return;
    }

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "contested.kdbx", |_| {});
    let held = open(&database, BUILT_PASSWORD);

    let _frozen = support::Frozen::over(scratch.path());

    let error = vault_core::Vault::open(
        &database,
        support::password(BUILT_PASSWORD),
        vault_core::LockPolicy::Respect,
    )
    .expect_err("a held lock is still a held lock");
    assert!(matches!(error, vault_core::VaultError::Locked(_)));

    drop(held);
}

/// The format outranks the place. A KDBX 3 database carrying attachments is
/// refused a read of those attachments whatever it is kept on, because the
/// reason is what the format can hold and not where the file sits.
#[test]
fn a_format_that_cannot_be_read_back_is_still_that_wherever_it_is_kept() {
    if !permissions_apply() {
        return;
    }

    let (scratch, database) = support::scratch("rich-kdbx31.kdbx");
    let _frozen = support::Frozen::over(scratch.path());

    let mut vault = open(&database, SECRET);
    assert!(vault.is_read_only());
    assert!(
        matches!(
            vault.save(),
            Err(vault_core::VaultError::ReadOnlyKdbx3Attachments)
        ),
        "the medium answered for a format that cannot be written back"
    );
}

/// Reading the file again does not thaw it. `reload` settles what the database
/// is all over again, and a vault that came back writable would offer a save
/// that could never reach the file.
#[test]
fn a_vault_kept_where_nothing_can_be_written_stays_that_way_when_it_is_read_again() {
    if !permissions_apply() {
        return;
    }

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = built(scratch.path(), "reloaded.kdbx", |_| {});
    let _frozen = support::Frozen::over(scratch.path());

    let mut vault = open(&database, BUILT_PASSWORD);
    vault.reload().expect("the file reads again");
    assert!(vault.is_read_only(), "a reload thawed the medium");
}

/// The screen is told a place is taken before anybody types a password, and the
/// creation is refused by the reservation. The two answers have to be the same
/// answer, or the screen offers a place the creation then refuses - or refuses
/// one it would have taken.
#[test]
fn a_name_is_taken_exactly_when_it_cannot_be_reserved() {
    use vault_core::storage::atomic::{reserve, taken};

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let at = |name: &str| scratch.path().join(name);

    write(&at("a-vault.kdbx"), "somebody's vault");
    write(&at("empty.kdbx"), "");
    std::fs::create_dir(at("a-folder.kdbx")).expect("the folder is made");
    std::os::unix::fs::symlink(at("nowhere.kdbx"), at("dangling.kdbx")).expect("a link");
    std::os::unix::fs::symlink(at("a-vault.kdbx"), at("linked.kdbx")).expect("a link");

    for name in [
        "a-vault.kdbx",
        "empty.kdbx",
        "a-folder.kdbx",
        "dangling.kdbx",
        "linked.kdbx",
    ] {
        assert!(taken(&at(name)), "{name} is not reported as taken");
        assert!(reserve(&at(name)).is_err(), "{name} was reserved over");
    }

    let free = at("free.kdbx");
    assert!(!taken(&free));
    reserve(&free).expect("a free name is reserved");
    assert!(taken(&free), "a reserved name is not reported as taken");

    // A link that led nowhere is still nothing but a link: the reservation did
    // not follow it and make the file it named.
    assert!(!at("nowhere.kdbx").exists());
    assert_eq!(
        std::fs::read_to_string(at("a-vault.kdbx")).expect("it reads"),
        "somebody's vault"
    );
}

/// The files Coffer keeps beside a vault open with its password and are not
/// the vault. The shape of the name is the whole of the rule, so it is pinned
/// here from both sides.
#[test]
fn only_the_names_coffer_keeps_beside_a_vault_are_reserved() {
    use vault_core::storage::reserved;

    for name in [
        "vault.kdbx.1.bak",
        "vault.kdbx.10.bak",
        "personal.7.bak",
        "vault.kdbx.unsaved.kdbx",
        "vault.kdbx.1.bak.unsaved.kdbx",
    ] {
        assert!(reserved(Path::new(name)), "{name} is not reserved");
    }

    for name in [
        "vault.kdbx",
        "vault.kdbx.11.bak",
        "vault.kdbx.0.bak",
        ".unsaved.kdbx",
        "unsaved.kdbx",
        "vault.unsaved",
        "",
    ] {
        assert!(!reserved(Path::new(name)), "{name} is reserved");
    }
}
