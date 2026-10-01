//! A field of the reader's own: hidden or shown, and called something else.
//!
//! Both move a value the window may never hold, so both happen inside the
//! vault, and both are edits like any other - a version behind them, nothing
//! written when nothing changed, and not one value lost on the way.

use keepass::db::fields;
use vault_core::model::{EntryId, FieldValue};
use vault_core::{NewValue, Typing, Vault, VaultError, Written};
use zeroize::Zeroizing;

use crate::support::{self, BUILT_PASSWORD, built, open};

/// A value that is hard to carry: markup the file escapes, text written right
/// to left, an override, and lines.
const AWKWARD: &str = "<a href=\"x\">&amp;</a> \u{202e}gnp.exe 'ok'\nsecond line\r\nthird";

/// A database holding one entry with one open and one protected field of the
/// reader's own, saved and opened again so that it is what a file gives.
fn with_fields(directory: &std::path::Path) -> (Vault, EntryId) {
    let path = built(directory, "fields.kdbx", |database| {
        database.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "card");
            entry.set_unprotected("Account number", "12345678");
            entry.set_protected("PIN", "4921");
            entry.set_unprotected("Awkward", AWKWARD);
        });
    });
    let vault = reopened(open(&path, BUILT_PASSWORD));
    let id = support::entry_titled(&vault, "card").id;
    (vault, id)
}

/// The value a field holds, read the way a reveal reads it, and whether the
/// file protects it. Nothing at all for a field the entry does not have.
fn held(vault: &Vault, id: EntryId, name: &str) -> Option<(String, bool)> {
    let entry = vault.entry(id)?;
    let field = entry.field(name)?;
    let protected = matches!(field.value, FieldValue::Protected { .. });
    let value = vault.reveal(id, name)?;
    Some((
        value.expose_str().expect("the value is text").to_owned(),
        protected,
    ))
}

/// The value KeePassXC's export gives a field, and whether it marks it to be
/// kept protected. Read off the text rather than parsed: the export is the
/// other implementation's own words, and this asks it two things.
fn exported_field(xml: &str, name: &str) -> Option<(String, bool)> {
    let key = format!("<Key>{name}</Key>");
    let after = xml.get(xml.find(&key)? + key.len()..)?;
    let opens = after.find("<Value")?;
    let tag = after.get(opens..opens + after.get(opens..)?.find('>')? + 1)?;
    let rest = after.get(opens + tag.len()..)?;
    let value = rest.get(..rest.find("</Value>")?)?;
    Some((value.to_owned(), tag.contains("ProtectInMemory=\"True\"")))
}

/// The vault saved, let go of, and opened again from the file: what the next
/// launch would find.
fn reopened(mut vault: Vault) -> Vault {
    vault.save().expect("the database saves");
    let path = vault.path().to_owned();
    drop(vault);
    open(&path, BUILT_PASSWORD)
}

#[test]
fn hiding_a_field_keeps_its_value_and_shows_it_again_unchanged() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (mut vault, id) = with_fields(scratch.path());

    for name in ["Account number", "Awkward"] {
        let before = held(&vault, id, name).expect("the field is there");
        assert!(!before.1, "{name} started out protected");

        vault
            .set_protection(id, name, true)
            .expect("the field is hidden");
        assert_eq!(held(&vault, id, name), Some((before.0.clone(), true)));

        vault = reopened(vault);
        assert_eq!(held(&vault, id, name), Some((before.0.clone(), true)));

        vault
            .set_protection(id, name, false)
            .expect("the field is shown");
        vault = reopened(vault);
        assert_eq!(held(&vault, id, name), Some((before.0, false)));
    }

    // Everything else on the entry is where it was.
    assert_eq!(
        held(&vault, id, "PIN"),
        Some(("4921".to_owned(), true)),
        "a field nobody touched changed"
    );
}

#[test]
fn hiding_a_field_is_an_edit_and_hiding_it_again_is_not() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (mut vault, id) = with_fields(scratch.path());
    let before = vault.entry(id).expect("the entry is there");

    vault
        .set_protection(id, "PIN", true)
        .expect("a hidden field may be hidden");
    let after = vault.entry(id).expect("the entry is there");
    assert_eq!(
        after.versions, before.versions,
        "nothing changed, and a version was kept"
    );
    assert_eq!(after.times.modified, before.times.modified);

    vault
        .set_protection(id, "PIN", false)
        .expect("the field is shown");
    let shown = vault.entry(id).expect("the entry is there");
    assert_eq!(shown.versions, before.versions + 1);

    // The version holds the field as it was: hidden. Putting it back hides it.
    let newest = vault
        .versions(id)
        .last()
        .map(|version| version.index)
        .expect("there is a version");
    vault
        .restore_version(id, newest)
        .expect("the version comes back");
    assert_eq!(held(&vault, id, "PIN"), Some(("4921".to_owned(), true)));
}

#[test]
fn the_fields_every_entry_has_keep_the_protection_the_database_gave_them() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (mut vault, id) = with_fields(scratch.path());
    vault
        .set_field(
            id,
            fields::PASSWORD,
            NewValue::Protected(Zeroizing::new("p".to_owned())),
        )
        .expect("the password is set");
    let before = vault.entry(id).expect("the entry is there");

    for name in [
        fields::TITLE,
        fields::USERNAME,
        fields::PASSWORD,
        fields::URL,
        fields::NOTES,
    ] {
        for protect in [true, false] {
            assert!(
                matches!(
                    vault.set_protection(id, name, protect),
                    Err(VaultError::StandardField)
                ),
                "{name} took a protection from the window"
            );
        }
    }

    // `title` is not `Title`: KeePass matches the five exactly, and so does
    // Coffer, so a field of the reader's own by that name is theirs to hide.
    vault
        .set_field(id, "title", NewValue::Open("mine".to_owned()))
        .expect("the field is made");
    vault
        .set_protection(id, "title", true)
        .expect("a field of the reader's own is hidden");

    let after = vault.entry(id).expect("the entry is there");
    for name in [fields::TITLE, fields::PASSWORD] {
        assert_eq!(after.field(name), before.field(name), "{name} changed");
    }
}

#[test]
fn a_field_that_is_not_there_is_not_hidden_or_renamed_into_being() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (mut vault, id) = with_fields(scratch.path());
    let before = vault.entry(id).expect("the entry is there");

    assert!(matches!(
        vault.set_protection(id, "Nowhere", true),
        Err(VaultError::NoSuchField)
    ));
    assert!(matches!(
        vault.rename_field(id, "Nowhere", "Somewhere"),
        Err(VaultError::NoSuchField)
    ));
    assert!(matches!(
        vault.set_protection(EntryId::new(), "PIN", true),
        Err(VaultError::NoSuchEntry)
    ));
    assert!(matches!(
        vault.rename_field(EntryId::new(), "PIN", "Card PIN"),
        Err(VaultError::NoSuchEntry)
    ));

    let after = vault.entry(id).expect("the entry is there");
    assert!(after.field("Somewhere").is_none() && after.field("Nowhere").is_none());
    assert_eq!(after.versions, before.versions);
}

/// The press that hides a field and the value written on the way out of it
/// travel side by side, and either can arrive first. The value is written in
/// the protection the field has when it arrives, never in the one the screen
/// read before the press.
#[test]
fn a_value_written_after_the_field_was_hidden_stays_hidden() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (mut vault, id) = with_fields(scratch.path());

    vault
        .set_protection(id, "Account number", true)
        .expect("the field is hidden");
    vault
        .set_field(id, "Account number", NewValue::Open("87654321".to_owned()))
        .expect("the value is written");
    assert_eq!(
        held(&vault, id, "Account number"),
        Some(("87654321".to_owned(), true)),
        "a late value took the field's protection off"
    );

    // And the other way round: a field shown stays shown.
    vault
        .set_protection(id, "PIN", false)
        .expect("the field is shown");
    vault
        .set_field(
            id,
            "PIN",
            NewValue::Protected(Zeroizing::new("1111".to_owned())),
        )
        .expect("the value is written");
    assert_eq!(held(&vault, id, "PIN"), Some(("1111".to_owned(), false)));

    // A field the entry does not have yet is made the way it was asked for.
    vault
        .set_field(
            id,
            "New",
            NewValue::Protected(Zeroizing::new(String::new())),
        )
        .expect("the field is made");
    vault
        .set_field(id, "Open", NewValue::Open(String::new()))
        .expect("the field is made");
    assert_eq!(held(&vault, id, "New"), Some((String::new(), true)));
    assert_eq!(held(&vault, id, "Open"), Some((String::new(), false)));
}

/// What a lock writes for a field is the reader's text in the field's own
/// protection. A draft that only disagreed about protection is nothing new.
#[test]
fn typing_saved_by_a_lock_keeps_the_fields_protection() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (mut vault, id) = with_fields(scratch.path());
    let before = vault.entry(id).expect("the entry is there");

    let written = vault
        .set_typed(
            id,
            "PIN",
            NewValue::Open("4921".to_owned()),
            Typing::InPlace,
        )
        .expect("the draft is taken");
    assert_eq!(written, Written::Nothing);
    assert_eq!(
        vault.entry(id).expect("the entry").versions,
        before.versions
    );

    let written = vault
        .set_typed(
            id,
            "PIN",
            NewValue::Open("0000".to_owned()),
            Typing::InPlace,
        )
        .expect("the draft is taken");
    assert_eq!(written, Written::Into);
    assert_eq!(held(&vault, id, "PIN"), Some(("0000".to_owned(), true)));
}

#[test]
fn renaming_a_field_takes_its_value_and_its_protection_with_it() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (mut vault, id) = with_fields(scratch.path());

    for (from, to) in [
        ("PIN", "Card PIN"),
        ("Account number", "Account no."),
        ("Awkward", "\u{202e}<Awk & ward>"),
    ] {
        let before = held(&vault, id, from).expect("the field is there");
        let versions = vault.entry(id).expect("the entry").versions;

        vault
            .rename_field(id, from, to)
            .expect("the field is renamed");
        assert_eq!(held(&vault, id, from), None, "{from} is still there");
        assert_eq!(held(&vault, id, to), Some(before.clone()));
        assert_eq!(
            vault.entry(id).expect("the entry").versions,
            versions + 1,
            "a rename kept no version, or more than one"
        );

        vault = reopened(vault);
        assert_eq!(held(&vault, id, to), Some(before));
    }
}

#[test]
fn putting_back_the_version_a_rename_kept_puts_back_the_old_name() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (mut vault, id) = with_fields(scratch.path());

    vault
        .rename_field(id, "PIN", "Card PIN")
        .expect("the field is renamed");
    let newest = vault
        .versions(id)
        .last()
        .map(|version| version.index)
        .expect("the rename kept a version");
    vault
        .restore_version(id, newest)
        .expect("the version comes back");

    assert_eq!(held(&vault, id, "PIN"), Some(("4921".to_owned(), true)));
    assert_eq!(held(&vault, id, "Card PIN"), None);
}

/// Renaming onto a name in use would put one value over another, and renaming
/// onto one of the five would make a field of the reader's own the password.
#[test]
fn a_rename_that_would_write_over_something_is_refused_and_changes_nothing() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (mut vault, id) = with_fields(scratch.path());
    let before = vault.entry(id).expect("the entry is there");

    for (from, to, expected) in [
        ("PIN", "Account number", "taken"),
        ("PIN", fields::PASSWORD, "taken"),
        ("PIN", fields::TITLE, "taken"),
        // The entry was built without an address, and `URL` is still the
        // address on every entry: a field renamed to it would be one.
        ("PIN", fields::URL, "taken"),
        ("PIN", fields::NOTES, "taken"),
        ("PIN", "", "unnamed"),
        ("PIN", "a\u{0}b", "unwritable"),
        ("PIN", "bell\u{7}", "unwritable"),
        (fields::TITLE, "Name", "standard"),
        (fields::PASSWORD, "Not the password", "standard"),
    ] {
        let refused = vault.rename_field(id, from, to);
        let matched = match expected {
            "taken" => matches!(refused, Err(VaultError::FieldNameTaken)),
            "unnamed" => matches!(refused, Err(VaultError::UnnamedField)),
            "unwritable" => matches!(refused, Err(VaultError::UnwritableText)),
            _ => matches!(refused, Err(VaultError::StandardField)),
        };
        assert!(matched, "{from:?} to {to:?} was not refused as {expected}");
    }

    let after = vault.entry(id).expect("the entry is there");
    assert_eq!(
        after.fields, before.fields,
        "a refused rename changed a field"
    );
    assert_eq!(after.versions, before.versions);
    assert_eq!(held(&vault, id, "PIN"), Some(("4921".to_owned(), true)));
}

#[test]
fn a_rename_to_the_same_name_writes_nothing_and_one_in_another_case_is_a_rename() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (mut vault, id) = with_fields(scratch.path());
    let before = vault.entry(id).expect("the entry is there");

    vault
        .rename_field(id, "PIN", "PIN")
        .expect("the same name is no change");
    let after = vault.entry(id).expect("the entry is there");
    assert_eq!(after.versions, before.versions);
    assert_eq!(after.times.modified, before.times.modified);

    // KeePass matches names exactly, so `pin` is a name of its own.
    vault
        .rename_field(id, "PIN", "pin")
        .expect("the case changes");
    assert_eq!(held(&vault, id, "pin"), Some(("4921".to_owned(), true)));
    assert_eq!(held(&vault, id, "PIN"), None);
}

/// The content attack: an entry with two hundred fields of the reader's own,
/// every one renamed and hidden. Not one value may go missing or land in
/// another field.
#[test]
fn two_hundred_fields_renamed_and_hidden_lose_nothing() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "many.kdbx", |database| {
        database.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "many");
            for index in 0..200 {
                entry.set_unprotected(format!("Code {index}"), format!("value {index}"));
            }
        });
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let id = support::entry_titled(&vault, "many").id;

    for index in 0..200 {
        let from = format!("Code {index}");
        let to = format!("Recovery code {index}");
        vault
            .rename_field(id, &from, &to)
            .expect("the field is renamed");
        vault
            .set_protection(id, &to, true)
            .expect("the field is hidden");
    }

    let vault = reopened(vault);
    for index in 0..200 {
        assert_eq!(
            held(&vault, id, &format!("Recovery code {index}")),
            Some((format!("value {index}"), true)),
            "field {index} did not come through"
        );
        assert_eq!(held(&vault, id, &format!("Code {index}")), None);
    }
}

/// KeePassXC reads what Coffer wrote: the new name, the value under it, and
/// the protection, which it shows as the field being hidden.
#[test]
fn keepassxc_reads_a_renamed_and_hidden_field_the_way_coffer_left_it() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (mut vault, id) = with_fields(scratch.path());

    vault
        .rename_field(id, "Account number", "Sort code")
        .expect("the field is renamed");
    vault
        .set_protection(id, "Sort code", true)
        .expect("the field is hidden");
    vault
        .set_protection(id, "PIN", false)
        .expect("the field is shown");
    vault.save().expect("the database saves");

    let exported = support::export(&tool, vault.path(), BUILT_PASSWORD, None);
    // The entry as it is now. Its versions follow it in the export, and the
    // one the rename kept still holds the old name, as it should.
    let exported = exported
        .split("<History>")
        .next()
        .expect("there is text before any history");
    assert_eq!(
        exported_field(exported, "Sort code"),
        Some(("12345678".to_owned(), true)),
        "KeePassXC does not read the renamed field as hidden"
    );
    assert_eq!(exported_field(exported, "Account number"), None);
    assert_eq!(
        exported_field(exported, "PIN"),
        Some(("4921".to_owned(), false)),
        "KeePassXC still reads the field that was shown as hidden"
    );
}
