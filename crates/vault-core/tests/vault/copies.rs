//! Copying an entry: what the copy holds, what it does not, and that the pool
//! of files stays whole around it.

use keepass::db::{MemoryProtection, Value, fields};
use vault_core::model::{Deletion, EntryId, FieldValue};
use vault_core::{Vault, VaultError};

use crate::support::{
    self, BUILT_PASSWORD, RICH, SECRET, built, entry_titled, files, library, made, open, reopened,
};

/// The title the vault gives an entry, read the way a reveal reads it, so a
/// protected one says what it is too.
fn title(vault: &Vault, id: EntryId) -> Option<String> {
    vault
        .reveal(id, fields::TITLE)?
        .expose_str()
        .map(str::to_owned)
}

/// The files one entry carries, by name, with their bytes.
fn files_of(vault: &Vault, id: EntryId) -> Vec<(String, Vec<u8>)> {
    let entry = vault.entry(id).expect("the entry is there");
    entry
        .attachments
        .iter()
        .map(|file| {
            let bytes = vault.attachment(id, &file.name).expect("the file is there");
            (file.name.clone(), bytes.expose().to_vec())
        })
        .collect()
}

/// A copy holds everything the entry holds - every field under its own
/// protection, the notes protected the way the fixture keeps them, tags, the
/// icon, both colours, the override URL, the quality check, the expiry,
/// auto-type and custom data - beside it in the same folder, called what it
/// is called with " copy" after it.
#[test]
fn a_copy_holds_every_field_with_its_protection() {
    let (_scratch, path) = support::scratch(RICH);
    let mut vault = open(&path, SECRET);
    let basic = entry_titled(&vault, "basic");

    let copy = vault.duplicate_entry(basic.id).expect("the copy is made");
    assert_ne!(copy, basic.id);
    let made = vault.entry(copy).expect("the copy is there");
    assert_eq!(made.group, basic.group, "the copy went somewhere else");
    assert_eq!(title(&vault, copy).as_deref(), Some("basic copy"));
    assert_eq!(
        made.field(fields::NOTES).map(|field| &field.value),
        Some(&FieldValue::Protected {
            empty: false,
            lines: true
        })
    );
    drop(reopened(vault, SECRET));

    let file = library(&path, SECRET);
    let mut original = support::holding(&file.entry(basic.id).expect("the entry is there"));
    let copied = support::holding(&file.entry(copy).expect("the copy is there"));
    original.fields.insert(
        fields::TITLE.to_owned(),
        Value::unprotected("basic copy".to_owned()),
    );
    assert_eq!(copied, original);
}

/// A copy's files are its own. Each is a file of its own in the pool, so
/// either entry can lose one without the other noticing, and the pool closes up
/// behind it every time: the bytes stay on the entry they belong to through
/// every save and every reading of the file.
#[test]
fn a_copy_has_files_of_its_own_and_the_pool_stays_unbroken() {
    let (_scratch, path) = support::scratch(RICH);
    let mut vault = open(&path, SECRET);
    let key = entry_titled(&vault, "ssh key").id;
    let before = files(&vault);

    let copy = vault.duplicate_entry(key).expect("the copy is made");
    vault = reopened(vault, SECRET);
    assert_eq!(files_of(&vault, copy), files_of(&vault, key));

    vault
        .remove_attachment(copy, "id_ed25519")
        .expect("the copy's file goes");
    vault = reopened(vault, SECRET);
    let mut after = files(&vault);
    after.retain(|(title, _, _)| title != "ssh key copy");
    assert_eq!(after, before, "the original lost a file with its copy's");

    vault
        .remove_attachment(key, "KeeAgent.settings")
        .expect("the original's file goes");
    vault = reopened(vault, SECRET);
    let kept = files_of(&vault, copy);
    assert_eq!(kept.len(), 1);
    assert_eq!(
        Some(&kept[0]),
        before
            .iter()
            .find(|(title, name, _)| title == "ssh key" && name == "KeeAgent.settings")
            .map(|(_, name, bytes)| (name.clone(), bytes.clone()))
            .as_ref(),
        "the copy lost its file with the original's"
    );
}

/// The fixture's "attachments" entry gives one file two names, which only
/// another client can write. Its copy gets a file for each name, so taking one
/// off the copy is never the refusal a file two names share earns, and the
/// original keeps both its names on the one file.
#[test]
fn a_copy_of_an_entry_that_names_one_file_twice_never_shares_it() {
    let (_scratch, path) = support::scratch(RICH);
    let mut vault = open(&path, SECRET);
    let shared = entry_titled(&vault, "attachments").id;
    let original = files_of(&vault, shared);

    let copy = vault.duplicate_entry(shared).expect("the copy is made");
    assert_eq!(files_of(&vault, copy), original);

    for name in ["zero-byte.txt", "../../escape.txt"] {
        vault
            .remove_attachment(copy, name)
            .unwrap_or_else(|error| panic!("{name} would not come off the copy: {error}"));
    }
    vault = reopened(vault, SECRET);
    assert_eq!(files_of(&vault, shared), original);
    assert_eq!(files_of(&vault, copy).len(), original.len() - 2);
}

/// A copy has no past and is made now: no versions, every date the moment it
/// was made - the fixture's 1600 and 3000 are not carried - and nobody has
/// used it yet. The expiry is the reader's choice and is carried, three
/// thousand and all.
#[test]
fn a_copy_keeps_no_versions_and_is_dated_now_but_keeps_its_expiry() {
    let (_scratch, path) = support::scratch(RICH);
    let mut vault = open(&path, SECRET);
    let versioned = entry_titled(&vault, "versioned");
    let extreme = entry_titled(&vault, "extreme timestamps");
    assert!(versioned.versions > 0, "the fixture entry has history");

    let started = chrono::Utc::now().naive_utc() - chrono::Duration::seconds(1);
    let copies = [
        vault
            .duplicate_entry(versioned.id)
            .expect("the copy is made"),
        vault.duplicate_entry(extreme.id).expect("the copy is made"),
    ];
    let ended = chrono::Utc::now().naive_utc() + chrono::Duration::seconds(1);
    assert_eq!(vault.versions(versioned.id).len(), versioned.versions);
    drop(reopened(vault, SECRET));

    let file = library(&path, SECRET);
    for copy in copies {
        let entry = file.entry(copy).expect("the copy is in the file");
        assert_eq!(
            entry
                .history
                .as_ref()
                .map(|history| history.get_entries().len()),
            Some(0)
        );
        for (name, moment) in [
            ("created", entry.times.creation),
            ("modified", entry.times.last_modification),
            ("looked at", entry.times.last_access),
            ("moved", entry.times.location_changed),
        ] {
            assert!(
                moment.is_some_and(|moment| started <= moment && moment <= ended),
                "the copy says it was {name} at another time: {moment:?}"
            );
        }
        assert_eq!(entry.times.usage_count, Some(0));
    }

    let carried = file.entry(copies[1]).expect("the copy is in the file");
    let source = file.entry(extreme.id).expect("the entry is in the file");
    assert_eq!(carried.times.expires, Some(true));
    assert_eq!(carried.times.expiry, source.times.expiry);

    let plain = file.entry(copies[0]).expect("the copy is in the file");
    let source = file.entry(versioned.id).expect("the entry is in the file");
    assert_eq!(plain.times.expires, source.times.expires);
    assert_eq!(plain.times.expiry, source.times.expiry);
}

/// A file may leave an entry's expiry date out, which every client then fills
/// in its own way. A copy of such an entry is given the one everything Coffer
/// makes is given: the moment it was made, beside the fact that it never
/// expires.
#[test]
fn an_entry_with_no_expiry_date_makes_a_copy_dated_when_it_was_made() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "undated.kdbx", |database| {
        database
            .root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "undated"));
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let undated = entry_titled(&vault, "undated").id;
    let started = chrono::Utc::now().naive_utc() - chrono::Duration::seconds(1);
    let copy = vault.duplicate_entry(undated).expect("the copy is made");
    let ended = chrono::Utc::now().naive_utc() + chrono::Duration::seconds(1);
    drop(reopened(vault, BUILT_PASSWORD));

    let file = library(&path, BUILT_PASSWORD);
    assert_eq!(
        file.entry(undated).map(|entry| entry.times.expiry),
        Some(None)
    );
    let dated = file.entry(copy).expect("the copy is in the file");
    assert_eq!(dated.times.expires, Some(false));
    assert!(
        dated
            .times
            .expiry
            .is_some_and(|moment| started <= moment && moment <= ended),
        "{:?}",
        dated.times.expiry
    );
}

/// An entry with no title makes a copy with none, rather than one called by
/// the window's word for nothing; one with no title field at all makes a copy
/// without one. A copy of a copy says so twice.
#[test]
fn an_untitled_entry_is_copied_untitled_and_a_copy_of_a_copy_says_so_twice() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "titles.kdbx", |database| {
        let mut root = database.root_mut();
        root.add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, ""));
        root.add_entry()
            .edit(|entry| entry.set_unprotected(fields::USERNAME, "no title at all"));
        root.add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "basic"));
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let tree = vault.tree();
    let id_of = |wanted: &str| {
        tree.entries
            .iter()
            .find(|row| row.title.open() == Some(wanted) || row.username.open() == Some(wanted))
            .map(|row| row.id)
            .expect("the entry is there")
    };
    let (empty, absent, basic) = (id_of(""), id_of("no title at all"), id_of("basic"));

    let copy = vault.duplicate_entry(empty).expect("the copy is made");
    assert_eq!(title(&vault, copy).as_deref(), Some(""));
    let copy = vault.duplicate_entry(absent).expect("the copy is made");
    assert_eq!(title(&vault, copy), None, "a title was made up");

    let once = vault.duplicate_entry(basic).expect("the copy is made");
    let twice = vault.duplicate_entry(once).expect("the copy is made");
    assert_eq!(title(&vault, twice).as_deref(), Some("basic copy copy"));
}

/// A database that protects titles keeps the copy's protected too, and the
/// copy's title is made in Rust: nothing of it crosses to be named.
#[test]
fn a_protected_title_is_copied_protected_and_named_in_rust() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "protected-titles.kdbx", |database| {
        database.meta.memory_protection = Some(MemoryProtection {
            protect_title: true,
            ..MemoryProtection::default()
        });
        database
            .root_mut()
            .add_entry()
            .edit(|entry| entry.set_protected(fields::TITLE, "Chase \u{202e} evil"));
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let original = vault.tree().entries[0].id;

    let copy = vault.duplicate_entry(original).expect("the copy is made");
    vault = reopened(vault, BUILT_PASSWORD);
    let made = vault.entry(copy).expect("the copy is there");
    assert!(matches!(
        made.field(fields::TITLE).map(|field| &field.value),
        Some(FieldValue::Protected { empty: false, .. })
    ));
    assert_eq!(
        title(&vault, copy).as_deref(),
        Some("Chase \u{202e} evil copy")
    );
}

/// A copy names the custom icon the entry does, and keeps it when the entry it
/// was copied from is erased: the icon is in the file for as long as anything
/// names it, and the copy does.
#[test]
fn a_copy_keeps_a_custom_icon_after_the_original_is_erased() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "icons.kdbx", |database| {
        let id = database
            .root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "iconic"))
            .id();
        database
            .entry_mut(id)
            .expect("the entry is there")
            .set_icon_custom_new(vec![1, 2, 3, 4]);
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let original = entry_titled(&vault, "iconic").id;
    let copy = vault.duplicate_entry(original).expect("the copy is made");

    vault
        .delete_entries(&[(original, Deletion::Bin)])
        .expect("it goes to the bin");
    vault
        .delete_entries(&[(original, Deletion::Forever)])
        .expect("and out of the file");
    drop(reopened(vault, BUILT_PASSWORD));

    let file = library(&path, BUILT_PASSWORD);
    let kept = file.entry(copy).expect("the copy is there");
    assert_eq!(
        kept.custom_icon().map(|icon| icon.data.clone()),
        Some(vec![1, 2, 3, 4])
    );
}

/// KeeAgent keeps its settings for a key in custom data and in a file beside
/// the key, and a copy of the key that lost either would be a key KeeAgent
/// no longer offers.
#[test]
fn a_copy_of_an_ssh_key_carries_its_keeagent_settings() {
    let (_scratch, path) = support::scratch(RICH);
    let mut vault = open(&path, SECRET);
    let key = entry_titled(&vault, "ssh key").id;
    let copy = vault.duplicate_entry(key).expect("the copy is made");
    drop(reopened(vault, SECRET));

    let file = library(&path, SECRET);
    let original = file.entry(key).expect("the entry is there");
    let copied = file.entry(copy).expect("the copy is there");
    assert_eq!(copied.custom_data, original.custom_data);
    assert!(copied.custom_data.contains_key("KeeAgent.Settings"));
    let settings = |entry: &keepass::db::EntryRef<'_>| {
        entry
            .attachment_by_name("KeeAgent.settings")
            .map(|file| file.data.get().clone())
    };
    assert!(settings(&copied).is_some());
    assert_eq!(settings(&copied), settings(&original));
}

/// Nothing is copied out of the recycle bin, where the copy would be a deleted
/// entry nobody deleted; nothing in a vault Coffer will not write; and nothing
/// from an entry that is not there. Nothing changes on the way.
#[test]
fn nothing_is_copied_out_of_the_bin_out_of_a_read_only_vault_or_from_nowhere() {
    let (_scratch, path) = support::scratch(RICH);
    let mut vault = open(&path, SECRET);
    let deleted = entry_titled(&vault, "deleted entry").id;
    let root = vault.tree().id;
    let gone = made(&mut vault, root, "gone");
    vault
        .delete_entries(&[(gone, Deletion::Bin)])
        .expect("it goes to the bin");
    vault
        .delete_entries(&[(gone, Deletion::Forever)])
        .expect("and out of the file");

    let tree = vault.tree();
    let (count, edits) = (vault.count(), vault.edits());
    assert!(matches!(
        vault.duplicate_entry(deleted),
        Err(VaultError::IntoRecycleBin)
    ));
    assert!(matches!(
        vault.duplicate_entry(gone),
        Err(VaultError::NoSuchEntry)
    ));
    assert_eq!(vault.tree(), tree);
    assert_eq!((vault.count(), vault.edits()), (count, edits));
    vault.save().expect("the database saves");
    drop(vault);

    let snapshot = vault_core::storage::snapshot::slot(&path, 1).expect("a slot has a name");
    let (_older, third) = support::scratch("rich-kdbx31.kdbx");
    for (path, read_only) in [
        (snapshot, VaultError::ReadOnlySnapshot.to_string()),
        (third, VaultError::ReadOnlyKdbx3Attachments.to_string()),
    ] {
        let mut vault = open(&path, SECRET);
        let basic = entry_titled(&vault, "basic").id;
        let refused = vault
            .duplicate_entry(basic)
            .expect_err("a read-only vault made a copy");
        assert_eq!(refused.to_string(), read_only);
    }
}

/// A hundred copies of an entry carrying a file, copies of copies among them,
/// save and come back from the file whole: every one with the file's bytes.
/// Saving them again, with nothing new, does not grow the file.
#[test]
fn a_hundred_copies_of_an_entry_with_a_file_save_and_reopen_whole() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "hundred.kdbx", |database| {
        database
            .root_mut()
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "Server"))
            .add_attachment("cert.pem", Value::protected(vec![0x42; 3 * 1024]));
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let original = entry_titled(&vault, "Server").id;

    let mut latest = original;
    for round in 0..100 {
        let from = if round % 2 == 0 { original } else { latest };
        latest = vault.duplicate_entry(from).expect("the copy is made");
        if round % 10 == 9 {
            vault.save().expect("the database saves");
        }
    }
    let before = std::fs::metadata(&path).expect("the file is there").len();
    vault = reopened(vault, BUILT_PASSWORD);

    let carried = files(&vault);
    assert_eq!(carried.len(), 101);
    assert!(
        carried
            .iter()
            .all(|(_, name, bytes)| name == "cert.pem" && bytes == &vec![0x42; 3 * 1024])
    );
    let after = std::fs::metadata(&path).expect("the file is there").len();
    assert!(
        after <= before + 64 * 1024,
        "a save with nothing new grew the file from {before} to {after} bytes"
    );
}

/// Two hundred fields of the reader's own, half of them hidden, a megabyte of
/// notes holding XML's own characters and a right-to-left override, a file of
/// no bytes and one named like a path: the copy has every one of them, read
/// back from the file.
#[test]
fn a_copy_with_200_fields_a_megabyte_of_notes_and_a_zero_byte_file_loses_nothing() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let notes: String = "<b>&amp;</b> \u{202e}reversed\u{202c} "
        .chars()
        .cycle()
        .take(1024 * 1024)
        .collect();
    let path = built(scratch.path(), "heavy.kdbx", |database| {
        let id = database
            .root_mut()
            .add_entry()
            .edit(|entry| {
                entry.times.expiry = chrono::NaiveDate::from_ymd_opt(2031, 1, 1)
                    .and_then(|day| day.and_hms_opt(0, 0, 0));
                entry.set_unprotected(fields::TITLE, "heavy");
                entry.set_unprotected(fields::NOTES, notes.clone());
                for field in 0..200 {
                    let name = format!("field {field:03}");
                    if field % 2 == 0 {
                        entry.set_protected(name, format!("secret {field}"));
                    } else {
                        entry.set_unprotected(name, format!("open {field}"));
                    }
                }
            })
            .id();
        let mut entry = database.entry_mut(id).expect("the entry is there");
        entry.add_attachment("empty", Value::protected(Vec::new()));
        entry.add_attachment("../../etc/passwd", Value::protected(b"root".to_vec()));
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let heavy = entry_titled(&vault, "heavy").id;
    let copy = vault.duplicate_entry(heavy).expect("the copy is made");
    vault = reopened(vault, BUILT_PASSWORD);
    assert_eq!(files_of(&vault, copy), files_of(&vault, heavy));
    drop(vault);

    let file = library(&path, BUILT_PASSWORD);
    let mut original = support::holding(&file.entry(heavy).expect("the entry is there"));
    let copied = support::holding(&file.entry(copy).expect("the copy is there"));
    original.fields.insert(
        fields::TITLE.to_owned(),
        Value::unprotected("heavy copy".to_owned()),
    );
    assert_eq!(copied.fields.len(), 202);
    assert!(copied == original, "the copy lost something of the entry");
}

/// KeePassXC reads a copy as an entry of its own: listed beside the original,
/// with the original's files byte for byte, and no versions.
#[test]
fn copies_read_in_keepassxc() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let (scratch, path) = support::scratch(RICH);
    let mut vault = open(&path, SECRET);
    let key = entry_titled(&vault, "ssh key").id;
    let versioned = entry_titled(&vault, "versioned").id;
    vault.duplicate_entry(key).expect("the copy is made");
    vault.duplicate_entry(versioned).expect("the copy is made");
    vault.save().expect("the database saves");
    drop(vault);

    let listed = support::entry_paths(&tool, &path, SECRET, None);
    for title in ["Work/ssh key copy", "Versioned/versioned copy"] {
        assert!(
            listed.iter().any(|path| path == title),
            "{title}: {listed:?}"
        );
    }

    for name in ["id_ed25519", "KeeAgent.settings"] {
        let into = scratch.path().join(format!("{name}.exported"));
        let original =
            support::attachment_bytes(&tool, &path, SECRET, None, "Work/ssh key", name, &into);
        let copied =
            support::attachment_bytes(&tool, &path, SECRET, None, "Work/ssh key copy", name, &into);
        assert_eq!(copied, original, "{name}");
    }

    let exported = support::export(&tool, &path, SECRET, None);
    let entry =
        support::exported_entry(&exported, "versioned copy").expect("the copy is in the export");
    assert!(
        !entry.contains("<History>") || entry.contains("<History/>"),
        "KeePassXC reads versions on the copy"
    );
}
