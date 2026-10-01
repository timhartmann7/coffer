//! Reading a database: what comes back, and what deliberately does not.

use vault_core::model::{EntryId, FieldValue, GroupId, fields};
use vault_core::{LockPolicy, VaultError};

use crate::support::{self, RICH, SECRET, all_entries, entry_titled, fixture, open, password};

#[test]
fn the_tree_holds_every_group_and_entry() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let tree = vault.tree();

    let mut names: Vec<String> = Vec::new();
    fn walk(group: &vault_core::model::Project, into: &mut Vec<String>) {
        into.push(group.name.clone());
        for section in &group.sections {
            walk(section, into);
        }
    }
    walk(&tree, &mut names);
    names.sort();

    assert_eq!(
        names,
        vec![
            "Coffer",
            "Personal",
            "Recycle Bin",
            "Versioned",
            "Work",
            "level 1",
            "level 2",
            "level 3",
        ]
    );

    assert_eq!(all_entries(&vault).len(), 11);
    assert_eq!(tree.notes.as_deref(), Some("root group notes"));
    // A group's empty <Notes/> comes back absent rather than empty. KeePassXC
    // writes the empty element for both, so the round trip cannot tell them
    // apart either and nothing is lost through it. Entry field values are a
    // different path, and an empty one there does stay empty.
    assert_eq!(
        tree.sections
            .iter()
            .find(|section| section.name == "Personal")
            .and_then(|section| section.notes.clone()),
        None
    );
}

#[test]
fn every_entry_names_the_group_it_hangs_under_and_the_names_survive_a_save() {
    let (_scratch, database) = support::scratch(RICH);

    fn collect(group: &vault_core::model::Project, into: &mut Vec<(GroupId, EntryId)>) {
        for entry in &group.entries {
            assert_eq!(
                entry.group, group.id,
                "an entry in {:?} says it belongs to another group",
                group.name
            );
            into.push((group.id, entry.id));
        }
        for section in &group.sections {
            collect(section, into);
        }
    }

    let mut before = Vec::new();
    {
        let mut vault = open(&database, SECRET);
        collect(&vault.tree(), &mut before);
        vault.save().expect("the database saves");
    }

    let mut unique: Vec<GroupId> = before.iter().map(|(group, _)| *group).collect();
    unique.sort_by_key(|group| group.uuid());
    unique.dedup();
    assert!(unique.len() >= 5, "the fixture should span several groups");

    let mut after = Vec::new();
    collect(&open(&database, SECRET).tree(), &mut after);

    before.sort_by_key(|(group, entry)| (group.uuid(), entry.uuid()));
    after.sort_by_key(|(group, entry)| (group.uuid(), entry.uuid()));
    assert_eq!(
        before, after,
        "a save moved an entry to a different group, or changed an identifier"
    );
}

#[test]
fn previous_versions_never_appear_in_the_tree() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);

    let versioned = entry_titled(&vault, "versioned");
    assert_eq!(versioned.versions, 6, "the fixture carries six versions");

    // Six versions of an entry titled "versioned" exist in the file. Exactly one
    // entry with that title may be reachable through the tree.
    let reachable = all_entries(&vault)
        .into_iter()
        .filter(|entry| entry.title.open() == Some("versioned"))
        .count();
    assert_eq!(reachable, 1);

    let versions = vault.versions(versioned.id);
    assert_eq!(versions.len(), 6);

    // Oldest first, as the file lists them.
    let dates: Vec<_> = versions.iter().map(|version| version.modified).collect();
    let mut sorted = dates.clone();
    sorted.sort();
    assert_eq!(dates, sorted, "versions come back oldest first");
}

#[test]
fn the_recycle_bin_is_marked() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);

    fn find(group: &vault_core::model::Project) -> Option<vault_core::model::Project> {
        if group.is_recycle_bin {
            return Some(group.clone());
        }
        group.sections.iter().find_map(find)
    }

    let bin = find(&vault.tree()).expect("the fixture nominates a recycle bin");
    assert_eq!(bin.name, "Recycle Bin");
    assert_eq!(bin.entries.len(), 1);
    assert_eq!(bin.entries[0].title.open(), Some("deleted entry"));
}

#[test]
fn the_tree_carries_nothing_a_list_does_not_need() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);

    // The tree and the entry list get the four things they filter on. Notes and
    // custom field values are the user's data and stay in the database until a
    // screen asks for one entry.
    let rendered = format!("{:?}", vault.tree());
    for absent in [
        "first line",
        "second line",
        "value 1",
        "meta-custom",
        "correct horse",
        "javascript:",
    ] {
        assert!(
            !rendered.contains(absent),
            "the tree carried {absent:?}, which a list has no use for"
        );
    }

    let summary = all_entries(&vault)
        .into_iter()
        .find(|entry| entry.title.open() == Some("basic"))
        .expect("the entry is in the tree");
    assert_eq!(summary.username.open(), Some("alice"));
    assert_eq!(summary.tags, vec!["alpha", "beta", "gamma"]);
    assert!(summary.has_password);
    assert_eq!(summary.attachments, 0);
    assert_eq!(summary.versions, 0);

    // The same entry asked for by itself does carry them.
    let whole = vault.entry(summary.id).expect("the entry is there");
    assert_eq!(
        whole
            .field("Notes")
            .map(|field| matches!(field.value, vault_core::model::FieldValue::Protected { .. })),
        Some(true)
    );
    assert_eq!(
        vault
            .entry(entry_titled(&vault, "many custom fields").id)
            .expect("the entry is there")
            .field("custom-002")
            .and_then(|field| field.value.open()),
        Some("value 2"),
        "an entry asked for by itself carries its custom fields"
    );
}

#[test]
fn a_protected_title_leaves_the_list_with_no_title_rather_than_an_empty_one() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);

    // The fixture protects Notes, which is the same mechanism.
    let entry = entry_titled(&vault, "basic");
    let summary = entry.summary();
    assert_eq!(summary.title.open(), Some("basic"));
    assert_eq!(
        entry.field(fields::NOTES).map(|field| field.value.clone()),
        Some(FieldValue::Protected {
            empty: false,
            lines: true
        })
    );
    assert_eq!(
        entry
            .field(fields::NOTES)
            .and_then(|field| field.value.open()),
        None,
        "a protected value must never come back as an empty string"
    );
}

#[test]
fn a_protected_field_arrives_without_its_value() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let entry = entry_titled(&vault, "basic");

    let password_field = entry.field(fields::PASSWORD).expect("a password field");
    assert_eq!(
        password_field.value,
        FieldValue::Protected {
            empty: false,
            lines: false
        }
    );
    assert!(entry.has_password());

    // The rendered form of the metadata must not contain the value either.
    let rendered = format!("{entry:?}");
    assert!(
        !rendered.contains("correct horse"),
        "entry metadata printed a protected value"
    );
    assert!(
        !rendered.contains("alice"),
        "entry metadata printed a field value"
    );
}

#[test]
fn an_unprotected_field_arrives_with_its_value() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let entry = entry_titled(&vault, "basic");

    assert_eq!(entry.username(), "alice");
    assert_eq!(entry.url(), "https://example.com/login?a=1&b=2");
    assert_eq!(entry.tags, vec!["alpha", "beta", "gamma"]);
}

#[test]
fn revealing_hands_back_the_value_and_nothing_else_does() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let entry = entry_titled(&vault, "basic");

    let revealed = vault
        .reveal(entry.id, fields::PASSWORD)
        .expect("the password reveals");
    assert_eq!(revealed.expose_str(), Some("correct horse battery staple"));
    assert_eq!(format!("{revealed:?}"), "[redacted]");

    assert!(vault.reveal(entry.id, "no such field").is_none());
}

#[test]
fn a_protected_notes_field_is_still_protected() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let entry = entry_titled(&vault, "basic");

    let notes = entry.field(fields::NOTES).expect("a notes field");
    assert_eq!(
        notes.value,
        FieldValue::Protected {
            empty: false,
            lines: true
        },
        "a note in two lines does not say so"
    );
    assert_eq!(
        vault
            .reveal(entry.id, fields::NOTES)
            .and_then(|value| value.expose_str().map(str::to_owned)),
        Some("first line\nsecond line".to_owned())
    );
}

#[test]
fn attachments_come_back_with_their_bytes_intact() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let entry = entry_titled(&vault, "attachments");

    let names: Vec<&str> = entry
        .attachments
        .iter()
        .map(|attachment| attachment.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "../../escape.txt",
            "all-256-bytes.bin",
            "nested/path/name.txt",
            "zero-byte.txt",
        ]
    );

    let every_byte = vault
        .attachment(entry.id, "all-256-bytes.bin")
        .expect("the attachment is there");
    assert_eq!(every_byte.expose(), (0u8..=255).collect::<Vec<u8>>());

    let empty = vault
        .attachment(entry.id, "zero-byte.txt")
        .expect("the zero-byte attachment is there");
    assert!(empty.expose().is_empty());
}

#[test]
fn whitespace_inside_a_value_survives_but_a_value_that_is_only_whitespace_does_not() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let entry = entry_titled(&vault, "many custom fields");

    let read = |name: &str| {
        vault
            .reveal(entry.id, name)
            .and_then(|value| value.expose_str().map(str::to_owned))
    };

    assert_eq!(read("padded"), Some("  leading and trailing  ".to_owned()));
    assert_eq!(read("newlines"), Some("a\nb\n".to_owned()));
    assert_eq!(read("whitespace-protected"), Some(" \t ".to_owned()));

    // The one that does not survive has a fixture of its own, so that this
    // limitation is a fact the suite states rather than one it hides.
    let (_blank, blank) = support::scratch("whitespace-kdbx41.kdbx");
    let vault = open(&blank, SECRET);
    let entry = entry_titled(&vault, "blank values");

    let read = |name: &str| {
        vault
            .reveal(entry.id, name)
            .and_then(|value| value.expose_str().map(str::to_owned))
    };
    assert_eq!(
        read("spaces-only"),
        Some(String::new()),
        "the library kept a whitespace-only value: this limitation is over"
    );
    assert_eq!(read("tab-only"), Some(String::new()));
    assert_eq!(read("padded"), Some("  kept  ".to_owned()));
}

/// A protected value says whether it is written in lines, so that a new one
/// can be written the same way, and says nothing else about itself. A line
/// feed, a CRLF and a CR on its own are each a break the way a text area reads
/// them, and a value with none is one line however long it is.
#[test]
fn a_protected_value_says_whether_it_is_in_lines_and_nothing_more() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let database = support::built(scratch.path(), "lines.kdbx", |db| {
        db.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "codes");
            entry.set_protected("LF", "0451-7719\n2231-0098");
            entry.set_protected("CRLF", "0451-7719\r\n2231-0098");
            entry.set_protected("CR", "0451-7719\r2231-0098");
            entry.set_protected("Long", "0451-7719 ".repeat(10_000));
            entry.set_protected("Empty", "");
            entry.set_unprotected("Open", "first\nsecond");
        });
    });
    let vault = open(&database, support::BUILT_PASSWORD);
    let entry = entry_titled(&vault, "codes");

    for (name, lines) in [
        ("LF", true),
        ("CRLF", true),
        ("CR", true),
        ("Long", false),
        ("Empty", false),
        ("Open", true),
    ] {
        assert_eq!(
            entry.field(name).map(vault_core::model::Field::in_lines),
            Some(lines),
            "{name}"
        );
    }
    let printed = format!("{entry:?}");
    assert!(!printed.contains("0451"), "{printed}");
}

#[test]
fn empty_values_survive_as_empty_rather_than_absent() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let entry = entry_titled(&vault, "many custom fields");

    let open_field = entry
        .field("empty-unprotected")
        .expect("the field is there");
    assert_eq!(open_field.value, FieldValue::Open(String::new()));

    let protected = entry.field("empty-protected").expect("the field is there");
    assert_eq!(
        protected.value,
        FieldValue::Protected {
            empty: true,
            lines: false
        }
    );
    assert!(protected.is_empty());
}

#[test]
fn unicode_and_markup_come_back_byte_for_byte() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);

    let unicode = entry_titled(&vault, "ユニコード 🔐 é\u{301}");
    assert_eq!(unicode.username(), "עברית \u{202e}reversed\u{202c}");
    assert_eq!(unicode.url(), "https://例.テスト/");

    let markup = entry_titled(&vault, "<script>alert(1)</script>");
    assert_eq!(markup.username(), "&amp; &lt; &gt; &quot; &apos;");
}

#[test]
fn dangerous_url_schemes_are_handed_back_unchanged() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let entry = entry_titled(&vault, "dangerous urls");

    // vault-core stores what the file says. Deciding which of these may reach an
    // href is the screen's job, and it has to see them exactly as written.
    assert_eq!(entry.url(), "javascript:alert(document.domain)");
    for (name, expected) in [
        ("url-file", "file:///etc/passwd"),
        ("url-vbscript", "vbscript:msgbox(1)"),
        ("url-mixed-case", "JaVaScRiPt:alert(1)"),
        ("url-whitespace", "  javascript:alert(1)"),
    ] {
        let field = entry.field(name).expect("the field is there");
        assert_eq!(field.value, FieldValue::Open(expected.to_owned()));
    }
}

#[test]
fn timestamps_at_the_edges_of_the_range_survive() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let entry = entry_titled(&vault, "extreme timestamps");

    assert_eq!(
        entry.times.created.map(|moment| moment.to_string()),
        Some("1600-01-01 00:00:00".to_owned())
    );
    assert_eq!(
        entry.times.modified.map(|moment| moment.to_string()),
        Some("3000-12-31 23:59:59".to_owned())
    );
    assert_eq!(
        entry.times.expires.map(|moment| moment.to_string()),
        Some("3000-12-31 23:59:59".to_owned())
    );
}

#[test]
fn an_entry_that_does_not_expire_reports_no_expiry() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    assert_eq!(entry_titled(&vault, "attachments").times.expires, None);
}

#[test]
fn the_wrong_password_is_one_message_with_no_detail() {
    let (_scratch, database) = support::scratch(RICH);
    let error = vault_core::Vault::open(&database, password("wrong"), LockPolicy::Respect)
        .expect_err("the wrong password is refused");

    assert!(matches!(error, VaultError::WrongCredentials));
    assert_eq!(error.to_string(), "wrong password or key file");
}

/// The credentials a database opened only by its key file wants.
fn key_file_alone() -> vault_core::MasterKey {
    password("")
        .with_key_file(&fixture("keyfile-only.key"))
        .expect("the key file reads")
}

#[test]
fn a_database_that_wants_a_key_file_and_no_password_opens() {
    let (_scratch, database) = support::scratch("keyfile-only-kdbx41.kdbx");
    let vault = vault_core::Vault::open(&database, key_file_alone(), LockPolicy::Respect)
        .expect("the key file alone opens it");

    assert_eq!(all_entries(&vault).len(), 1);
}

/// The composition that opened the file has to be the one that writes it back.
/// A save that folded the empty password back in would put a database on disk
/// that neither Coffer nor KeePassXC could open again, with the reader's own
/// key file in their hand - every entry gone, and nothing on the screen to say
/// so until the next unlock.
#[test]
fn a_database_opened_by_its_key_file_alone_is_written_back_the_same_way() {
    let (_scratch, database) = support::scratch("keyfile-only-kdbx41.kdbx");
    let mut vault = vault_core::Vault::open(&database, key_file_alone(), LockPolicy::Respect)
        .expect("the key file alone opens it");

    let id = all_entries(&vault).first().expect("an entry").id;
    vault
        .set_field(
            id,
            fields::USERNAME,
            vault_core::NewValue::Open("written back".to_owned()),
        )
        .expect("the field is set");
    vault.save().expect("the vault saves");
    drop(vault);

    let again = vault_core::Vault::open(&database, key_file_alone(), LockPolicy::Respect)
        .expect("the file Coffer wrote opens with the same key file");
    let entry = again.entry(id).expect("the entry survives");
    assert_eq!(
        entry
            .field(fields::USERNAME)
            .and_then(|held| held.value.open()),
        Some("written back")
    );
}

/// The second attempt is a guess, and a guess must not become a way in. A key
/// file that is not this database's is refused as firmly with the fallback in
/// place as it was without one.
#[test]
fn the_wrong_key_file_is_still_refused_when_no_password_is_typed() {
    let (_scratch, database) = support::scratch("keyfile-only-kdbx41.kdbx");
    let wrong = password("")
        .with_key_file(&fixture("keyfile.key"))
        .expect("the key file reads");

    vault_core::Vault::open(&database, wrong, LockPolicy::Respect)
        .expect_err("another database's key file opens nothing");
}

/// The fallback only ever applies where there is a second composition to try.
/// A database that wants a password as well as its key file is not opened by
/// the file on its own.
#[test]
fn a_database_that_wants_both_is_not_opened_by_its_key_file_alone() {
    let (_scratch, database) = support::scratch("keyfile-kdbx41.kdbx");
    let half = password("")
        .with_key_file(&fixture("keyfile.key"))
        .expect("the key file reads");

    vault_core::Vault::open(&database, half, LockPolicy::Respect)
        .expect_err("the key file alone is not enough");
}

#[test]
fn a_key_file_database_needs_its_key_file() {
    let (_scratch, database) = support::scratch("keyfile-kdbx41.kdbx");

    let without =
        vault_core::Vault::open(&database, password("coffer-keyfile"), LockPolicy::Respect)
            .expect_err("the password alone is not enough");
    assert!(matches!(without, VaultError::WrongCredentials));

    let key = password("coffer-keyfile")
        .with_key_file(&fixture("keyfile.key"))
        .expect("the key file reads");
    vault_core::Vault::open(&database, key, LockPolicy::Respect).expect("both halves open it");
}

#[test]
fn a_kdbx3_database_with_attachments_is_read_only() {
    let (_scratch, database) = support::scratch("rich-kdbx31.kdbx");
    let mut vault = open(&database, SECRET);

    assert!(vault.is_read_only());
    assert!(matches!(
        vault.save().expect_err("saving is refused"),
        VaultError::ReadOnlyKdbx3Attachments
    ));
}

#[test]
fn a_kdbx3_database_with_an_empty_attachment_fails_to_open_rather_than_lose_it() {
    // keepass 0.13.22 cannot deserialise an empty Meta/Binaries entry, which is
    // where KDBX 3 keeps attachments. It is a clean refusal rather than a panic
    // or a silently dropped attachment, and this test pins it: the day the
    // library learns to read one, this fails and the limitation comes out of
    // the documentation.
    let (_scratch, database) = support::scratch("legacy-empty-attachment-kdbx31.kdbx");
    let error = vault_core::Vault::open(&database, password(SECRET), LockPolicy::Respect)
        .expect_err("the library cannot read it");

    assert!(matches!(error, VaultError::DamagedContent));
}

#[test]
fn an_empty_database_opens_with_nothing_in_it() {
    let (_scratch, database) = support::scratch("empty-kdbx31.kdbx");
    let vault = open(&database, SECRET);

    assert!(all_entries(&vault).is_empty());
    assert!(
        !vault.is_read_only(),
        "no attachments, so it can be upgraded"
    );
}
