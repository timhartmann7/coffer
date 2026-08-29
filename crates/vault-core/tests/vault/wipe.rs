//! What a lock leaves in memory.
//!
//! The slice's own acceptance criterion, checked from inside the process: put a
//! value into a database, let the database go, and count the copies of that
//! value this process is still holding. The answer has to be none.
//!
//! Every needle is read out of the fixture's source at run time. A string
//! written here would be a constant in the test binary, and the sweep would
//! find that constant and report a failure nobody could fix.
//!
//! The databases these tests open are deliberately crowded. macOS's own
//! allocator writes over a freed block up to about sixteen kilobytes, so a
//! small database leaves nothing behind whatever Coffer does and proves
//! nothing; the buffers a parser builds for a database of a few hundred entries
//! are far past that, and are what the allocator in `vault_core::scrub` exists
//! for. Linux zeroes nothing at any size.

use std::path::Path;

use keepass::db::fields;

use crate::scan::Sweep;
use crate::support::{BUILT_PASSWORD, RICH, SECRET, built, fixtures, open, scratch};

/// The first value the fixture marks as protected. The marker is a word out of
/// the format, so naming it here names nothing that is in anybody's database.
fn protected_value() -> String {
    body(r#"<Value ProtectInMemory="True">"#)
}

/// The first note in the fixture. Unprotected, so the library holds it in a
/// plain `String` that nothing upstream zeroizes.
fn open_value() -> String {
    body("<Key>Notes</Key><Value>")
}

fn body(after: &str) -> String {
    let source = std::fs::read_to_string(fixtures().join("sources/rich.xml"))
        .expect("the fixture's source is beside the fixture");
    let (_, rest) = source
        .split_once(after)
        .unwrap_or_else(|| panic!("the fixture's source still holds {after}"));
    let (value, _) = rest.split_once("</Value>").expect("the element closes");

    assert!(
        value.len() > 16 && !value.contains('&'),
        "a needle has to be long enough to be distinctive and literal enough to be found"
    );
    value.to_owned()
}

/// How many entries carry the needle. Enough that the decompressed database is
/// hundreds of kilobytes, which is where a freed block stops being something
/// the system allocator cleans up on its own.
const CROWD: usize = 600;

/// A database with the needle in every entry, protected and open.
fn crowded(directory: &Path, name: &str, protected: &str, plain: &str) -> std::path::PathBuf {
    built(directory, name, |database| {
        for index in 0..CROWD {
            database.root_mut().add_entry().edit(|entry| {
                entry.set_unprotected(fields::TITLE, format!("entry {index}"));
                entry.set_unprotected(fields::NOTES, plain);
                entry.set_protected(fields::PASSWORD, protected);
            });
        }
    })
}

/// Opens the database, does what a screen would do with it, and lets it go.
///
/// Deliberately not inlined: the point of the caller's stack wipe is to write
/// over the frames key derivation used, and it can only reach frames deeper
/// than its own.
#[inline(never)]
fn open_and_drop(path: &Path, secret: &str) {
    let vault = open(path, secret);
    let _ = vault.tree();
    drop(vault);
}

/// The positive control, and it comes first on purpose.
///
/// Every way this sweep can be broken - a region list that comes back empty, a
/// read that always fails, a needle that is never in the file - reports zero
/// hits, which is the same answer a perfect wipe gives. A test that only
/// asserted the zero would pass with the scanner switched off.
#[test]
fn a_value_in_an_open_database_is_there_to_be_found() {
    let (_scratch, path) = scratch(RICH);
    let vault = open(&path, SECRET);

    for needle in [protected_value(), open_value()] {
        let mut sweep = Sweep::for_needle(needle.into_bytes());
        assert!(
            sweep.hits() > 0,
            "the sweep found nothing while the database was open, so it would find \
             nothing whatever a lock did"
        );
    }

    drop(vault);
}

/// The criterion itself. Nothing on this path wipes anything of its own: the
/// database is written, parsed and dropped the way any code path drops one, and
/// the allocator underneath is the whole of what makes the answer zero.
#[test]
fn nothing_of_a_database_survives_it_being_dropped() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let protected = protected_value();
    let plain = open_value();
    let path = crowded(directory.path(), "crowded.kdbx", &protected, &plain);

    open_and_drop(&path, BUILT_PASSWORD);
    vault_core::scrub::stack();

    for needle in [protected, plain] {
        let mut sweep = Sweep::for_needle(needle.into_bytes());
        assert_eq!(
            sweep.hits(),
            0,
            "a value from the database is still in this process's memory"
        );
    }
}

/// A hundred locks and unlocks, which is the other half of the criterion. The
/// hundredth has to be as clean as the first.
#[test]
fn a_hundred_opens_and_closes_leave_nothing_behind() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let protected = protected_value();
    let plain = open_value();
    let path = crowded(directory.path(), "crowded.kdbx", &protected, &plain);

    for _ in 0..100 {
        open_and_drop(&path, BUILT_PASSWORD);
    }
    vault_core::scrub::stack();

    for needle in [protected, plain] {
        let mut sweep = Sweep::for_needle(needle.into_bytes());
        assert_eq!(sweep.hits(), 0, "a hundred cycles left a value behind");
    }
}

/// Reloading throws away one whole tree and reads another. It assigns into the
/// field rather than dropping the vault, which is the path a destructor on the
/// vault itself would never have seen.
#[test]
fn reloading_leaves_none_of_the_tree_it_replaced() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let gone = protected_value();
    let staying = open_value();

    let path = crowded(directory.path(), "reloaded.kdbx", &gone, &gone);
    let mut vault = open(&path, BUILT_PASSWORD);

    // Somebody else writes the file, and Coffer is asked to take their version.
    let replacement = crowded(directory.path(), "replacement.kdbx", &staying, &staying);
    std::fs::copy(&replacement, &path).expect("the replacement lands on the database");
    vault
        .reload()
        .expect("the replacement opens with the same password");

    let mut sweep = Sweep::for_needle(gone.into_bytes());
    assert_eq!(
        sweep.hits(),
        0,
        "the tree the reload replaced is still in memory"
    );
    drop(vault);
}

/// Reading what is on disk to answer the conflict dialog decrypts a second
/// whole database and throws it away. That copy is nobody's vault and is wiped
/// on the same terms.
#[test]
fn reading_the_file_on_disk_leaves_none_of_it() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let held = protected_value();
    let written = open_value();

    let path = crowded(directory.path(), "held.kdbx", &held, &held);
    let vault = open(&path, BUILT_PASSWORD);

    let replacement = crowded(directory.path(), "written.kdbx", &written, &written);
    std::fs::copy(&replacement, &path).expect("the replacement lands on the database");

    let found = vault.rival();
    assert!(found.entries.is_some(), "the file on disk opens");

    let mut sweep = Sweep::for_needle(written.into_bytes());
    assert_eq!(
        sweep.hits(),
        0,
        "the version on disk is still in memory after being read"
    );
    drop(vault);
}
