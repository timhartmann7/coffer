//! Reading a database: what comes back, and what deliberately does not.

use vault_core::model::{FieldValue, fields};
use vault_core::{LockPolicy, VaultError};

use crate::support::{self, all_entries, entry_titled, fixture, open, password};

const RICH: &str = "rich-kdbx41.kdbx";
const SECRET: &str = "coffer-test";

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
        .filter(|entry| entry.title() == "versioned")
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
    assert_eq!(bin.entries[0].title(), "deleted entry");
}

#[test]
fn a_protected_field_arrives_without_its_value() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let entry = entry_titled(&vault, "basic");

    let password_field = entry.field(fields::PASSWORD).expect("a password field");
    assert_eq!(password_field.value, FieldValue::Protected { empty: false });
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
    assert_eq!(notes.value, FieldValue::Protected { empty: false });
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
fn empty_values_survive_as_empty_rather_than_absent() {
    let (_scratch, database) = support::scratch(RICH);
    let vault = open(&database, SECRET);
    let entry = entry_titled(&vault, "many custom fields");

    let open_field = entry
        .field("empty-unprotected")
        .expect("the field is there");
    assert_eq!(open_field.value, FieldValue::Open(String::new()));

    let protected = entry.field("empty-protected").expect("the field is there");
    assert_eq!(protected.value, FieldValue::Protected { empty: true });
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
