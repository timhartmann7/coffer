//! A file offered to an entry under a name the entry already gives another.
//!
//! Every phone calls every scan `Scanned Document.pdf` and every SSH key is
//! `id_ed25519`, and the second page of a passport used to take the place of the
//! first without a word. What is tested here is that nothing is ever lost to a
//! name: the offer alone changes nothing, and each of the two answers keeps
//! exactly what it says it keeps, through the disk and back.

use std::path::{Path, PathBuf};

use keepass::db::{Value, fields};
use vault_core::model::EntryId;
use vault_core::{Attached, NewValue, Rescue, VaultError};

use crate::support::{self, BUILT_PASSWORD, built, entry_titled, files, open};

const SCAN: &str = "Scanned Document.pdf";

/// A passport entry carrying its first page, and a neighbour whose own scan of
/// the same name sits beside it in the pool. Written and read back once, so the
/// pool is numbered the way a database from disk is.
fn passport(directory: &Path) -> PathBuf {
    let path = built(directory, "passport.kdbx", |database| {
        for (title, page) in [
            ("passport", b"page one".as_slice()),
            ("neighbour", b"theirs"),
        ] {
            database.root_mut().add_entry().edit(|entry| {
                entry.set_unprotected(fields::TITLE, title);
                entry.add_attachment(SCAN, Value::unprotected(page.to_vec()));
            });
        }
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    vault.save().expect("the database saves");
    path
}

fn id_of(vault: &vault_core::Vault, title: &str) -> EntryId {
    entry_titled(vault, title).id
}

fn owned(files: &[(&str, &str, &[u8])]) -> Vec<(String, String, Vec<u8>)> {
    let mut owned: Vec<(String, String, Vec<u8>)> = files
        .iter()
        .map(|(title, name, bytes)| ((*title).to_owned(), (*name).to_owned(), bytes.to_vec()))
        .collect();
    owned.sort();
    owned
}

/// The defect itself. The offer says what is there and what the new file could
/// be called, and does nothing else: not to the files, not to the history, and
/// not to whether the vault thinks it has something to write.
#[test]
fn a_file_under_a_name_the_entry_uses_is_asked_about_and_changes_nothing() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = passport(scratch.path());

    let before;
    {
        let mut vault = open(&path, BUILT_PASSWORD);
        let id = id_of(&vault, "passport");
        before = files(&vault);
        let versions = vault.versions(id).len();

        let asked = vault
            .add_attachment(id, SCAN, b"page two")
            .expect("the file is offerable");
        let Attached::Taken(clash) = asked else {
            panic!("the second page went over the first without a word");
        };
        assert_eq!(
            clash.size,
            b"page one".len(),
            "the size is the one already there"
        );
        assert_eq!(clash.free, "Scanned Document 2.pdf");

        assert_eq!(files(&vault), before);
        assert_eq!(vault.versions(id).len(), versions);
        assert_eq!(
            vault.rescue(),
            Rescue::Nothing,
            "a question the reader has not answered left the vault with something to write"
        );
    }

    assert_eq!(files(&open(&path, BUILT_PASSWORD)), before);
}

/// The default answer. The old file stays exactly where it was, the new one goes
/// beside it, and the neighbour's file of the same name is nobody's business.
#[test]
fn keeping_both_puts_the_new_file_beside_the_old_one() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = passport(scratch.path());

    {
        let mut vault = open(&path, BUILT_PASSWORD);
        let id = id_of(&vault, "passport");
        let versions = vault.versions(id).len();

        let _ = vault.add_attachment(id, SCAN, b"page two");
        vault
            .keep_both(id, SCAN, b"page two")
            .expect("both are kept");

        // Files are not part of an entry's history, either way in.
        assert_eq!(vault.versions(id).len(), versions);
        vault.save().expect("the database saves");
    }

    assert_eq!(
        files(&open(&path, BUILT_PASSWORD)),
        owned(&[
            ("neighbour", SCAN, b"theirs"),
            ("passport", SCAN, b"page one"),
            ("passport", "Scanned Document 2.pdf", b"page two"),
        ])
    );
}

/// The destructive answer takes the old file and nothing else.
#[test]
fn replacing_takes_the_old_file_and_leaves_the_neighbours_alone() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = passport(scratch.path());

    {
        let mut vault = open(&path, BUILT_PASSWORD);
        let id = id_of(&vault, "passport");
        let _ = vault.add_attachment(id, SCAN, b"page two");
        vault
            .replace_attachment(id, SCAN, b"page two")
            .expect("the file is replaced");
        vault.save().expect("the database saves");
    }

    assert_eq!(
        files(&open(&path, BUILT_PASSWORD)),
        owned(&[
            ("neighbour", SCAN, b"theirs"),
            ("passport", SCAN, b"page two"),
        ])
    );
}

/// Every shape a real name takes, kept beside itself and read back off the
/// disk: the number before the last extension or at the end, and the name
/// stored as text - slashes, dots and a right-to-left override included -
/// because nothing turns an attachment name into a path here.
#[test]
fn every_shape_of_name_is_kept_beside_itself_and_survives_the_disk() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "names.kdbx", |_| {});

    let shapes = [
        (SCAN, "Scanned Document 2.pdf"),
        ("id_ed25519", "id_ed25519 2"),
        (".env", ".env 2"),
        ("archive.tar.gz", "archive.tar 2.gz"),
        ("Scan 2.pdf", "Scan 2 2.pdf"),
        ("Dr. Smith letter", "Dr. Smith letter 2"),
        ("../../escape.txt", "../../escape 2.txt"),
        ("evil\u{202e}fdp.exe", "evil\u{202e}fdp 2.exe"),
        ("nothing.txt", "nothing 2.txt"),
    ];

    let mut wanted: Vec<(&str, &str, &[u8])> = Vec::new();
    {
        let mut vault = open(&path, BUILT_PASSWORD);
        let root = vault.tree().id;
        let id = vault.create_entry(root).expect("the entry is made");
        vault
            .set_field(id, fields::TITLE, NewValue::Open("names".to_owned()))
            .expect("the title is written");

        for (name, free) in shapes {
            // A file with no bytes in it is a file like any other, and the
            // size the question reports for it is nothing at all.
            let first: &[u8] = if name == "nothing.txt" { b"" } else { b"first" };
            support::attach(&mut vault, id, name, first);

            let asked = vault
                .add_attachment(id, name, b"second")
                .expect("offerable");
            assert_eq!(
                asked,
                Attached::Taken(vault_core::Clash {
                    size: first.len(),
                    free: free.to_owned()
                }),
                "{name:?}"
            );
            vault.keep_both(id, name, b"second").expect("both are kept");

            wanted.push(("names", name, first));
            wanted.push(("names", free, b"second"));
        }
        vault.save().expect("the database saves");
    }

    assert_eq!(files(&open(&path, BUILT_PASSWORD)), owned(&wanted));
}

/// The number is found among every name on the entry, and a name that differs
/// from it only in letter case counts as taken: the format would hold both,
/// and a Mac's disk would not let the reader write them out side by side.
#[test]
fn the_name_kept_beside_passes_over_names_already_taken_in_any_case() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "taken.kdbx", |_| {});
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let id = vault.create_entry(root).expect("the entry is made");

    support::attach(&mut vault, id, "Scan.pdf", b"one");
    support::attach(&mut vault, id, "scan 2.PDF", b"two");

    let Ok(Attached::Taken(clash)) = vault.add_attachment(id, "Scan.pdf", b"three") else {
        panic!("the name is taken");
    };
    assert_eq!(clash.free, "Scan 3.pdf");

    // Thirty more of the same scan: every one finds the next number, and none
    // lands on a name already there.
    for round in 0..30u8 {
        vault
            .keep_both(id, "Scan.pdf", &[round])
            .expect("both are kept");
    }
    let names: Vec<String> = vault
        .entry(id)
        .expect("the entry is there")
        .attachments
        .into_iter()
        .map(|attachment| attachment.name)
        .collect();
    assert_eq!(names.len(), 32);
    let lowered: std::collections::HashSet<String> =
        names.iter().map(|name| name.to_lowercase()).collect();
    assert_eq!(
        lowered.len(),
        32,
        "two names differ only in case: {names:?}"
    );
    assert!(names.contains(&"Scan 32.pdf".to_owned()), "{names:?}");
}

/// The answer comes after the question, and the entry can change in between.
/// The name is worked out again when the answer arrives: one taken since is
/// passed over, and one that has come free is simply the file's own.
#[test]
fn keeping_both_works_the_name_out_again_when_the_answer_comes() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = passport(scratch.path());
    let mut vault = open(&path, BUILT_PASSWORD);
    let id = id_of(&vault, "passport");

    let _ = vault.add_attachment(id, SCAN, b"page two");
    support::attach(
        &mut vault,
        id,
        "Scanned Document 2.pdf",
        b"arrived meanwhile",
    );
    vault
        .keep_both(id, SCAN, b"page two")
        .expect("both are kept");
    assert_eq!(
        vault
            .attachment(id, "Scanned Document 3.pdf")
            .expect("it went under the next number")
            .expose(),
        b"page two"
    );

    let _ = vault.add_attachment(id, SCAN, b"page three");
    vault
        .remove_attachment(id, SCAN)
        .expect("the first page goes meanwhile");
    vault
        .keep_both(id, SCAN, b"page three")
        .expect("both are kept");
    assert_eq!(
        vault
            .attachment(id, SCAN)
            .expect("it went under its own name")
            .expose(),
        b"page three"
    );
}

/// A replacement is a removal first, and it is refused for the same reason one
/// is: an earlier version holds the file there in place. Refused, it costs
/// nothing - and the answer that loses nothing is still open to the reader.
#[test]
fn a_replacement_earlier_versions_stand_in_the_way_of_costs_nothing() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "held.kdbx", |_| {});

    {
        let mut vault = open(&path, BUILT_PASSWORD);
        let root = vault.tree().id;
        let id = vault.create_entry(root).expect("the entry is made");
        vault
            .set_field(id, fields::TITLE, NewValue::Open("held".to_owned()))
            .expect("the title is written");
        support::attach(&mut vault, id, SCAN, b"page one");
        // A version written after the file went on names it.
        vault
            .set_field(id, fields::NOTES, NewValue::Open("renewed".to_owned()))
            .expect("the note is written");
        vault.save().expect("the database saves");
    }

    let mut vault = open(&path, BUILT_PASSWORD);
    let id = id_of(&vault, "held");
    let before = files(&vault);
    let versions = vault.versions(id);

    let _ = vault.add_attachment(id, SCAN, b"page two");
    assert!(matches!(
        vault.replace_attachment(id, SCAN, b"page two"),
        Err(VaultError::AttachmentInHistory)
    ));
    assert_eq!(
        files(&vault),
        before,
        "a refused replacement took something"
    );
    assert_eq!(
        vault.versions(id),
        versions,
        "a refused replacement took a version"
    );

    vault
        .keep_both(id, SCAN, b"page two")
        .expect("keeping both needs nothing to go");
    vault.save().expect("the database saves");
    drop(vault);

    assert_eq!(
        files(&open(&path, BUILT_PASSWORD)),
        owned(&[
            ("held", SCAN, b"page one"),
            ("held", "Scanned Document 2.pdf", b"page two"),
        ])
    );
}

/// The format tells `Scan.pdf` from `scan.pdf`, and so does every KeePass
/// client: a name that differs only in letter case is a second file, and adding
/// it takes nothing away, so it is not asked about.
#[test]
fn a_name_that_differs_only_in_letter_case_is_a_second_file() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "case.kdbx", |_| {});

    {
        let mut vault = open(&path, BUILT_PASSWORD);
        let root = vault.tree().id;
        let id = vault.create_entry(root).expect("the entry is made");
        vault
            .set_field(id, fields::TITLE, NewValue::Open("case".to_owned()))
            .expect("the title is written");
        support::attach(&mut vault, id, "Scan.pdf", b"upper");
        assert!(matches!(
            vault.add_attachment(id, "scan.pdf", b"lower"),
            Ok(Attached::Added)
        ));
        vault.save().expect("the database saves");
    }

    assert_eq!(
        files(&open(&path, BUILT_PASSWORD)),
        owned(&[
            ("case", "Scan.pdf", b"upper"),
            ("case", "scan.pdf", b"lower")
        ])
    );
}

/// A file another client took off an entry can still be named by one of the
/// entry's earlier versions. The question is about the files the entry has
/// now: the name is free on it, and the version goes on holding the bytes it
/// always held.
///
/// From KeePassXC, because Coffer cannot make this shape: it refuses to take a
/// file off an entry while a version still holds it.
#[test]
fn a_name_only_an_earlier_version_gives_a_file_is_free_on_the_entry() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let scratch = tempfile::tempdir().expect("a scratch directory");

    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<KeePassFile>
  <Meta>
    <Generator>coffer-test</Generator>
    <Binaries>
      <Binary ID="0" Compressed="False">cGFnZSBvbmU=</Binary>
    </Binaries>
  </Meta>
  <Root>
    <Group>
      <UUID>aGlzdG9yeS1yb290LS0tLQ==</UUID>
      <Name>history</Name>
      <Times><CreationTime>2020-01-01T00:00:00Z</CreationTime><Expires>False</Expires></Times>
      <Entry>
        <UUID>c3ViamVjdC1lbnRyeS0tLQ==</UUID>
        <Times><CreationTime>2020-01-01T00:00:00Z</CreationTime><LastModificationTime>2021-01-01T00:00:00Z</LastModificationTime><Expires>False</Expires></Times>
        <String><Key>Title</Key><Value>subject</Value></String>
        <QualityCheck>False</QualityCheck>
        <History>
          <Entry>
            <UUID>c3ViamVjdC1lbnRyeS0tLQ==</UUID>
            <Times><CreationTime>2020-01-01T00:00:00Z</CreationTime><LastModificationTime>2020-06-01T00:00:00Z</LastModificationTime><Expires>False</Expires></Times>
            <String><Key>Title</Key><Value>subject</Value></String>
            <QualityCheck>False</QualityCheck>
            <Binary><Key>Scanned Document.pdf</Key><Value Ref="0"/></Binary>
          </Entry>
        </History>
      </Entry>
    </Group>
  </Root>
</KeePassFile>
"#;
    let path = support::imported(&tool, scratch.path(), "history", xml, support::SECRET);

    {
        let mut vault = open(&path, support::SECRET);
        let id = id_of(&vault, "subject");
        assert!(files(&vault).is_empty(), "the entry itself carries no file");
        assert_eq!(
            version_files(&vault, id),
            vec![(SCAN.to_owned(), b"page one".len())],
            "the import did not keep the file the version names"
        );

        assert!(matches!(
            vault.add_attachment(id, SCAN, b"a new scan"),
            Ok(Attached::Added)
        ));
        vault.save().expect("the database saves");
    }

    let vault = open(&path, support::SECRET);
    let id = id_of(&vault, "subject");
    assert_eq!(files(&vault), owned(&[("subject", SCAN, b"a new scan")]));
    assert_eq!(
        version_files(&vault, id),
        vec![(SCAN.to_owned(), b"page one".len())],
        "the version lost the file it held"
    );
}

/// The files the entry's one earlier version names, with their sizes.
fn version_files(vault: &vault_core::Vault, id: EntryId) -> Vec<(String, usize)> {
    vault
        .version(id, 0)
        .expect("the version is there")
        .attachments
        .into_iter()
        .map(|attachment| (attachment.name, attachment.size))
        .collect()
}

/// Every way a file goes on asks the same questions first, and none of them
/// answers an entry that is not there.
#[test]
fn nothing_goes_on_an_entry_that_is_not_there() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = passport(scratch.path());
    let (_other, elsewhere) = support::scratch("minimal-kdbx41.kdbx");

    // An identifier that is real, just not in this database.
    let nobody = open(&elsewhere, support::SECRET).tree().entries[0].id;
    let mut vault = open(&path, BUILT_PASSWORD);
    let before = files(&vault);

    assert!(matches!(
        vault.add_attachment(nobody, SCAN, b"x"),
        Err(VaultError::NoSuchEntry)
    ));
    assert!(matches!(
        vault.keep_both(nobody, SCAN, b"x"),
        Err(VaultError::NoSuchEntry)
    ));
    assert!(matches!(
        vault.replace_attachment(nobody, SCAN, b"x"),
        Err(VaultError::NoSuchEntry)
    ));
    assert_eq!(files(&vault), before);
}
