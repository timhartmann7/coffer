//! Serialisation properties: whatever tree goes in comes back out.
//!
//! The round-trip suite proves fidelity against KeePassXC on a fixed set of
//! fixtures. This proves it against trees nobody chose, which is where the
//! cases nobody thought of live.

use keepass::Database;
use keepass::db::Value;
use proptest::prelude::*;

use crate::support::{BUILT_PASSWORD, built, open};

/// Text a KeePass file can carry.
///
/// Control characters are excluded because XML 1.0 cannot represent them, so a
/// database holding one is a file no reader can open. Coffer refuses to write
/// one through [`Vault::set_field`][vault_core::Vault::set_field]; this
/// generator builds databases through the library directly, which would happily
/// write the broken file.
fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("   ".to_owned()),
        "[a-zA-Z0-9 ]{0,40}",
        Just("<script>&amp;</script>".to_owned()),
        Just("עברית \u{202e}reversed\u{202c}".to_owned()),
        Just("ユニコード 🔐 é\u{301}".to_owned()),
        Just("x".repeat(5000)),
    ]
}

fn field() -> impl Strategy<Value = (String, String, bool)> {
    ("[a-z]{1,12}", text(), any::<bool>())
}

#[derive(Debug, Clone)]
struct EntryShape {
    fields: Vec<(String, String, bool)>,
    tags: Vec<String>,
    attachments: Vec<(String, Vec<u8>)>,
}

#[derive(Debug, Clone)]
struct GroupShape {
    name: String,
    entries: Vec<EntryShape>,
    sections: Vec<GroupShape>,
}

fn entry() -> impl Strategy<Value = EntryShape> {
    (
        prop::collection::vec(field(), 0..8),
        prop::collection::vec("[a-z]{1,8}", 0..4),
        prop::collection::vec(
            ("[a-z.]{1,10}", prop::collection::vec(any::<u8>(), 0..64)),
            0..3,
        ),
    )
        .prop_map(|(fields, tags, attachments)| EntryShape {
            fields,
            tags,
            attachments,
        })
}

/// A group tree up to four levels deep, which is as deep as a generated case
/// needs to be: the hundred-level case is a fixed test of its own.
fn group() -> impl Strategy<Value = GroupShape> {
    let leaf =
        ("[a-z ]{1,10}", prop::collection::vec(entry(), 0..4)).prop_map(|(name, entries)| {
            GroupShape {
                name,
                entries,
                sections: Vec::new(),
            }
        });

    leaf.prop_recursive(4, 16, 3, |inner| {
        (
            "[a-z ]{1,10}",
            prop::collection::vec(entry(), 0..3),
            prop::collection::vec(inner, 0..3),
        )
            .prop_map(|(name, entries, sections)| GroupShape {
                name,
                entries,
                sections,
            })
    })
}

fn fill(database: &mut Database, under: keepass::db::GroupId, shape: &[GroupShape]) {
    for section in shape {
        let group = database
            .group_mut(under)
            .expect("the parent group is there")
            .add_group()
            .edit(|group| group.name.clone_from(&section.name))
            .id();

        for entry in &section.entries {
            let mut parent = database.group_mut(group).expect("the group is there");
            let mut handle = parent.add_entry();

            handle.tags.clone_from(&entry.tags);
            for (key, value, protected) in &entry.fields {
                handle.set(
                    key.clone(),
                    if *protected {
                        Value::protected(value.clone())
                    } else {
                        Value::unprotected(value.clone())
                    },
                );
            }
            for (name, bytes) in &entry.attachments {
                handle.add_attachment(name.clone(), Value::unprotected(bytes.clone()));
            }
        }

        fill(database, group, &section.sections);
    }
}

/// Everything about a database that a save must preserve, read back through the
/// public API so that the comparison is of what Coffer can actually see.
fn observe(vault: &vault_core::Vault) -> Vec<String> {
    fn walk(group: &vault_core::model::Project, into: &mut Vec<String>, vault: &vault_core::Vault) {
        into.push(format!(
            "group {} recycle={}",
            group.name, group.is_recycle_bin
        ));

        let mut entries: Vec<String> = group
            .entries
            .iter()
            .map(|entry| {
                let mut parts = vec![format!("tags {:?}", entry.tags)];
                for field in &entry.fields {
                    let revealed = vault
                        .reveal(entry.id, &field.name)
                        .and_then(|value| value.expose_str().map(str::to_owned));
                    parts.push(format!("field {} = {revealed:?}", field.name));
                }
                for attachment in &entry.attachments {
                    let bytes = vault
                        .attachment(entry.id, &attachment.name)
                        .map(|value| value.expose().to_vec());
                    parts.push(format!(
                        "attachment {} = {:?} ({} bytes)",
                        attachment.name, bytes, attachment.size
                    ));
                }
                parts.join("; ")
            })
            .collect();
        entries.sort();
        into.extend(entries);

        let mut sections: Vec<&vault_core::model::Project> = group.sections.iter().collect();
        sections.sort_by(|a, b| a.name.cmp(&b.name));
        for section in sections {
            walk(section, into, vault);
        }
    }

    let mut seen = Vec::new();
    walk(&vault.tree(), &mut seen, vault);
    seen
}

proptest! {
    // Every case builds, saves and reopens a database twice. Sixty-four cases
    // keep the whole suite inside half a minute; raise it when hunting.
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn any_tree_survives_a_save_and_a_reopen(shape in prop::collection::vec(group(), 0..4)) {
        let scratch = tempfile::tempdir().expect("a scratch directory");
        let database = built(scratch.path(), "generated.kdbx", |db| {
            let root = db.root_mut().id();
            fill(db, root, &shape);
        });

        let mut vault = open(&database, BUILT_PASSWORD);
        let before = observe(&vault);
        vault.save().expect("the database saves");
        drop(vault);

        let reopened = open(&database, BUILT_PASSWORD);
        prop_assert_eq!(before, observe(&reopened));
    }
}
