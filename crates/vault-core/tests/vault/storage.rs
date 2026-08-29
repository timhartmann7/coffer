//! The filesystem side: atomic writes, snapshots, the lock file, and noticing
//! that somebody else wrote the database.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use vault_core::storage::atomic::write_atomic;
use vault_core::storage::lock::{Lock, Outcome};
use vault_core::storage::watch::{Change, Stamp};
use vault_core::storage::{snapshot, watch};

use crate::support::{self, BUILT_PASSWORD, built, open, permissions_apply};

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
        assert_eq!(holder.user, std::env::var("USER").unwrap_or_default());
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
    let mut child = std::process::Command::new("/bin/sh")
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
    assert_eq!(
        watch::since(&target, stamp).expect("it compares"),
        Change::None
    );

    write(&target, "two");
    assert_eq!(
        watch::since(&target, stamp).expect("it compares"),
        Change::Modified
    );

    std::fs::remove_file(&target).expect("the file goes");
    assert_eq!(
        watch::since(&target, stamp).expect("it compares"),
        Change::Gone
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
        watch::since(&target, stamp).expect("it compares"),
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
