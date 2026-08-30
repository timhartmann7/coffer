//! The round-trip suite: the test this project exists to pass.
//!
//! For every fixture: export it with keepassxc-cli, open it with Coffer, save
//! it, export it again, and compare. Any surviving difference is a field Coffer
//! lost or altered. Attachment bytes do not appear in a KDBX 4 export at all, so
//! they are pulled out separately and compared byte for byte.
//!
//! Failures print the differing XML. The suite only ever runs against the
//! fixtures in this repository, whose contents are committed beside it as plain
//! XML, so there is nothing in that output that is not already in the tree.

use std::path::{Path, PathBuf};

use vault_core::{LockPolicy, Vault};

use crate::normalise::{canonical, differences};
use crate::support;

struct Fixture {
    file: &'static str,
    password: &'static str,
    key_file: Option<&'static str>,
    /// True when Coffer reads this fixture in one KDBX version and writes
    /// another, which makes a handful of format-only elements appear or
    /// disappear.
    cross_version: bool,
    /// What keepassxc-cli finds in this fixture. Both sides of the attachment
    /// comparison are read through the same parser, so a parser that answered
    /// nothing would compare nothing and pass. These are what say it looked.
    entries: usize,
    attachments: usize,
}

#[test]
fn kdbx41_round_trips() {
    round_trip(&Fixture {
        file: "rich-kdbx41.kdbx",
        password: "coffer-test",
        key_file: None,
        cross_version: false,
        entries: 10,
        attachments: 6,
    });
}

#[test]
fn kdbx40_round_trips() {
    round_trip(&Fixture {
        file: "rich-kdbx40.kdbx",
        password: "coffer-test",
        key_file: None,
        cross_version: false,
        entries: 10,
        attachments: 6,
    });
}

#[test]
fn minimal_round_trips() {
    round_trip(&Fixture {
        file: "minimal-kdbx41.kdbx",
        password: "coffer-test",
        key_file: None,
        cross_version: false,
        entries: 1,
        attachments: 0,
    });
}

#[test]
fn key_file_database_round_trips() {
    round_trip(&Fixture {
        file: "keyfile-kdbx41.kdbx",
        password: "coffer-keyfile",
        key_file: Some("keyfile.key"),
        cross_version: false,
        entries: 1,
        attachments: 0,
    });
}

#[test]
fn kdbx31_round_trips_as_kdbx41() {
    round_trip(&Fixture {
        file: "empty-kdbx31.kdbx",
        password: "coffer-test",
        key_file: None,
        cross_version: true,
        entries: 0,
        attachments: 0,
    });
}

fn round_trip(fixture: &Fixture) {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };

    let directory = tempfile::tempdir().expect("a scratch directory");
    let original = directory.path().join("original.kdbx");
    let saved = directory.path().join("saved.kdbx");
    std::fs::copy(support::fixture(fixture.file), &original).expect("the fixture copies");
    std::fs::copy(support::fixture(fixture.file), &saved).expect("the fixture copies");

    let key_file: Option<PathBuf> = fixture.key_file.map(support::fixture);

    let before = support::export(&tool, &original, fixture.password, key_file.as_deref());

    let mut vault = open(&saved, fixture, key_file.as_deref());
    vault.save().expect("the database saves");
    drop(vault);

    let after = support::export(&tool, &saved, fixture.password, key_file.as_deref());

    let before = canonical(&before, fixture.cross_version);
    let after = canonical(&after, fixture.cross_version);
    let differences = differences(&before, &after);

    assert!(
        differences.is_empty(),
        "{} lost or altered a field on the way through Coffer:\n  {}",
        fixture.file,
        differences.join("\n  ")
    );

    assert_attachments_survive(
        &tool,
        fixture,
        &original,
        &saved,
        directory.path(),
        key_file.as_deref(),
    );
}

fn open(path: &Path, fixture: &Fixture, key_file: Option<&Path>) -> Vault {
    let mut key = support::password(fixture.password);
    if let Some(key_file) = key_file {
        key = key.with_key_file(key_file).expect("the key file reads");
    }
    Vault::open(path, key, LockPolicy::Respect).expect("the database opens")
}

/// A KDBX 4 export names attachments and never carries their bytes, so a
/// corrupted attachment would slip through the XML comparison untouched.
fn assert_attachments_survive(
    tool: &Path,
    fixture: &Fixture,
    original: &Path,
    saved: &Path,
    scratch: &Path,
    key_file: Option<&Path>,
) {
    let secret = fixture.password;
    let entries = support::entry_paths(tool, original, secret, key_file);

    assert_eq!(
        entries.len(),
        fixture.entries,
        "{} does not hold the entries this comparison is built on",
        fixture.file
    );

    assert_eq!(
        entries,
        support::entry_paths(tool, saved, secret, key_file),
        "{} changed which entries exist",
        fixture.file
    );

    let mut compared = 0;

    for entry in &entries {
        let names = support::attachment_names(tool, original, secret, key_file, entry);
        compared += names.len();
        assert_eq!(
            names,
            support::attachment_names(tool, saved, secret, key_file, entry),
            "{}: entry {entry} changed which attachments it has",
            fixture.file
        );

        for name in &names {
            let before = support::attachment_bytes(
                tool,
                original,
                secret,
                key_file,
                entry,
                name,
                &scratch.join("before.bin"),
            );
            let after = support::attachment_bytes(
                tool,
                saved,
                secret,
                key_file,
                entry,
                name,
                &scratch.join("after.bin"),
            );

            assert_eq!(
                before.len(),
                after.len(),
                "{}: attachment {name} on {entry} changed size",
                fixture.file
            );
            assert!(
                before == after,
                "{}: attachment {name} on {entry} changed contents",
                fixture.file
            );
        }
    }

    assert_eq!(
        compared, fixture.attachments,
        "{} does not hold the attachments this comparison is built on",
        fixture.file
    );
}

#[test]
fn the_failure_report_names_elements_and_never_quotes_a_value() {
    let before = canonical(
        r#"<KeePassFile><Root><Entry><String><Key>Password</Key>
           <Value ProtectInMemory="True">SECRETCANARY</Value></String></Entry></Root></KeePassFile>"#,
        false,
    );
    let after = canonical(
        r#"<KeePassFile><Root><Entry><String><Key>Password</Key>
           <Value ProtectInMemory="True">something else entirely</Value></String></Entry></Root></KeePassFile>"#,
        false,
    );

    let report = differences(&before, &after).join("\n");

    assert!(!report.is_empty(), "a changed value went unreported");
    assert!(
        report.contains("Value"),
        "the report should name the element that changed: {report}"
    );
    assert!(
        !report.contains("SECRETCANARY") && !report.contains("something else"),
        "the report quoted a value: {report}"
    );
}

#[test]
fn the_normaliser_sorts_the_collections_the_format_keeps_as_maps() {
    let one = canonical(
        r#"<KeePassFile><Meta><CustomData>
             <Item><Key>b</Key><Value>2</Value></Item>
             <Item><Key>a</Key><Value>1</Value></Item>
           </CustomData></Meta></KeePassFile>"#,
        false,
    );
    let other = canonical(
        r#"<KeePassFile><Meta><CustomData>
             <Item><Key>a</Key><Value>1</Value></Item>
             <Item><Key>b</Key><Value>2</Value></Item>
           </CustomData></Meta></KeePassFile>"#,
        false,
    );

    assert!(differences(&one, &other).is_empty());
}

#[test]
fn the_normaliser_reads_both_timestamp_spellings_as_the_same_moment() {
    // KDBX 3 exports ISO 8601, KDBX 4 exports base64 seconds since year one.
    let iso = canonical(
        "<KeePassFile><Times><CreationTime>2020-01-01T00:00:00Z</CreationTime></Times></KeePassFile>",
        false,
    );
    let packed = canonical(
        "<KeePassFile><Times><CreationTime>ANid1Q4AAAA=</CreationTime></Times></KeePassFile>",
        false,
    );

    assert!(
        differences(&iso, &packed).is_empty(),
        "the two spellings of 2020-01-01 did not compare equal: {iso:?} against {packed:?}"
    );
}

#[test]
fn keepassxc_writing_the_file_while_coffer_holds_it_open_is_noticed() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };

    let (_scratch, database) = support::scratch("minimal-kdbx41.kdbx");
    let mut vault = support::open(&database, "coffer-test");

    // The other client Coffer is dogfooded against, writing the same file.
    let path = database.to_string_lossy().into_owned();
    support::cli(
        &tool,
        "coffer-test",
        None,
        &[
            "edit",
            "-q",
            "--url",
            "https://written-by-keepassxc",
            &path,
            "only entry",
        ],
    )
    .expect("keepassxc-cli edits the database");

    assert!(
        matches!(
            vault.save().expect_err("the save is refused"),
            vault_core::VaultError::ExternalChange
        ),
        "KeePassXC wrote the file and Coffer did not notice"
    );

    // Reopening picks up their work rather than losing it.
    drop(vault);
    let reopened = support::open(&database, "coffer-test");
    assert_eq!(
        support::entry_titled(&reopened, "only entry").url(),
        "https://written-by-keepassxc"
    );
}

#[test]
fn keepassxc_reads_back_a_database_coffer_wrote_over_theirs() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };

    let (_scratch, database) = support::scratch("rich-kdbx41.kdbx");

    // Coffer writes first, KeePassXC reads second: the other order.
    let mut vault = support::open(&database, "coffer-test");
    let id = support::entry_titled(&vault, "basic").id;
    vault
        .set_field(
            id,
            vault_core::model::fields::URL,
            vault_core::NewValue::Open("https://written-by-coffer".to_owned()),
        )
        .expect("the field is written");
    vault.save().expect("the database saves");
    drop(vault);

    let shown = support::cli(
        &tool,
        "coffer-test",
        None,
        &[
            "show",
            "-q",
            "-a",
            "URL",
            &database.to_string_lossy(),
            "basic",
        ],
    )
    .expect("keepassxc-cli reads what Coffer wrote");

    assert_eq!(
        String::from_utf8_lossy(&shown).trim(),
        "https://written-by-coffer"
    );
}
