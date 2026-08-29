//! Making, changing and deleting what is in a database, and the pool of files
//! underneath it.

use keepass::db::fields;
use vault_core::model::{EntryId, Field, FieldValue, GroupId};
use vault_core::{NewValue, Vault, VaultError};
use zeroize::Zeroizing;

use crate::support::{self, BUILT_PASSWORD, RICH, SECRET, built, entry_titled, open};

/// A database with three entries in the root, each carrying one file, saved and
/// closed so that it comes back the way a database from disk comes back.
///
/// That matters more than it looks. The library counts the entries pointing at
/// a file only from the moment it wrote the pointer itself, so a database built
/// in memory has counts a database read from a file does not, and a test that
/// never went through the disk would be testing the wrong code.
fn three_files(directory: &std::path::Path) -> std::path::PathBuf {
    let path = built(directory, "files.kdbx", |database| {
        for round in 0..3u8 {
            database
                .root_mut()
                .add_entry()
                .edit(|entry| {
                    entry.set_unprotected(fields::TITLE, format!("entry {round}"));
                    entry.add_attachment(
                        format!("file {round}"),
                        keepass::db::Value::unprotected(vec![round; 32]),
                    );
                })
                .id();
        }
    });

    // Written and read back once, so the file numbers are the ones the reader
    // hands out rather than the ones the writer happened to use.
    let mut vault = open(&path, BUILT_PASSWORD);
    vault.save().expect("the database saves");
    path
}

/// Every entry's file, by the title of the entry that holds it.
fn files(vault: &Vault) -> Vec<(String, String, Vec<u8>)> {
    let mut found = Vec::new();
    for summary in support::all_entries(vault) {
        let entry = vault.entry(summary.id).expect("the entry is there");
        let title = entry
            .field(fields::TITLE)
            .and_then(|field| field.value.open())
            .unwrap_or_default()
            .to_owned();

        for attachment in &entry.attachments {
            let bytes = vault
                .attachment(entry.id, &attachment.name)
                .expect("the file is there");
            found.push((
                title.clone(),
                attachment.name.clone(),
                bytes.expose().to_vec(),
            ));
        }
    }
    found.sort();
    found
}

fn only_entry(vault: &Vault, title: &str) -> EntryId {
    entry_titled(vault, title).id
}

fn root_of(vault: &Vault) -> GroupId {
    vault.tree().id
}

/// The one folder with this name, wherever it sits.
fn only_group(vault: &Vault, name: &str) -> GroupId {
    fn walk(group: &vault_core::model::Project, name: &str, into: &mut Vec<GroupId>) {
        if group.name == name {
            into.push(group.id);
        }
        for section in &group.sections {
            walk(section, name, into);
        }
    }

    let mut found = Vec::new();
    walk(&vault.tree(), name, &mut found);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one folder called {name:?}"
    );
    found.remove(0)
}

#[test]
fn a_new_entry_carries_the_fields_the_database_asks_to_protect() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "new.kdbx", |_| {});
    let mut vault = open(&path, BUILT_PASSWORD);

    let root = root_of(&vault);
    let id = vault.create_entry(root).expect("the entry is made");
    let entry = vault.entry(id).expect("the entry is there");

    let named: Vec<&str> = entry
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(named, vec!["Notes", "Password", "Title", "URL", "UserName"]);

    // A database that states no memory protection is a database that wants what
    // KeePass protects by default, which is the password and nothing else.
    for field in &entry.fields {
        let protected = matches!(field.value, FieldValue::Protected { .. });
        assert_eq!(
            protected,
            field.name == fields::PASSWORD,
            "{} came back with the wrong protection",
            field.name
        );
        assert!(
            field.is_empty(),
            "{} came back with something in it",
            field.name
        );
    }
}

#[test]
fn an_entry_made_in_coffer_is_there_after_the_file_is_opened_again() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "new.kdbx", |_| {});

    let id = {
        let mut vault = open(&path, BUILT_PASSWORD);
        let root = root_of(&vault);
        let id = vault.create_entry(root).expect("the entry is made");
        vault
            .set_field(id, fields::TITLE, NewValue::Open("a new login".to_owned()))
            .expect("the title is written");
        vault
            .set_field(
                id,
                fields::PASSWORD,
                NewValue::Protected(Zeroizing::new("hunter2".to_owned())),
            )
            .expect("the password is written");
        vault
            .set_tags(id, vec!["prod".to_owned(), "ssh".to_owned()])
            .expect("the tags are written");
        vault.save().expect("the database saves");
        id
    };

    let vault = open(&path, BUILT_PASSWORD);
    let entry = vault.entry(id).expect("the entry survived the save");
    assert_eq!(
        entry.field(fields::TITLE).and_then(|f| f.value.open()),
        Some("a new login")
    );
    assert_eq!(entry.tags, vec!["prod".to_owned(), "ssh".to_owned()]);
    assert_eq!(
        vault
            .reveal(id, fields::PASSWORD)
            .expect("the password is there")
            .expose_str(),
        Some("hunter2")
    );
}

#[test]
fn a_folder_can_be_made_renamed_and_filled() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "folders.kdbx", |_| {});
    let mut vault = open(&path, BUILT_PASSWORD);

    let root = root_of(&vault);
    let project = vault
        .create_group(root, "Work")
        .expect("the folder is made");
    let section = vault
        .create_group(project, "Servers")
        .expect("the folder is made");
    let id = vault.create_entry(section).expect("the entry is made");

    vault
        .rename_group(section, "Machines")
        .expect("the folder is renamed");
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&path, BUILT_PASSWORD);
    let tree = vault.tree();
    let work = tree
        .sections
        .iter()
        .find(|section| section.name == "Work")
        .expect("the project is there");
    let machines = work.sections.first().expect("the section is there");
    assert_eq!(machines.name, "Machines");
    assert_eq!(
        machines.entries.first().map(|entry| entry.id),
        Some(id),
        "the entry is not in the folder it was made in"
    );
}

#[test]
fn the_top_of_the_vault_is_not_a_folder_anybody_can_delete() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "root.kdbx", |_| {});
    let mut vault = open(&path, BUILT_PASSWORD);

    let root = root_of(&vault);
    assert!(matches!(
        vault.delete_group(root),
        Err(VaultError::CannotMoveRoot)
    ));
    // The tree still has its top, which is what a database with a deleted root
    // group would not.
    assert_eq!(vault.tree().id, root);
}

#[test]
fn a_deleted_entry_goes_to_the_recycle_bin_and_the_second_deletion_takes_it_out_of_the_file() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);

    let id = only_entry(&vault, "basic");
    vault.delete_entry(id).expect("the entry is deleted");

    let bin = vault
        .tree()
        .sections
        .iter()
        .find(|section| section.is_recycle_bin)
        .cloned()
        .expect("the fixture has a recycle bin");
    assert!(
        bin.entries.iter().any(|entry| entry.id == id),
        "the entry did not land in the recycle bin"
    );

    vault.delete_entry(id).expect("the entry is deleted again");
    assert!(
        vault.entry(id).is_none(),
        "the entry is still in the database"
    );

    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, SECRET);
    assert!(vault.entry(id).is_none(), "the entry came back");
}

#[test]
fn a_database_that_keeps_no_recycle_bin_deletes_outright() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "nobin.kdbx", |database| {
        database.meta.recyclebin_enabled = Some(false);
        database
            .root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "doomed"));
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    let id = only_entry(&vault, "doomed");
    vault.delete_entry(id).expect("the entry is deleted");

    assert!(vault.entry(id).is_none(), "the entry is still there");
    assert!(
        vault
            .tree()
            .sections
            .iter()
            .all(|section| !section.is_recycle_bin),
        "a database that asked for no recycle bin was given one"
    );
}

#[test]
fn a_database_that_wants_a_recycle_bin_and_has_none_is_given_one() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "makebin.kdbx", |database| {
        database
            .root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "doomed"));
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    let id = only_entry(&vault, "doomed");
    vault.delete_entry(id).expect("the entry is deleted");
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&path, BUILT_PASSWORD);
    let bin = vault
        .tree()
        .sections
        .iter()
        .find(|section| section.is_recycle_bin)
        .cloned()
        .expect("a recycle bin was made and written");
    assert_eq!(bin.name, "Recycle Bin");
    assert!(bin.entries.iter().any(|entry| entry.id == id));
}

#[test]
fn deleting_a_folder_takes_everything_in_it() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "folder.kdbx", |database| {
        database.meta.recyclebin_enabled = Some(false);
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    let root = root_of(&vault);
    let project = vault
        .create_group(root, "Work")
        .expect("the folder is made");
    let section = vault
        .create_group(project, "Servers")
        .expect("the folder is made");
    let inside = vault.create_entry(section).expect("the entry is made");

    vault.delete_group(project).expect("the folder is deleted");
    assert!(
        vault.entry(inside).is_none(),
        "the entry outlived its folder"
    );
    assert!(
        vault.tree().sections.iter().all(|s| s.name != "Work"),
        "the folder is still in the tree"
    );
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&path, BUILT_PASSWORD);
    assert!(vault.entry(inside).is_none(), "the entry came back");
    assert!(vault.tree().sections.iter().all(|s| s.name != "Work"));
}

/// A `RecycleBinUUID` naming a folder that is not there is how a vault ends up
/// with two recycle bins, and the library clears those fields only for the
/// removal that keeps no record of what it deleted - which is not the one
/// Coffer uses.
#[test]
fn a_deleted_folder_is_not_left_named_in_the_settings_of_the_file() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let (_scratch, database) = support::scratch(RICH);

    let named = {
        let vault = open(&database, SECRET);
        let bin = vault
            .tree()
            .sections
            .iter()
            .find(|section| section.is_recycle_bin)
            .cloned()
            .expect("the fixture has a recycle bin");
        bin.id
    };

    let before = support::export(&tool, &database, SECRET, None);
    assert!(
        before.contains("<RecycleBinUUID>"),
        "the fixture is supposed to name a recycle bin"
    );

    {
        let mut vault = open(&database, SECRET);
        vault
            .delete_group(named)
            .expect("the recycle bin itself is deleted");
        vault.save().expect("the database saves");
    }

    // The identifier is still in the file, in `DeletedObjects` where it belongs:
    // what must be gone is the settings field that pointed at the folder.
    let after = support::export(&tool, &database, SECRET, None);
    assert!(
        !after.contains("<RecycleBinUUID>cmVjeWNsZWJpbi0tLS0tLQ=="),
        "the file still names the folder that was deleted"
    );
    assert!(
        after.contains("cmVjeWNsZWJpbi0tLS0tLQ=="),
        "the deletion was not recorded at all"
    );
}

#[test]
fn a_value_a_keepass_file_cannot_hold_is_refused_wherever_it_is_offered() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "hostile.kdbx", |_| {});
    let mut vault = open(&path, BUILT_PASSWORD);

    let root = root_of(&vault);
    let id = vault.create_entry(root).expect("the entry is made");

    // A vertical tab is not one of the three control characters XML 1.0 allows,
    // so a file carrying one is a file no conformant reader opens again.
    let hostile = "no\u{b}";
    assert!(matches!(
        vault.create_group(root, hostile),
        Err(VaultError::UnwritableText)
    ));
    assert!(matches!(
        vault.rename_group(root, hostile),
        Err(VaultError::UnwritableText)
    ));
    assert!(matches!(
        vault.set_tags(id, vec![hostile.to_owned()]),
        Err(VaultError::UnwritableText)
    ));
    assert!(matches!(
        vault.add_attachment(id, hostile, Zeroizing::new(b"x".to_vec())),
        Err(VaultError::UnwritableText)
    ));
}

#[test]
fn a_tag_that_would_come_back_as_two_is_refused() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "tags.kdbx", |_| {});
    let mut vault = open(&path, BUILT_PASSWORD);

    let root = root_of(&vault);
    let id = vault.create_entry(root).expect("the entry is made");

    // The format keeps tags as one string and reads three characters as the
    // separator between them. A tag holding one comes back as two tags, and a
    // padded one comes back trimmed: both are the file changing a value.
    for rejected in ["a;b", "a,b", "a\tb", " a", "a ", ""] {
        assert!(
            matches!(
                vault.set_tags(id, vec![rejected.to_owned()]),
                Err(VaultError::UnwritableText)
            ),
            "{rejected:?} was accepted"
        );
    }

    vault
        .set_tags(id, vec!["one".to_owned(), "two words".to_owned()])
        .expect("ordinary tags are written");
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&path, BUILT_PASSWORD);
    assert_eq!(
        vault.entry(id).expect("the entry is there").tags,
        vec!["one".to_owned(), "two words".to_owned()]
    );
}

#[test]
fn removing_a_file_leaves_every_other_file_on_the_entry_that_had_it() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = three_files(scratch.path());

    let before = {
        let vault = open(&path, BUILT_PASSWORD);
        files(&vault)
    };
    assert_eq!(before.len(), 3);

    {
        let mut vault = open(&path, BUILT_PASSWORD);
        let middle = only_entry(&vault, "entry 1");
        vault
            .remove_attachment(middle, "file 1")
            .expect("the file is removed");
        vault.save().expect("the database saves");
    }

    // The file that was removed is gone, and the two that were not are still
    // whole and still on the entries that had them. Nothing here is subtle: a
    // hole in the pool hands the third entry the second entry's bytes.
    let vault = open(&path, BUILT_PASSWORD);
    let after = files(&vault);
    assert_eq!(
        after,
        before
            .into_iter()
            .filter(|(_, name, _)| name != "file 1")
            .collect::<Vec<_>>()
    );
}

#[test]
fn removing_one_of_two_names_for_the_same_file_leaves_the_other_holding_it() {
    let (_scratch, database) = support::scratch(RICH);

    // The fixture gives one file two names on the same entry, which is what
    // KeePass does when the same bytes are attached twice.
    let (id, bytes) = {
        let vault = open(&database, SECRET);
        let entry = entry_titled(&vault, "attachments");
        let bytes = vault
            .attachment(entry.id, "zero-byte.txt")
            .expect("the file is there");
        (entry.id, bytes.expose().to_vec())
    };

    {
        let mut vault = open(&database, SECRET);
        vault
            .remove_attachment(id, "zero-byte.txt")
            .expect("the name is removed");
        vault.save().expect("the database saves");
    }

    let vault = open(&database, SECRET);
    let entry = vault.entry(id).expect("the entry is there");
    let names: Vec<&str> = entry
        .attachments
        .iter()
        .map(|attachment| attachment.name.as_str())
        .collect();
    assert!(!names.contains(&"zero-byte.txt"), "the name is still there");
    assert!(
        names.contains(&"../../escape.txt"),
        "the other name went too"
    );
    assert_eq!(
        vault
            .attachment(id, "../../escape.txt")
            .expect("the file is there")
            .expose(),
        bytes.as_slice(),
        "the bytes behind the surviving name changed"
    );
}

#[test]
fn a_file_an_earlier_version_still_holds_is_not_taken_away_from_it() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "pinned.kdbx", |_| {});

    let id = {
        let mut vault = open(&path, BUILT_PASSWORD);
        let root = root_of(&vault);
        let id = vault.create_entry(root).expect("the entry is made");
        vault
            .add_attachment(id, "key.pem", Zeroizing::new(b"a private key".to_vec()))
            .expect("the file is added");
        // A change to the files writes no version. A change to a field does,
        // and that version holds the file the entry had at the time.
        assert!(vault.versions(id).is_empty());
        vault
            .set_field(id, fields::NOTES, NewValue::Open("edited".to_owned()))
            .expect("the note is written");
        assert_eq!(vault.versions(id).len(), 1);
        vault.save().expect("the database saves");
        id
    };

    let mut vault = open(&path, BUILT_PASSWORD);
    assert!(
        matches!(
            vault.remove_attachment(id, "key.pem"),
            Err(VaultError::AttachmentInHistory { versions: 1 })
        ),
        "a file an earlier version holds was removed anyway"
    );
    assert!(
        vault.attachment(id, "key.pem").is_ok(),
        "the refusal took the file anyway"
    );

    // Clearing the versions is what lets it go, and the refusal says so.
    vault.clear_history(id).expect("the versions are dropped");
    vault
        .remove_attachment(id, "key.pem")
        .expect("the file goes once nothing holds it");
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&path, BUILT_PASSWORD);
    assert!(
        vault
            .entry(id)
            .expect("the entry is there")
            .attachments
            .is_empty()
    );
}

#[test]
fn deleting_an_entry_takes_its_files_and_leaves_everybody_elses() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = three_files(scratch.path());

    let before = {
        let vault = open(&path, BUILT_PASSWORD);
        files(&vault)
    };

    {
        let mut vault = open(&path, BUILT_PASSWORD);
        let middle = only_entry(&vault, "entry 1");
        // Straight out of the file: the recycle bin would only move it, and a
        // moved entry still holds its files.
        vault
            .delete_entry(middle)
            .expect("the entry goes to the bin");
        vault.delete_entry(middle).expect("the entry is deleted");
        vault.save().expect("the database saves");
    }

    let vault = open(&path, BUILT_PASSWORD);
    assert_eq!(
        files(&vault),
        before
            .into_iter()
            .filter(|(title, _, _)| title != "entry 1")
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_file_added_and_taken_away_again_leaves_the_pool_as_it_was() {
    let (_scratch, database) = support::scratch(RICH);

    let before = {
        let vault = open(&database, SECRET);
        files(&vault)
    };

    {
        let mut vault = open(&database, SECRET);
        let id = only_entry(&vault, "basic");
        vault
            .add_attachment(id, "note.txt", Zeroizing::new(b"temporary".to_vec()))
            .expect("the file is added");
        vault.save().expect("the database saves");
    }

    {
        let mut vault = open(&database, SECRET);
        let id = only_entry(&vault, "basic");
        assert_eq!(
            vault
                .attachment(id, "note.txt")
                .expect("the file is there")
                .expose(),
            b"temporary"
        );
        vault
            .remove_attachment(id, "note.txt")
            .expect("the file is removed");
        vault.save().expect("the database saves");
    }

    let vault = open(&database, SECRET);
    assert_eq!(
        files(&vault),
        before,
        "the pool did not come back as it was"
    );
}

#[test]
fn a_file_larger_than_a_vault_holds_is_refused_before_it_is_read_in() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "large.kdbx", |_| {});
    let mut vault = open(&path, BUILT_PASSWORD);

    let root = root_of(&vault);
    let id = vault.create_entry(root).expect("the entry is made");

    // The inner header keeps a file's length in four bytes, so a file past four
    // gigabytes is written truncated and the database cannot be opened again.
    // Coffer stops long before that, because the whole database is read into
    // memory to be opened at all.
    let huge = Zeroizing::new(vec![0u8; (256 * 1024 * 1024) + 1]);
    assert!(matches!(
        vault.add_attachment(id, "huge.bin", huge),
        Err(VaultError::AttachmentTooLarge)
    ));
}

#[test]
fn a_name_the_file_can_hold_is_stored_exactly_as_it_was_given() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "names.kdbx", |_| {});

    // An attachment name is free text and nothing turns one into a path here.
    let awkward = ["../../escape.txt", "nested/path/name.txt", "ユニコード 🔐"];

    let id = {
        let mut vault = open(&path, BUILT_PASSWORD);
        let root = root_of(&vault);
        let id = vault.create_entry(root).expect("the entry is made");
        for (round, name) in awkward.iter().enumerate() {
            vault
                .add_attachment(id, name, Zeroizing::new(vec![round as u8; 8]))
                .expect("the file is added");
        }
        vault.save().expect("the database saves");
        id
    };

    let vault = open(&path, BUILT_PASSWORD);
    let entry = vault.entry(id).expect("the entry is there");
    let mut names: Vec<&str> = entry
        .attachments
        .iter()
        .map(|attachment| attachment.name.as_str())
        .collect();
    names.sort_unstable();
    let mut wanted = awkward.to_vec();
    wanted.sort_unstable();
    assert_eq!(names, wanted);

    for (round, name) in awkward.iter().enumerate() {
        assert_eq!(
            vault
                .attachment(id, name)
                .expect("the file is there")
                .expose(),
            vec![round as u8; 8].as_slice(),
            "{name} came back with somebody else's bytes"
        );
    }
}

#[test]
fn every_file_can_be_taken_away_one_at_a_time_without_the_others_moving() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = three_files(scratch.path());

    // Removing the lowest-numbered file first is the order that forces the pool
    // to close up on every step, which is the order most likely to hand
    // somebody else's bytes to an entry.
    for round in 0..3u8 {
        let mut wanted = {
            let vault = open(&path, BUILT_PASSWORD);
            files(&vault)
        };
        wanted.retain(|(_, name, _)| name != &format!("file {round}"));

        let mut vault = open(&path, BUILT_PASSWORD);
        let id = only_entry(&vault, &format!("entry {round}"));
        vault
            .remove_attachment(id, &format!("file {round}"))
            .expect("the file is removed");
        vault.save().expect("the database saves");
        drop(vault);

        let vault = open(&path, BUILT_PASSWORD);
        assert_eq!(
            files(&vault),
            wanted,
            "round {round} disturbed another file"
        );
    }
}

#[test]
fn keepassxc_sees_the_files_coffer_left_on_the_entries_that_had_them() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let (_scratch, database) = support::scratch(RICH);

    {
        let mut vault = open(&database, SECRET);
        let id = only_entry(&vault, "attachments");
        vault
            .remove_attachment(id, "all-256-bytes.bin")
            .expect("the file is removed");
        vault.save().expect("the database saves");
    }

    let paths = support::entry_paths(&tool, &database, SECRET, None);
    let holder = paths
        .iter()
        .find(|path| path.ends_with("/attachments"))
        .expect("keepassxc lists the entry that had the file");
    let names = support::attachment_names(&tool, &database, SECRET, None, holder);
    assert!(
        !names.contains(&"all-256-bytes.bin".to_owned()),
        "keepassxc still lists the file Coffer removed"
    );

    // The entry that shared nothing with it still has its own, byte for byte.
    let key = paths
        .iter()
        .find(|path| path.ends_with("/ssh key"))
        .expect("keepassxc lists the entry that kept its file");
    let landing = tempfile::tempdir().expect("a scratch directory");
    let bytes = support::attachment_bytes(
        &tool,
        &database,
        SECRET,
        None,
        key,
        "id_ed25519",
        &landing.path().join("key"),
    );
    assert!(
        bytes.starts_with(b"-----BEGIN OPENSSH PRIVATE KEY-----"),
        "the surviving file came back as somebody else's bytes"
    );
}

#[test]
fn keepassxc_shows_a_version_coffer_wrote() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let (_scratch, database) = support::scratch(RICH);

    {
        let mut vault = open(&database, SECRET);
        let id = only_entry(&vault, "basic");
        vault
            .set_field(
                id,
                fields::PASSWORD,
                NewValue::Protected(Zeroizing::new("a newer password".to_owned())),
            )
            .expect("the password is written");
        vault.save().expect("the database saves");
    }

    let exported = support::export(&tool, &database, SECRET, None);
    // The value that was replaced is in the history block rather than gone, and
    // the value that replaced it is the entry's own.
    assert!(
        exported.contains("correct horse battery staple"),
        "the state before the edit is not in the file keepassxc reads"
    );
    assert!(exported.contains("a newer password"));

    // The version keepassxc offers to restore is a version of this entry: the
    // block sits inside the entry and carries the entry's own identifier. A
    // version whose identifier had drifted is a file nothing can open again.
    let at = exported
        .find("correct horse battery staple")
        .expect("the value is in the file");
    let before = exported.get(..at).expect("the file has a beginning");
    assert!(
        before.rfind("<History>") > before.rfind("</History>"),
        "the state before the edit is not inside a history block"
    );
    assert!(
        exported.matches("ZS1iYXNpYy0tLS0tLS0tLQ==").count() >= 2,
        "the version does not carry the identifier of the entry it belongs to"
    );
}

/// The whole of the first rule, applied to the editing path: a change to one
/// value changes that value and nothing else. Every other entry comes back with
/// the same fields, the same protection, the same tags and the same files.
#[test]
fn an_edit_changes_the_field_it_was_asked_to_change_and_nothing_else() {
    let (_scratch, database) = support::scratch(RICH);

    let before = {
        let vault = open(&database, SECRET);
        picture(&vault)
    };

    let id = {
        let mut vault = open(&database, SECRET);
        let id = only_entry(&vault, "basic");
        vault
            .set_field(id, fields::NOTES, NewValue::Open("a new note".to_owned()))
            .expect("the note is written");
        vault.save().expect("the database saves");
        id
    };

    let vault = open(&database, SECRET);
    let after = picture(&vault);

    assert_eq!(
        before.len(),
        after.len(),
        "the database gained or lost an entry"
    );
    for (was, now) in before.iter().zip(after.iter()) {
        if was.0 == id {
            continue;
        }
        assert_eq!(was, now, "an entry nobody edited came back different");
    }

    let changed = after
        .iter()
        .find(|(entry, _, _, _)| *entry == id)
        .expect("the entry is still there");
    assert!(
        changed
            .1
            .contains(&("Notes".to_owned(), Some("a new note".to_owned()), false)),
        "the note was not written"
    );
}

/// Every entry, with everything about it that a save must not change on its
/// own: its fields, whether the database protects each one, its tags, and the
/// bytes of every file it holds.
type Picture = Vec<(
    EntryId,
    Vec<(String, Option<String>, bool)>,
    Vec<String>,
    Vec<(String, Vec<u8>)>,
)>;

fn picture(vault: &Vault) -> Picture {
    let mut found: Picture = support::all_entries(vault)
        .into_iter()
        .map(|summary| {
            let entry = vault.entry(summary.id).expect("the entry is there");
            let mut fields: Vec<(String, Option<String>, bool)> = entry
                .fields
                .iter()
                .map(|field| {
                    (
                        field.name.clone(),
                        field.value.open().map(str::to_owned),
                        matches!(field.value, FieldValue::Protected { .. }),
                    )
                })
                .collect();
            fields.sort();

            let files: Vec<(String, Vec<u8>)> = entry
                .attachments
                .iter()
                .map(|attachment| {
                    let bytes = vault
                        .attachment(entry.id, &attachment.name)
                        .expect("the file is there");
                    (attachment.name.clone(), bytes.expose().to_vec())
                })
                .collect();

            (entry.id, fields, entry.tags.clone(), files)
        })
        .collect();
    found.sort_by_key(|(id, _, _, _)| id.to_string());
    found
}

/// The other order the two clients can go in: the file is written first and the
/// change in the window comes second. The window still holds what it opened, so
/// the save still stops and asks.
#[test]
fn a_file_written_before_the_edit_stops_the_save_just_the_same() {
    let (_scratch, database) = support::scratch(RICH);
    let mut ours = open(&database, SECRET);
    let id = only_entry(&ours, "basic");

    {
        let mut theirs = Vault::open(
            &database,
            support::password(SECRET),
            vault_core::LockPolicy::TakeOver,
        )
        .expect("the database opens");
        theirs
            .set_field(id, fields::NOTES, NewValue::Open("theirs".to_owned()))
            .expect("the note is written");
        theirs.save().expect("the database saves");
    }

    ours.set_field(id, fields::NOTES, NewValue::Open("ours".to_owned()))
        .expect("the note is written");
    assert!(matches!(ours.save(), Err(VaultError::ExternalChange)));

    // Keeping both is the way out that loses nothing: this version goes beside
    // the database, and the database keeps theirs.
    let beside = database.with_extension("mine.kdbx");
    ours.save_copy(&beside).expect("the copy is written");
    ours.reload().expect("the file reads again");
    drop(ours);

    let mine = open(&beside, SECRET);
    assert_eq!(
        mine.entry(id)
            .expect("the entry is there")
            .field(fields::NOTES)
            .and_then(|field| field.value.open()),
        Some("ours")
    );
    drop(mine);

    let theirs = open(&database, SECRET);
    assert_eq!(
        theirs
            .entry(id)
            .expect("the entry is there")
            .field(fields::NOTES)
            .and_then(|field| field.value.open()),
        Some("theirs")
    );
}

#[test]
fn emptying_the_recycle_bin_takes_what_is_in_it_out_of_the_file() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);

    let deleted = only_entry(&vault, "deleted entry");
    let living = only_entry(&vault, "basic");
    vault.delete_entry(living).expect("the entry is deleted");

    // A folder in the bin, with a folder in that. Emptying has to take the
    // whole branch in one go rather than coming back for a folder that went
    // with its parent.
    let project = only_group(&vault, "Personal");
    let inner = vault
        .create_group(project, "nested")
        .expect("the folder is made");
    vault
        .create_group(inner, "deeper")
        .expect("the folder is made");
    vault.delete_group(project).expect("the folder is deleted");

    assert_eq!(vault.count(), 11, "the fixture holds eleven entries");

    vault.empty_recycle_bin().expect("the bin is emptied");
    assert!(vault.entry(deleted).is_none(), "the bin still holds it");
    assert!(vault.entry(living).is_none(), "the bin still holds it");
    // The entry that was in the bin, the entry deleted into it, and the three
    // the deleted folder brought with it.
    assert_eq!(vault.count(), 6);
    assert!(
        vault.tree().sections.iter().all(|s| s.name != "Personal"),
        "the folder that went into the bin is still in the tree"
    );

    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, SECRET);
    assert_eq!(vault.count(), 6, "the entries came back");
    assert!(
        vault
            .tree()
            .sections
            .iter()
            .any(|section| section.is_recycle_bin),
        "emptying the bin took the bin as well"
    );
}

#[test]
fn a_field_can_be_taken_off_an_entry() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let id = only_entry(&vault, "many custom fields");

    assert!(matches!(
        vault.remove_field(id, "no such field"),
        Err(VaultError::NoSuchField)
    ));

    vault
        .remove_field(id, "custom-001")
        .expect("the field is removed");
    assert!(
        vault
            .entry(id)
            .expect("the entry is there")
            .field("custom-001")
            .is_none()
    );

    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, SECRET);
    let entry = vault.entry(id).expect("the entry is there");
    assert!(entry.field("custom-001").is_none(), "the field came back");
    assert!(
        entry.field("custom-002").is_some(),
        "removing one field took another"
    );
}

/// A version is read the way an entry is read, and what it protects it goes on
/// protecting: the value comes one field at a time or not at all.
#[test]
fn a_version_is_read_without_its_protected_values_crossing() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let id = only_entry(&vault, "versioned");

    let oldest = vault.versions(id).first().expect("there is one").index;
    let version = vault.version(id, oldest).expect("the version reads");

    assert_eq!(
        version.field(fields::PASSWORD).map(Field::is_empty),
        Some(false)
    );
    assert_eq!(
        version
            .field(fields::PASSWORD)
            .and_then(|field| field.value.open()),
        None,
        "a version handed its protected value over without being asked"
    );
    assert_eq!(
        vault
            .reveal_version(id, oldest, fields::PASSWORD)
            .expect("the value comes back on request")
            .expose_str(),
        Some("version 1 password")
    );

    assert!(vault.version(id, 99).is_none());
    assert!(vault.reveal_version(id, 99, fields::PASSWORD).is_none());
}

#[test]
fn restoring_a_version_keeps_the_state_it_replaced() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let id = only_entry(&vault, "versioned");

    let versions = vault.versions(id);
    assert_eq!(versions.len(), 6, "the fixture carries six versions");
    let oldest = versions.first().expect("there is an oldest").index;

    let wanted = vault
        .reveal_version(id, oldest, fields::PASSWORD)
        .expect("the version has a password")
        .expose_str()
        .expect("it is text")
        .to_owned();
    let replaced = vault
        .reveal(id, fields::PASSWORD)
        .expect("the entry has a password")
        .expose_str()
        .expect("it is text")
        .to_owned();

    vault
        .restore_version(id, oldest)
        .expect("the version is restored");

    assert_eq!(
        vault
            .reveal(id, fields::PASSWORD)
            .expect("the entry has a password")
            .expose_str(),
        Some(wanted.as_str())
    );
    assert_eq!(
        vault.versions(id).len(),
        7,
        "the state the restore replaced was not kept"
    );
    assert!(
        (0..7).any(|index| vault
            .reveal_version(id, index, fields::PASSWORD)
            .and_then(|value| value.expose_str().map(str::to_owned))
            == Some(replaced.clone())),
        "the state the restore replaced is not in the history"
    );
}

#[test]
fn a_version_can_be_dropped_and_the_rest_stay_in_order() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let id = only_entry(&vault, "versioned");

    let before: Vec<_> = vault
        .versions(id)
        .into_iter()
        .map(|version| version.modified)
        .collect();
    let dropping = vault.versions(id)[2].index;
    let dropped = vault.versions(id)[2].modified;

    vault
        .delete_version(id, dropping)
        .expect("the version goes");

    let after: Vec<_> = vault
        .versions(id)
        .into_iter()
        .map(|version| version.modified)
        .collect();
    assert_eq!(
        after,
        before
            .into_iter()
            .filter(|modified| *modified != dropped)
            .collect::<Vec<_>>()
    );

    assert!(matches!(
        vault.delete_version(id, 99),
        Err(VaultError::NoSuchVersion)
    ));
}

#[test]
fn clearing_the_versions_leaves_the_entry_as_it_is() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let id = only_entry(&vault, "versioned");

    let now = vault
        .reveal(id, fields::PASSWORD)
        .expect("the entry has a password")
        .expose_str()
        .expect("it is text")
        .to_owned();

    vault.clear_history(id).expect("the versions are dropped");
    assert!(vault.versions(id).is_empty());
    assert_eq!(
        vault
            .reveal(id, fields::PASSWORD)
            .expect("the entry has a password")
            .expose_str(),
        Some(now.as_str())
    );

    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, SECRET);
    assert!(vault.versions(id).is_empty(), "the versions came back");
}

#[test]
fn writing_over_a_file_somebody_else_changed_keeps_theirs_in_the_first_snapshot() {
    let (_scratch, database) = support::scratch(RICH);

    let mut ours = open(&database, SECRET);
    let id = only_entry(&ours, "basic");
    ours.set_field(id, fields::NOTES, NewValue::Open("ours".to_owned()))
        .expect("the note is written");

    // Somebody else writes the file while we hold it open.
    {
        let mut theirs = Vault::open(
            &database,
            support::password(SECRET),
            vault_core::LockPolicy::TakeOver,
        )
        .expect("the database opens");
        theirs
            .set_field(id, fields::NOTES, NewValue::Open("theirs".to_owned()))
            .expect("the note is written");
        theirs.save().expect("the database saves");
    }

    assert!(matches!(ours.save(), Err(VaultError::ExternalChange)));
    ours.save_over()
        .expect("keeping ours is allowed once asked");
    drop(ours);

    let vault = open(&database, SECRET);
    assert_eq!(
        vault
            .entry(id)
            .expect("the entry is there")
            .field(fields::NOTES)
            .and_then(|field| field.value.open()),
        Some("ours")
    );

    // Nothing was lost: what was on disk went into the snapshot chain before the
    // write, so the other version is still openable.
    drop(vault);
    let snapshot = vault_core::storage::snapshot::slot(&database, 1).expect("a slot has a name");
    let theirs = open(&snapshot, SECRET);
    assert_eq!(
        theirs
            .entry(id)
            .expect("the entry is there")
            .field(fields::NOTES)
            .and_then(|field| field.value.open()),
        Some("theirs")
    );
}

#[test]
fn a_copy_is_a_database_of_its_own_and_the_original_is_untouched() {
    let (scratch, database) = support::scratch(RICH);
    let beside = scratch.path().join("copy.kdbx");

    let mut vault = open(&database, SECRET);
    let id = only_entry(&vault, "basic");
    vault
        .set_field(
            id,
            fields::NOTES,
            NewValue::Open("only in the copy".to_owned()),
        )
        .expect("the note is written");
    vault.save_copy(&beside).expect("the copy is written");
    drop(vault);

    let copy = open(&beside, SECRET);
    assert_eq!(
        copy.entry(id)
            .expect("the entry is there")
            .field(fields::NOTES)
            .and_then(|field| field.value.open()),
        Some("only in the copy")
    );

    let original = open(&database, SECRET);
    assert_ne!(
        original
            .entry(id)
            .expect("the entry is there")
            .field(fields::NOTES)
            .and_then(|field| field.value.open()),
        Some("only in the copy"),
        "writing a copy wrote the database as well"
    );
    drop(copy);
    drop(original);

    let mut vault = open(&database, SECRET);
    assert!(matches!(
        vault.save_copy(&database),
        Err(VaultError::CopyOntoItself)
    ));
}

#[test]
fn reading_the_file_again_throws_away_what_was_not_saved() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let id = only_entry(&vault, "basic");

    assert!(!vault.is_dirty());
    vault
        .set_field(id, fields::NOTES, NewValue::Open("unsaved".to_owned()))
        .expect("the note is written");
    assert!(vault.is_dirty());

    vault.reload().expect("the file reads again");
    assert!(!vault.is_dirty());
    assert_ne!(
        vault
            .entry(id)
            .expect("the entry is there")
            .field(fields::NOTES)
            .and_then(|field| field.value.open()),
        Some("unsaved")
    );
}

#[test]
fn a_snapshot_is_opened_to_read_and_never_written_back() {
    let (_scratch, database) = support::scratch(RICH);

    {
        let mut vault = open(&database, SECRET);
        vault.save().expect("the database saves");
    }

    let snapshot = vault_core::storage::snapshot::slot(&database, 1).expect("a slot has a name");
    let mut vault = open(&snapshot, SECRET);
    let id = only_entry(&vault, "basic");

    assert!(vault.is_read_only());
    for refused in [
        vault.set_field(id, fields::NOTES, NewValue::Open("x".to_owned())),
        vault.delete_entry(id),
        vault.clear_history(id),
        vault.save(),
        vault.save_over(),
    ] {
        assert!(
            matches!(refused, Err(VaultError::ReadOnlySnapshot)),
            "a snapshot took a change"
        );
    }

    // A copy is the way out, and it is a database like any other.
    let beside = database.with_extension("recovered.kdbx");
    vault.save_copy(&beside).expect("the copy is written");
    assert!(!open(&beside, SECRET).is_read_only());
}

#[test]
fn a_database_coffer_wrote_says_coffer_wrote_it() {
    let (_scratch, database) = support::scratch(RICH);
    {
        let mut vault = open(&database, SECRET);
        vault.save().expect("the database saves");
    }

    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let exported = support::export(&tool, &database, SECRET, None);
    assert!(
        exported.contains("<Generator>Coffer</Generator>"),
        "the file still claims to be somebody else's work"
    );
}

/// An attachment name is somebody's text and a save panel wants a file name.
/// Everything that could make one mean a directory goes, and the panel is never
/// opened on an empty name.
#[test]
fn a_name_out_of_a_database_is_made_into_one_a_save_panel_can_use() {
    for (held, offered) in [
        ("id_ed25519", "id_ed25519"),
        ("../../escape.txt", "escape.txt"),
        ("nested/path/name.txt", "name.txt"),
        ("..\\..\\windows.txt", "windows.txt"),
        ("Macintosh HD:System:x", "x"),
        ("..", "attachment"),
        (".", "attachment"),
        ("", "attachment"),
        ("   ", "attachment"),
        (".hidden", "hidden"),
        ("with\u{0}null", "withnull"),
        ("ユニコード 🔐.pem", "ユニコード 🔐.pem"),
    ] {
        let attachment = vault_core::model::Attachment {
            name: held.to_owned(),
            size: 0,
        };
        assert_eq!(attachment.file_name(), offered, "{held:?}");
    }

    // However long the name in the database is, the one offered fits a
    // filesystem.
    let long = vault_core::model::Attachment {
        name: "a".repeat(10_000),
        size: 0,
    };
    assert!(long.file_name().chars().count() <= 200);
}

/// A file with nothing in it is still a file. The pool has to keep the name and
/// the slot, and the round trip has to bring both back.
#[test]
fn a_file_with_no_bytes_in_it_survives_being_added_and_taken_away() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "empty.kdbx", |_| {});

    let id = {
        let mut vault = open(&path, BUILT_PASSWORD);
        let root = root_of(&vault);
        let id = vault.create_entry(root).expect("the entry is made");
        vault
            .add_attachment(id, "nothing.txt", Zeroizing::new(Vec::new()))
            .expect("the file is added");
        vault
            .add_attachment(
                id,
                "something.txt",
                Zeroizing::new(b"a byte or two".to_vec()),
            )
            .expect("the file is added");
        vault.save().expect("the database saves");
        id
    };

    {
        let vault = open(&path, BUILT_PASSWORD);
        assert_eq!(
            vault
                .attachment(id, "nothing.txt")
                .expect("the file is there")
                .expose(),
            b""
        );
        assert_eq!(
            vault
                .attachment(id, "something.txt")
                .expect("the file is there")
                .expose(),
            b"a byte or two"
        );
    }

    // Taking the empty one away is taking away the file in the lower slot, so
    // the one above it has to move down without changing hands.
    {
        let mut vault = open(&path, BUILT_PASSWORD);
        vault
            .remove_attachment(id, "nothing.txt")
            .expect("the file is removed");
        vault.save().expect("the database saves");
    }

    let vault = open(&path, BUILT_PASSWORD);
    let entry = vault.entry(id).expect("the entry is there");
    assert_eq!(entry.attachments.len(), 1);
    assert_eq!(
        vault
            .attachment(id, "something.txt")
            .expect("the file is there")
            .expose(),
        b"a byte or two"
    );
}

/// Replacing a file by the same name is the shape that loses everything when it
/// is left to the library: it takes the new number before it notices the name
/// is taken, then destroys the old file and leaves the hole behind.
#[test]
fn a_file_replaced_by_one_of_the_same_name_keeps_the_new_bytes_and_the_pool() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = three_files(scratch.path());

    {
        let mut vault = open(&path, BUILT_PASSWORD);
        let middle = only_entry(&vault, "entry 1");
        vault
            .add_attachment(middle, "file 1", Zeroizing::new(b"replaced".to_vec()))
            .expect("the file is replaced");
        vault.save().expect("the database saves");
    }

    let vault = open(&path, BUILT_PASSWORD);
    assert_eq!(
        files(&vault),
        vec![
            ("entry 0".to_owned(), "file 0".to_owned(), vec![0u8; 32]),
            (
                "entry 1".to_owned(),
                "file 1".to_owned(),
                b"replaced".to_vec()
            ),
            ("entry 2".to_owned(), "file 2".to_owned(), vec![2u8; 32]),
        ]
    );
}

/// The fixture gives one file two names, which is what a client that pools
/// identical binaries produces. Taking both away is what makes the file go, and
/// it is the lowest one in the pool, so every other file in the database has to
/// move down a slot behind it - four of them, on the entries of two different
/// entries, all in one change.
#[test]
fn taking_the_last_name_off_a_shared_file_moves_everything_above_it_down() {
    let (_scratch, database) = support::scratch(RICH);

    let before = {
        let vault = open(&database, SECRET);
        files(&vault)
    };

    {
        let mut vault = open(&database, SECRET);
        let id = only_entry(&vault, "attachments");
        vault
            .remove_attachment(id, "zero-byte.txt")
            .expect("the first name goes");
        vault
            .remove_attachment(id, "../../escape.txt")
            .expect("the last name goes, and the file with it");
        vault.save().expect("the database saves");
    }

    let vault = open(&database, SECRET);
    assert_eq!(
        files(&vault),
        before
            .into_iter()
            .filter(|(_, name, _)| name != "zero-byte.txt" && name != "../../escape.txt")
            .collect::<Vec<_>>(),
        "closing the pool up handed a file to the wrong entry"
    );
}

/// The names on a KDBX 3 entry are the file's; the bytes behind them are not.
/// The reader collapses every attachment in one of those onto a single file, so
/// an entry whose row says one thing would hand over another entry's key.
#[test]
fn the_files_in_a_keepass_3_database_are_not_handed_out_at_all() {
    let (_scratch, database) = support::scratch("rich-kdbx31.kdbx");
    let vault = open(&database, SECRET);

    let entry = support::all_entries(&vault)
        .into_iter()
        .find(|summary| summary.attachments > 0)
        .expect("the fixture has an entry with a file on it");
    let entry = vault.entry(entry.id).expect("the entry is there");
    let name = &entry
        .attachments
        .first()
        .expect("the entry has a file")
        .name;

    assert!(matches!(
        vault.attachment(entry.id, name),
        Err(VaultError::UnreadableAttachments)
    ));

    // A KDBX 4 database hands the same file over without a word.
    let (_other, fourth) = support::scratch(RICH);
    let vault = open(&fourth, SECRET);
    let id = only_entry(&vault, "ssh key");
    assert!(
        vault
            .attachment(id, "id_ed25519")
            .expect("the file comes back")
            .expose()
            .starts_with(b"-----BEGIN OPENSSH PRIVATE KEY-----")
    );
}

/// A database that is not there any more, and a reader who says to write it
/// back anyway. The file goes back where it was rather than the save failing on
/// a stamp of something that is gone.
#[test]
fn a_database_somebody_deleted_is_written_back_when_the_reader_asks() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let id = only_entry(&vault, "basic");

    vault
        .set_field(id, fields::NOTES, NewValue::Open("ours".to_owned()))
        .expect("the note is written");
    std::fs::remove_file(&database).expect("the database is deleted");

    assert!(matches!(vault.save(), Err(VaultError::DatabaseGone)));
    vault
        .save_over()
        .expect("writing it back is allowed once asked");
    drop(vault);

    let vault = open(&database, SECRET);
    assert_eq!(
        vault
            .entry(id)
            .expect("the entry is there")
            .field(fields::NOTES)
            .and_then(|field| field.value.open()),
        Some("ours")
    );
}

/// A version is addressed by its position, and a save brings every entry's
/// history inside the database's limits. So a save moves the positions, and a
/// list of versions read before one names versions that are no longer there.
/// Whoever holds that list has to read it again afterwards.
#[test]
fn a_save_moves_the_positions_a_version_list_was_read_at() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "versions.kdbx", |database| {
        let id = database
            .root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "subject"))
            .id();

        // Twelve versions against the default limit of ten, each a minute apart
        // so that the order is not a matter of which one ties with which.
        for round in 0..12u32 {
            let mut entry = database.entry_mut(id).expect("the entry is there");
            entry.edit_tracking(|entry| {
                entry.set_unprotected(fields::USERNAME, format!("user {round}"));
            });
            entry.times.last_modification = chrono::NaiveDate::from_ymd_opt(2024, 1, 1)
                .and_then(|day| day.and_hms_opt(0, 0, 0))
                .map(|moment| moment + chrono::Duration::minutes(i64::from(round)));
        }
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    let id = only_entry(&vault, "subject");
    assert_eq!(vault.versions(id).len(), 12);

    let read_before: Vec<usize> = vault
        .versions(id)
        .into_iter()
        .map(|version| version.index)
        .collect();

    vault.save().expect("the database saves");

    let read_after = vault.versions(id);
    assert_eq!(read_after.len(), 10, "the save did not prune to the limit");
    assert!(
        read_after.len() < read_before.len(),
        "the positions the list was read at are not the positions there are now"
    );
    // The two the limit dropped are the two oldest, and the list is the ten that
    // are left, addressable again from where they are now.
    for version in read_after {
        assert!(vault.version(id, version.index).is_some());
    }
}

/// A KDBX file can hold one file that several entries name: KeePass 2 stores
/// identical binaries once and points every entry at the same one. Nothing in
/// Coffer can produce that shape, so it comes from the tool the fixtures come
/// from.
///
/// Taking one entry's name off it has to leave the other three holding the same
/// bytes, and it has to go on doing that after the pool has been rebuilt once
/// already - which is the case where the library's own record of who holds what
/// is a session old and names one entry out of four.
#[test]
fn a_file_four_entries_share_stays_whole_as_the_names_come_off_one_by_one() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let scratch = tempfile::tempdir().expect("a scratch directory");

    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<KeePassFile>
  <Meta>
    <Generator>coffer-test</Generator>
    <Binaries>
      <Binary ID="0" Compressed="False">c2hhcmVkIGJ5dGVz</Binary>
      <Binary ID="1" Compressed="False">bWluZSBhbG9uZQ==</Binary>
    </Binaries>
  </Meta>
  <Root>
    <Group>
      <UUID>c2hhcmVkLXJvb3QtLS0tLQ==</UUID>
      <Name>shared</Name>
      <Times><CreationTime>2020-01-01T00:00:00Z</CreationTime><Expires>False</Expires></Times>
      <Entry>
        <UUID>c2hhcmVkLWEtLS0tLS0tLQ==</UUID>
        <Times><CreationTime>2020-01-01T00:00:00Z</CreationTime><Expires>False</Expires></Times>
        <String><Key>Title</Key><Value>a</Value></String>
        <QualityCheck>False</QualityCheck>
        <Binary><Key>together.bin</Key><Value Ref="0"/></Binary>
      </Entry>
      <Entry>
        <UUID>c2hhcmVkLWItLS0tLS0tLQ==</UUID>
        <Times><CreationTime>2020-01-01T00:00:00Z</CreationTime><Expires>False</Expires></Times>
        <String><Key>Title</Key><Value>b</Value></String>
        <Binary><Key>together.bin</Key><Value Ref="0"/></Binary>
      </Entry>
      <Entry>
        <UUID>c2hhcmVkLWMtLS0tLS0tLQ==</UUID>
        <Times><CreationTime>2020-01-01T00:00:00Z</CreationTime><Expires>False</Expires></Times>
        <String><Key>Title</Key><Value>c</Value></String>
        <Binary><Key>together.bin</Key><Value Ref="0"/></Binary>
      </Entry>
      <Entry>
        <UUID>c2hhcmVkLWQtLS0tLS0tLQ==</UUID>
        <Times><CreationTime>2020-01-01T00:00:00Z</CreationTime><Expires>False</Expires></Times>
        <String><Key>Title</Key><Value>d</Value></String>
        <Binary><Key>together.bin</Key><Value Ref="0"/></Binary>
        <Binary><Key>alone.bin</Key><Value Ref="1"/></Binary>
      </Entry>
    </Group>
  </Root>
</KeePassFile>
"#;

    let path = support::imported(&tool, scratch.path(), "shared", xml, SECRET);

    {
        let vault = open(&path, SECRET);
        // `QualityCheck` is what makes keepassxc write KDBX 4.1, which is the
        // format that keeps its files where Coffer can read them.
        assert!(
            !vault.is_read_only(),
            "the import came back in a format Coffer will not write"
        );
        assert_eq!(
            files(&vault).len(),
            5,
            "the import did not keep one file under four names"
        );
    }

    // One name at a time, and after each one every other name still has to give
    // back the bytes it named.
    for gone in ["a", "b", "c"] {
        {
            let mut vault = open(&path, SECRET);
            let id = only_entry(&vault, gone);
            vault
                .remove_attachment(id, "together.bin")
                .expect("the name comes off");
            vault.save().expect("the database saves");
        }

        let vault = open(&path, SECRET);
        for kept in ["a", "b", "c", "d"] {
            let id = only_entry(&vault, kept);
            let entry = vault.entry(id).expect("the entry is there");
            let has = entry
                .attachments
                .iter()
                .any(|attachment| attachment.name == "together.bin");

            if kept <= gone {
                assert!(!has, "{kept} still names the file after {gone} let it go");
                continue;
            }
            assert!(has, "{kept} lost the file when {gone} let it go");
            assert_eq!(
                vault
                    .attachment(id, "together.bin")
                    .expect("the file is there")
                    .expose(),
                b"shared bytes",
                "{kept} came back holding somebody else's bytes"
            );
        }

        let id = only_entry(&vault, "d");
        assert_eq!(
            vault
                .attachment(id, "alone.bin")
                .expect("the file is there")
                .expose(),
            b"mine alone",
            "the file nobody shared moved onto the wrong entry"
        );
    }
}

/// A folder can hold the recycle bin - the format puts the bin wherever the
/// client that made it put it, and the tree pane already draws one that way.
/// Deleting that folder takes the bin with it rather than trying to move the
/// folder inside something it contains.
#[test]
fn a_folder_that_holds_the_recycle_bin_can_still_be_deleted() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "nested-bin.kdbx", |database| {
        let bin = {
            let mut root = database.root_mut();
            let mut project = root.add_group();
            project.name = "Work".to_owned();
            let mut bin = project.add_group();
            bin.name = "Recycle Bin".to_owned();
            bin.add_entry()
                .edit(|entry| entry.set_unprotected(fields::TITLE, "thrown away"));
            bin.id()
        };

        database.meta.recyclebin_enabled = Some(true);
        database.meta.recyclebin_uuid = Some(bin.uuid());
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    let project = only_group(&vault, "Work");
    assert!(
        vault
            .tree()
            .sections
            .iter()
            .any(|section| section.sections.iter().any(|inner| inner.is_recycle_bin)),
        "the fixture is supposed to hold the bin inside a folder"
    );

    vault.delete_group(project).expect("the folder is deleted");
    assert!(
        vault.tree().sections.iter().all(|s| s.name != "Work"),
        "the folder that held the bin could not be deleted"
    );
    vault.save().expect("the database saves");
    drop(vault);

    // The bin went with it, and nothing in the file still names one.
    let vault = open(&path, BUILT_PASSWORD);
    assert!(
        vault
            .tree()
            .sections
            .iter()
            .all(|section| !section.is_recycle_bin)
    );
}

/// The whole database is read into memory to be opened, so a vault past the
/// ceiling is a vault nobody can open again. It is measured on the way out,
/// where the file it would replace is still untouched.
///
/// The database is written uncompressed, so that the size of the file is the
/// size of what went into it and the test does not spend minutes deflating a
/// gigabyte to prove a bound that has nothing to do with compression.
#[test]
fn a_database_that_would_be_too_large_to_open_again_is_not_written() {
    use keepass::config::CompressionConfig;

    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "large.kdbx", |database| {
        database.config.compression_config = CompressionConfig::None;
    });

    let before = std::fs::metadata(&path).expect("the file is there").len();

    let mut vault = open(&path, BUILT_PASSWORD);
    let root = root_of(&vault);
    let id = vault.create_entry(root).expect("the entry is made");

    // Five files of a quarter of a gigabyte: each one is inside the limit on a
    // single file, and together they are past the limit on the database.
    for round in 0..5u8 {
        vault
            .add_attachment(
                id,
                &format!("big-{round}.bin"),
                Zeroizing::new(vec![round; 256 * 1024 * 1024]),
            )
            .expect("each file on its own is allowed");
    }

    assert!(matches!(vault.save(), Err(VaultError::TooLarge)));
    assert_eq!(
        std::fs::metadata(&path).expect("the file is there").len(),
        before,
        "the database was written even though it could not be opened again"
    );
    drop(vault);

    // And it still opens, because nothing was written.
    let vault = open(&path, BUILT_PASSWORD);
    assert!(vault.entry(id).is_none());
}
