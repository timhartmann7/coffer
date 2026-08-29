//! Making, changing and deleting what is in a database, and the pool of files
//! underneath it.

use keepass::db::fields;
use vault_core::model::{EntryId, FieldValue, GroupId};
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
        // An edit after the file was added writes a version that holds it.
        vault
            .set_field(id, fields::NOTES, NewValue::Open("edited".to_owned()))
            .expect("the note is written");
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
        vault.attachment(id, "key.pem").is_some(),
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
