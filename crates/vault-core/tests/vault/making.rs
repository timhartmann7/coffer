//! Making an entry: of a kind, and from one of the templates a vault keeps.

use std::path::{Path, PathBuf};

use keepass::Database;
use keepass::db::{Icon, MemoryProtection, Value, fields};
use vault_core::kind::Kind;
use vault_core::model::fields::Standard;
use vault_core::model::{Deletion, EntryId, FieldValue, GroupId, Project};
use vault_core::{NewValue, Vault, VaultError};

use crate::bin::bin_of;
use crate::support::{
    self, BUILT_PASSWORD, RICH, SECRET, built, cheap, entry_titled, exported_entry, exported_field,
    files, folder, library, only_group, open, reopened,
};

/// Every kind, with the tag it is made with and the icon KeePass draws for it.
/// Written out rather than read from the kind, so that a number that moves is a
/// test that fails: an icon is the one thing here a reader of the file sees and
/// Coffer never draws.
const MADE_AS: [(Kind, Option<&str>, usize); 8] = [
    (Kind::Login, None, 0),
    (Kind::BankCard, Some("card"), 66),
    (Kind::Wifi, Some("wifi"), 12),
    (Kind::Identity, Some("id"), 9),
    (Kind::Licence, Some("licence"), 47),
    (Kind::RecoveryCodes, Some("recovery"), 52),
    (Kind::SecureNote, Some("note"), 44),
    (Kind::SshKey, Some("ssh"), 29),
];

/// The fields each kind hides, written out rather than read from the kind. A
/// hidden value waits in Rust until the reader asks to see it; an open one goes
/// to the window with every read of the entry and is drawn in the clear. So a
/// secret that moved from one list to the other is a test that fails here, not
/// every CVV typed from then on sent to the window.
const HIDDEN: [(Kind, &[&str]); 8] = [
    (Kind::Login, &[]),
    (Kind::BankCard, &["CVV", "Number", "PIN"]),
    (Kind::Wifi, &[]),
    (Kind::Identity, &["Number"]),
    (Kind::Licence, &["Licence key"]),
    (Kind::RecoveryCodes, &["Recovery codes"]),
    (Kind::SecureNote, &["Secret note"]),
    (Kind::SshKey, &[]),
];

/// Whether a field is there, and how it is kept.
fn kept(vault: &Vault, id: EntryId, name: &str) -> Option<(bool, bool)> {
    let entry = vault.entry(id)?;
    let field = entry.field(name)?;
    Some((
        matches!(field.value, FieldValue::Protected { .. }),
        field.is_empty(),
    ))
}

/// Every kind writes the five fields every entry has, under the protection
/// the database asks for, then exactly the fields it names, each empty and
/// hidden as it says - and its tag and its icon, which are in the file once it
/// is written.
#[test]
fn every_kind_makes_the_fields_it_names_with_the_protection_it_names() {
    let (_scratch, path) = cheap("kinds.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;

    let mut made = Vec::new();
    for (kind, tag, _) in MADE_AS {
        let id = vault.create_entry(root, kind).expect("the entry is made");
        let entry = vault.entry(id).expect("the entry is there");

        let mut names: Vec<&str> = entry
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        let mut wanted = vec![
            fields::TITLE,
            fields::USERNAME,
            fields::PASSWORD,
            fields::URL,
            fields::NOTES,
        ];
        wanted.extend(kind.slots().iter().map(|slot| slot.name));
        names.sort_unstable();
        wanted.sort_unstable();
        assert_eq!(names, wanted, "{kind:?}");

        for standard in [fields::TITLE, fields::USERNAME, fields::URL, fields::NOTES] {
            assert_eq!(kept(&vault, id, standard), Some((false, true)), "{kind:?}");
        }
        assert_eq!(kept(&vault, id, fields::PASSWORD), Some((true, true)));
        for slot in kind.slots() {
            assert_eq!(
                kept(&vault, id, slot.name),
                Some((slot.protect, true)),
                "{kind:?} made {} the wrong way",
                slot.name
            );
        }
        assert_eq!(
            entry.tags,
            tag.map(str::to_owned).into_iter().collect::<Vec<_>>()
        );
        assert_eq!(entry.versions, 0, "a new entry has a past");
        made.push(id);
    }
    vault.save().expect("the database saves");
    drop(vault);

    let file = library(&path, BUILT_PASSWORD);
    for ((kind, _, icon), id) in MADE_AS.iter().zip(made) {
        let entry = file.entry(id).expect("the entry is in the file");
        assert_eq!(entry.icon(), Some(&Icon::BuiltIn(*icon)), "{kind:?}");
    }
}

/// Exactly the fields a kind names as secrets are hidden, in the file Coffer
/// writes and in the one it reads back, and every other field of the kind's
/// own is open.
#[test]
fn every_secret_a_kind_names_is_made_hidden_and_nothing_else_is() {
    let (_scratch, path) = cheap("hidden.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let made: Vec<_> = HIDDEN
        .iter()
        .map(|(kind, hidden)| {
            let id = vault.create_entry(root, *kind).expect("the entry is made");
            (*kind, *hidden, id)
        })
        .collect();

    let reread = reopened(vault, BUILT_PASSWORD);
    for (kind, hidden, id) in made {
        let entry = reread.entry(id).expect("the entry is there");
        let own: Vec<_> = entry
            .fields
            .iter()
            .filter(|field| Standard::of(&field.name).is_none())
            .collect();
        let mut protected: Vec<&str> = own
            .iter()
            .filter(|field| matches!(field.value, FieldValue::Protected { .. }))
            .map(|field| field.name.as_str())
            .collect();
        protected.sort_unstable();
        assert_eq!(protected, hidden, "{kind:?} hides the wrong fields");
        assert_eq!(
            own.len(),
            kind.slots().len(),
            "{kind:?} made a field it does not name"
        );
        for field in own {
            if !hidden.contains(&field.name.as_str()) {
                assert!(
                    matches!(field.value, FieldValue::Open(_)),
                    "{kind:?} keeps {} neither open nor hidden",
                    field.name
                );
            }
        }
    }
}

/// A database that asks for its titles and logins to be protected gets them
/// protected on an entry of every kind, as on a login: the kind adds fields
/// and leaves the five to the database.
#[test]
fn a_kind_keeps_the_databases_protection_for_the_standard_fields() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "protecting.kdbx", |database| {
        database.meta.memory_protection = Some(MemoryProtection {
            protect_title: true,
            protect_username: true,
            protect_password: true,
            protect_url: false,
            protect_notes: true,
        });
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;

    for kind in Kind::ALL {
        let id = vault.create_entry(root, kind).expect("the entry is made");
        for (name, protected) in [
            (fields::TITLE, true),
            (fields::USERNAME, true),
            (fields::PASSWORD, true),
            (fields::URL, false),
            (fields::NOTES, true),
        ] {
            assert_eq!(
                kept(&vault, id, name),
                Some((protected, true)),
                "{kind:?} {name}"
            );
        }
    }
}

/// What is in the recycle bin is deleted, and an entry made there would be a
/// deleted entry nobody deleted. Nothing is made there, in a folder inside it,
/// in a folder that is not there, or in a vault Coffer does not write, and
/// nothing about the vault changes on the way.
#[test]
fn nothing_is_made_in_the_recycle_bin_or_a_folder_that_is_not_there() {
    let (_scratch, path) = support::scratch(RICH);
    let mut vault = open(&path, SECRET);
    let root = vault.tree().id;
    let doomed = vault
        .create_group(root, "Doomed")
        .expect("the folder is made");
    vault
        .delete_group(doomed, Deletion::Bin)
        .expect("it goes to the bin");
    let gone = vault
        .create_group(root, "Gone")
        .expect("the folder is made");
    vault
        .delete_group(gone, Deletion::Bin)
        .expect("it goes to the bin");
    vault
        .delete_group(gone, Deletion::Forever)
        .expect("and out of the file");
    let bin = bin_of(&vault.tree()).id;

    let tree = vault.tree();
    let (count, edits) = (vault.count(), vault.edits());
    for kind in Kind::ALL {
        assert!(matches!(
            vault.create_entry(bin, kind),
            Err(VaultError::IntoRecycleBin)
        ));
        assert!(matches!(
            vault.create_entry(doomed, kind),
            Err(VaultError::IntoRecycleBin)
        ));
        assert!(matches!(
            vault.create_entry(gone, kind),
            Err(VaultError::NoSuchGroup)
        ));
    }
    assert_eq!(vault.tree(), tree);
    assert_eq!((vault.count(), vault.edits()), (count, edits));

    vault.save().expect("the database saves");
    drop(vault);
    let snapshot = vault_core::storage::snapshot::slot(&path, 1).expect("a slot has a name");
    let mut read_only = open(&snapshot, SECRET);
    let top = read_only.tree().id;
    assert!(matches!(
        read_only.create_entry(top, Kind::BankCard),
        Err(VaultError::ReadOnlySnapshot)
    ));
}

/// A snapshot keeps the templates group its vault had, and its templates are
/// templates still - but Coffer does not write a snapshot, so nothing is made
/// from one there, and the snapshot is left as it was opened.
#[test]
fn nothing_is_made_from_a_template_in_a_vault_coffer_does_not_write() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = templated(scratch.path(), |database| {
        Some(named(database, "Templates"))
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    vault
        .create_entry(root, Kind::Login)
        .expect("the entry is made");
    vault.save().expect("the database saves");
    drop(vault);

    let snapshot = vault_core::storage::snapshot::slot(&path, 1).expect("a slot has a name");
    let mut read_only = open(&snapshot, BUILT_PASSWORD);
    let template = entry_titled(&read_only, "Card template").id;
    let personal = only_group(&read_only, "Personal");
    let tree = read_only.tree();
    assert_eq!(
        marked(&tree).len(),
        1,
        "the snapshot keeps no templates group"
    );
    let (count, edits) = (read_only.count(), read_only.edits());
    for into in [tree.id, personal] {
        assert!(matches!(
            read_only.create_from_template(template, into),
            Err(VaultError::ReadOnlySnapshot)
        ));
    }
    assert_eq!(read_only.tree(), tree);
    assert_eq!((read_only.count(), read_only.edits()), (count, edits));
}

/// KeePassXC reads an entry of every kind as an ordinary one: the icon, the
/// tag, and each field the kind hides as hidden, the way a field a reader hid
/// in KeePassXC is.
#[test]
fn a_kinds_entry_reads_in_keepassxc_with_its_icon_tag_and_hidden_fields() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let (_scratch, path) = cheap("kinds-in-keepassxc.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    for (kind, _, _) in MADE_AS {
        let id = vault.create_entry(root, kind).expect("the entry is made");
        vault
            .set_field(id, fields::TITLE, NewValue::Open(kind.name().to_owned()))
            .expect("the title is written");
    }
    vault.save().expect("the database saves");
    drop(vault);

    let exported = support::export(&tool, &path, BUILT_PASSWORD, None);
    for (kind, tag, icon) in MADE_AS {
        let entry = exported_entry(&exported, kind.name())
            .unwrap_or_else(|| panic!("KeePassXC does not list {}", kind.name()));
        assert!(
            entry.contains(&format!("<IconID>{icon}</IconID>")),
            "KeePassXC does not draw {kind:?} with icon {icon}"
        );
        match tag {
            Some(tag) => assert!(entry.contains(&format!("<Tags>{tag}</Tags>")), "{kind:?}"),
            None => assert!(!entry.contains("<Tags>"), "{kind:?}"),
        }
        let hidden = HIDDEN
            .iter()
            .find(|(each, _)| *each == kind)
            .map(|(_, hidden)| *hidden)
            .unwrap_or_default();
        for slot in kind.slots() {
            assert_eq!(
                exported_field(entry, slot.name),
                Some((String::new(), hidden.contains(&slot.name))),
                "KeePassXC reads {} of {kind:?} the wrong way",
                slot.name
            );
        }
    }
}

/// A vault with a "Templates" folder holding a card template, which carries
/// a file and a folder of its own, and a "Personal" folder with one entry.
/// `names` says which group `Meta/EntryTemplatesGroup` names, if any.
fn templated(directory: &Path, names: impl FnOnce(&Database) -> Option<GroupId>) -> PathBuf {
    built(directory, "templated.kdbx", |database| {
        let templates = {
            let mut root = database.root_mut();
            let mut templates = root.add_group();
            templates.name = "Templates".to_owned();
            let template = templates.add_entry().id();
            let mut inner = templates.add_group();
            inner.name = "More templates".to_owned();
            inner
                .add_entry()
                .edit(|entry| entry.set_unprotected(fields::TITLE, "Nested template"));
            let mut personal = root.add_group();
            personal.name = "Personal".to_owned();
            personal
                .add_entry()
                .edit(|entry| entry.set_unprotected(fields::TITLE, "Not a template"));
            template
        };
        let mut template = database
            .entry_mut(templates)
            .expect("the template is there");
        template.set_unprotected(fields::TITLE, "Card template");
        template.set_unprotected("Cardholder", "");
        template.set_protected("CVV", "");
        template.set(
            fields::PASSWORD,
            Value::protected("template secret".to_owned()),
        );
        support::furnish(&mut template, "template");
        template.add_attachment("card.png", Value::protected(vec![0x89, 0x50, 0x4e, 0x47]));
        // A version, which an entry made from the template does not carry.
        template.edit_tracking(|template| {
            template.set_unprotected(fields::NOTES, "The bank's number is on the back");
        });

        let named = names(database).map(|group| group.uuid());
        database.meta.entry_templates_group = named;
        database.meta.entry_templates_group_changed =
            chrono::NaiveDate::from_ymd_opt(2025, 6, 7).and_then(|day| day.and_hms_opt(8, 9, 10));
    })
}

/// The group the vault's own tree calls the templates group, if any.
fn marked(tree: &Project) -> Vec<GroupId> {
    let mut found = Vec::new();
    if tree.is_templates {
        found.push(tree.id);
    }
    for section in &tree.sections {
        found.extend(marked(section));
    }
    found
}

fn named(database: &Database, name: &str) -> GroupId {
    database
        .iter_all_groups()
        .find(|group| group.name == name)
        .map(|group| group.id())
        .expect("the folder is there")
}

/// The templates group is the one `Meta/EntryTemplatesGroup` names, while it
/// is there, is not the top of the vault, and is not in the bin. A nil UUID
/// names none, as KeePass writes it. Put back out of the bin, it is the
/// templates group again.
#[test]
fn the_templates_group_is_the_one_meta_names_and_nothing_else() {
    let scratch = tempfile::tempdir().expect("a scratch directory");

    let none = templated(scratch.path(), |_| None);
    assert!(marked(&open(&none, BUILT_PASSWORD).tree()).is_empty());
    std::fs::remove_file(&none).expect("the file goes");

    let (_rich, rich) = support::scratch(RICH);
    assert!(
        marked(&open(&rich, SECRET).tree()).is_empty(),
        "a nil UUID named a group"
    );

    let elsewhere = Database::new().root().id();
    let missing = templated(scratch.path(), |_| Some(elsewhere));
    assert!(marked(&open(&missing, BUILT_PASSWORD).tree()).is_empty());
    std::fs::remove_file(&missing).expect("the file goes");

    let top = templated(scratch.path(), |database| Some(database.root().id()));
    assert!(
        marked(&open(&top, BUILT_PASSWORD).tree()).is_empty(),
        "the top of the vault was taken for a templates group"
    );
    std::fs::remove_file(&top).expect("the file goes");

    let path = templated(scratch.path(), |database| {
        Some(named(database, "Templates"))
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let templates = only_group(&vault, "Templates");
    assert_eq!(marked(&vault.tree()), vec![templates]);

    vault
        .delete_group(templates, Deletion::Bin)
        .expect("it goes to the bin");
    assert!(
        marked(&vault.tree()).is_empty(),
        "a templates group in the bin is still offered"
    );
    vault.put_back_group(templates).expect("it is put back");
    assert_eq!(marked(&vault.tree()), vec![templates]);

    // The bin itself, named as the templates group by a file.
    let root = vault.tree().id;
    let throwaway = vault
        .create_group(root, "Throwaway")
        .expect("the folder is made");
    vault
        .delete_group(throwaway, Deletion::Bin)
        .expect("it goes to the bin");
    vault.save().expect("the database saves");
    drop(vault);
    let mut file = library(&path, BUILT_PASSWORD);
    file.meta.entry_templates_group = file.recycle_bin().map(|bin| bin.id().uuid());
    let rewritten = scratch.path().join("bin-named.kdbx");
    let mut out = std::fs::File::create(&rewritten).expect("the file is made");
    file.save(
        &mut out,
        keepass::DatabaseKey::new().with_password(BUILT_PASSWORD),
    )
    .expect("the file is written");
    assert!(
        marked(&open(&rewritten, BUILT_PASSWORD).tree()).is_empty(),
        "the recycle bin was taken for a templates group"
    );
}

/// The nil UUID is how a file says it names no templates group, and a file can
/// also give a group that UUID. The name still names none: that group is an
/// ordinary folder, and nothing is made from what it holds.
#[test]
fn a_group_with_a_nil_uuid_is_never_the_templates_group() {
    // `Uuid`'s default is the nil UUID.
    let nil = GroupId::from_uuid(Default::default());
    assert!(nil.uuid().is_nil());
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "nil.kdbx", |database| {
        let mut root = database.root_mut();
        let mut group = root
            .add_group_with_id(nil)
            .expect("no group has the nil UUID yet");
        group.name = "Nil".to_owned();
        group
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "Held by nil"));
        database.meta.entry_templates_group = Some(nil.uuid());
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    assert_eq!(only_group(&vault, "Nil"), nil, "the file lost the nil UUID");
    assert!(marked(&vault.tree()).is_empty(), "a nil UUID named a group");
    let held = entry_titled(&vault, "Held by nil").id;
    let root = vault.tree().id;
    let (count, edits) = (vault.count(), vault.edits());
    assert!(matches!(
        vault.create_from_template(held, root),
        Err(VaultError::NotATemplate)
    ));
    assert_eq!((vault.count(), vault.edits()), (count, edits));
}

/// An entry made from a template is a copy of it in the folder it was asked
/// for: every field with its protection, everything else it holds, and a file
/// of its own with the template's bytes, under the template's own title. The
/// template is not touched: no version, no new date, nothing moved.
#[test]
fn an_entry_from_a_template_is_a_copy_made_where_it_was_asked() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = templated(scratch.path(), |database| {
        Some(named(database, "Templates"))
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let personal = only_group(&vault, "Personal");
    let template = entry_titled(&vault, "Card template");
    assert!(
        template.versions > 0,
        "the template has no version to leave behind"
    );

    let made = vault
        .create_from_template(template.id, personal)
        .expect("the entry is made");
    assert_ne!(made, template.id);
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&path, BUILT_PASSWORD);
    let entry = vault.entry(made).expect("the entry is there");
    assert_eq!(entry.group, personal);
    assert_eq!(entry.versions, 0);
    assert_eq!(
        vault.entry(template.id),
        Some(template.clone()),
        "the template changed"
    );
    let copied: Vec<_> = files(&vault)
        .into_iter()
        .filter(|(title, name, _)| title == "Card template" && name == "card.png")
        .collect();
    assert_eq!(
        copied.len(),
        2,
        "the entry has no file of its own: {copied:?}"
    );
    assert!(
        copied
            .iter()
            .all(|(_, _, bytes)| bytes == &[0x89, 0x50, 0x4e, 0x47])
    );

    let file = library(&path, BUILT_PASSWORD);
    let original = support::holding(&file.entry(template.id).expect("the template is there"));
    let copy = support::holding(&file.entry(made).expect("the entry is there"));
    assert_eq!(copy, original);
}

/// Nothing but an entry the templates group holds itself is made from: not one
/// outside it, not one in a folder inside it, not one whose templates group
/// went to the bin. A template is not made into the bin either, nor into a
/// folder that is not there. Nothing changes on the way.
#[test]
fn only_a_template_is_made_from() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = templated(scratch.path(), |database| {
        Some(named(database, "Templates"))
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let templates = only_group(&vault, "Templates");
    let template = entry_titled(&vault, "Card template").id;
    let outside = entry_titled(&vault, "Not a template").id;
    let nested = entry_titled(&vault, "Nested template").id;
    let doomed = vault
        .create_group(root, "Doomed")
        .expect("the folder is made");
    vault
        .delete_group(doomed, Deletion::Bin)
        .expect("it goes to the bin");
    let elsewhere = Database::new().root().id();

    let tree = vault.tree();
    let (count, edits) = (vault.count(), vault.edits());
    for (from, into) in [(outside, root), (nested, root)] {
        assert!(matches!(
            vault.create_from_template(from, into),
            Err(VaultError::NotATemplate)
        ));
    }
    assert!(matches!(
        vault.create_from_template(template, doomed),
        Err(VaultError::IntoRecycleBin)
    ));
    assert!(matches!(
        vault.create_from_template(template, elsewhere),
        Err(VaultError::NoSuchGroup)
    ));
    assert_eq!(vault.tree(), tree);
    assert_eq!((vault.count(), vault.edits()), (count, edits));

    vault
        .delete_group(templates, Deletion::Bin)
        .expect("it goes to the bin");
    assert!(matches!(
        vault.create_from_template(template, root),
        Err(VaultError::NotATemplate)
    ));
    vault
        .delete_group(templates, Deletion::Forever)
        .expect("and out of the file");
    assert!(matches!(
        vault.create_from_template(template, root),
        Err(VaultError::NoSuchEntry)
    ));
}

/// Coffer keeps the vault's templates group named in the file it writes, with
/// the date it was named, and lets go of the name only when the group goes
/// out of the file.
#[test]
fn the_templates_group_survives_a_save() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = templated(scratch.path(), |database| {
        Some(named(database, "Templates"))
    });
    let before = library(&path, BUILT_PASSWORD).meta;
    assert!(before.entry_templates_group_changed.is_some());

    let mut vault = open(&path, BUILT_PASSWORD);
    let templates = only_group(&vault, "Templates");
    let root = vault.tree().id;
    vault
        .create_entry(root, Kind::Wifi)
        .expect("the entry is made");
    vault.save().expect("the database saves");
    drop(vault);

    let after = library(&path, BUILT_PASSWORD).meta;
    assert_eq!(after.entry_templates_group, Some(templates.uuid()));
    assert_eq!(
        after.entry_templates_group_changed,
        before.entry_templates_group_changed
    );

    let mut vault = open(&path, BUILT_PASSWORD);
    vault
        .delete_group(templates, Deletion::Bin)
        .expect("it goes to the bin");
    vault
        .delete_group(templates, Deletion::Forever)
        .expect("and out of the file");
    vault.save().expect("the database saves");
    drop(vault);
    assert_eq!(
        library(&path, BUILT_PASSWORD).meta.entry_templates_group,
        None,
        "the file names a templates group that is not in it"
    );
    assert!(folder(&open(&path, BUILT_PASSWORD).tree(), templates).is_none());
}
