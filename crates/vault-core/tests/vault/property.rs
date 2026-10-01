//! Properties: whatever goes in comes back out, over trees and runs of changes
//! nobody chose.
//!
//! The round-trip suite proves fidelity against KeePassXC on a fixed set of
//! fixtures. This proves it against shapes nobody thought of, which is where the
//! cases nobody thought of live.

use crate::support::{self, BUILT_PASSWORD, built, open};
use keepass::Database;
use keepass::db::Value;
use proptest::prelude::*;

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
            .filter_map(|summary| vault.entry(summary.id))
            .map(|entry| {
                let mut parts = vec![format!("tags {:?}", entry.tags)];
                for field in &entry.fields {
                    let revealed = vault
                        .reveal(entry.id, &field.name)
                        .and_then(|value| value.expose_str().map(str::to_owned));
                    // The protection flag is part of the field. A save that
                    // wrote every value in the clear would otherwise pass.
                    parts.push(format!(
                        "field {} protected={} = {revealed:?}",
                        field.name,
                        field.value.open().is_none()
                    ));
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

    /// The same claim about a database Coffer made rather than one the suite
    /// assembled: a vault that starts empty is an ordinary vault the moment it
    /// exists, whatever is then put in it.
    #[test]
    fn any_tree_put_into_a_vault_coffer_made_survives_the_same_way(
        shape in prop::collection::vec(group(), 0..4)
    ) {
        let scratch = tempfile::tempdir().expect("a scratch directory");
        let path = scratch.path().join("made.kdbx");
        let made = vault_core::Vault::create(
            &path,
            crate::support::password(BUILT_PASSWORD),
            &vault_core::Recipe { name: "Work", work: vault_core::kdf::Work::at(1) },
        ).expect("the vault is made");
        drop(made);

        {
            let mut database = keepass::Database::parse(
                &std::fs::read(&path).expect("the file reads")[..],
                keepass::DatabaseKey::new().with_password(BUILT_PASSWORD),
            ).expect("the file opens");
            let root = database.root_mut().id();
            fill(&mut database, root, &shape);
            let mut file = std::fs::File::create(&path).expect("the file is rewritten");
            database
                .save(&mut file, keepass::DatabaseKey::new().with_password(BUILT_PASSWORD))
                .expect("it saves");
        }

        let mut vault = open(&path, BUILT_PASSWORD);
        let before = observe(&vault);
        vault.save().expect("the database saves");
        drop(vault);

        let reopened = open(&path, BUILT_PASSWORD);
        prop_assert_eq!(before, observe(&reopened));
    }
}

/// One thing a reader can do that touches the pool of files.
#[derive(Debug, Clone)]
enum Act {
    Add(usize, u8),
    /// A file offered under a name the entry already gives another, and what
    /// the reader answers when they are asked about it.
    AddAgain(usize, usize, u8, Answer),
    Remove(usize, usize),
    RemoveWithVersions(usize, usize),
    /// Any ordinary edit, which is what writes a version - and a version is what
    /// holds a file in the number it has.
    Edit(usize),
    ClearHistory(usize),
    DeleteEntry(usize),
    EmptyBin,
}

/// What a reader can say when a file's name is taken.
#[derive(Debug, Clone, Copy)]
enum Answer {
    Neither,
    Both,
    Replace,
}

fn act() -> impl Strategy<Value = Act> {
    let answer = prop_oneof![
        Just(Answer::Neither),
        Just(Answer::Both),
        Just(Answer::Replace)
    ];
    prop_oneof![
        (0usize..8, any::<u8>()).prop_map(|(entry, byte)| Act::Add(entry, byte)),
        (0usize..8, 0usize..4, any::<u8>(), answer)
            .prop_map(|(entry, file, byte, answer)| Act::AddAgain(entry, file, byte, answer)),
        (0usize..8, 0usize..4).prop_map(|(entry, file)| Act::Remove(entry, file)),
        (0usize..8, 0usize..4).prop_map(|(entry, file)| Act::RemoveWithVersions(entry, file)),
        (0usize..8).prop_map(Act::Edit),
        (0usize..8).prop_map(Act::ClearHistory),
        (0usize..8).prop_map(Act::DeleteEntry),
        Just(Act::EmptyBin),
    ]
}

/// Every entry in the vault, the bin included, and the files each one names with
/// the bytes behind them.
///
/// Byte for byte on purpose. The failure this is looking for is not a file that
/// vanishes but a file that comes back on somebody else's entry, which is what a
/// hole in the pool produces and what nothing but the bytes can catch.
fn files(
    vault: &vault_core::Vault,
) -> std::collections::BTreeMap<String, std::collections::BTreeMap<String, Vec<u8>>> {
    fn walk(
        group: &vault_core::model::Project,
        vault: &vault_core::Vault,
        into: &mut std::collections::BTreeMap<String, std::collections::BTreeMap<String, Vec<u8>>>,
    ) {
        for summary in &group.entries {
            let mut held = std::collections::BTreeMap::new();
            if let Some(entry) = vault.entry(summary.id) {
                for attachment in &entry.attachments {
                    let bytes = vault
                        .attachment(summary.id, &attachment.name)
                        .map(|value| value.expose().to_vec())
                        .unwrap_or_default();
                    held.insert(attachment.name.clone(), bytes);
                }
            }
            into.insert(summary.id.to_string(), held);
        }
        for section in &group.sections {
            walk(section, vault, into);
        }
    }

    let mut found = std::collections::BTreeMap::new();
    walk(&vault.tree(), vault, &mut found);
    found
}

fn entry_ids(vault: &vault_core::Vault) -> Vec<vault_core::model::EntryId> {
    fn walk(group: &vault_core::model::Project, into: &mut Vec<vault_core::model::EntryId>) {
        for entry in &group.entries {
            into.push(entry.id);
        }
        for section in &group.sections {
            walk(section, into);
        }
    }

    let mut found = Vec::new();
    walk(&vault.tree(), &mut found);
    found
}

proptest! {
    // Each case is a whole vault built, changed a dozen times, and saved and
    // reopened after every change.
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// A random run of the things that move files about, with the file the
    /// reader can see checked against the file that comes back off the disk
    /// after every one of them.
    ///
    /// The pool has to stay an unbroken run from zero or the next reader gets
    /// somebody else's bytes, and the arithmetic that keeps it that way now has
    /// to leave the files a previous version names on the numbers they have.
    /// That is far too much to hold in one's head, which is what this is for.
    #[test]
    fn no_run_of_changes_hands_a_file_to_the_wrong_entry(acts in prop::collection::vec(act(), 1..14)) {
        let scratch = tempfile::tempdir().expect("a scratch directory");
        let path = scratch.path().join("pool.kdbx");
        let mut vault = vault_core::Vault::create(
            &path,
            crate::support::password(BUILT_PASSWORD),
            &vault_core::Recipe { name: "Work", work: vault_core::kdf::Work::at(1) },
        ).expect("the vault is made");

        // Four entries, two of them carrying a file, and one already edited so
        // that a version is holding a file before anything else happens.
        let root = vault.tree().id;
        for round in 0..4u8 {
            let id = vault.create_entry(root).expect("the entry is made");
            vault
                .set_field(id, "Title", vault_core::NewValue::Open(format!("entry {round}")))
                .expect("the title is written");
            if round < 2 {
                support::attach(&mut vault, id, &format!("start-{round}.bin"), &[round; 48]);
            }
            if round == 1 {
                vault
                    .set_field(id, "UserName", vault_core::NewValue::Open("edited".to_owned()))
                    .expect("the login is written");
            }
        }
        vault.save().expect("the database saves");

        for (step, act) in acts.iter().enumerate() {
            let ids = entry_ids(&vault);
            if ids.is_empty() {
                break;
            }
            let pick = |n: usize| ids[n % ids.len()];
            let before = files(&vault);

            // What the vault should hold once this has happened. A refusal
            // leaves it exactly as it was, which is half of what is checked
            // here: an operation that was answered no must cost nothing.
            let mut expected = before.clone();
            let outcome = match act {
                Act::Add(entry, byte) => {
                    let id = pick(*entry);
                    let name = format!("added-{step}.bin");
                    let bytes = vec![*byte; 32 + step];
                    let done = vault.add_attachment(id, &name, &bytes);
                    if matches!(done, Ok(vault_core::Attached::Added)) {
                        expected.entry(id.to_string()).or_default().insert(name, bytes);
                    }
                    done.map(drop)
                }
                Act::AddAgain(entry, file, byte, answer) => {
                    let id = pick(*entry);
                    let held: Vec<String> = before
                        .get(&id.to_string())
                        .map(|files| files.keys().cloned().collect())
                        .unwrap_or_default();
                    if held.is_empty() {
                        continue;
                    }
                    let name = held[*file % held.len()].clone();
                    let bytes = vec![*byte; 16 + step];

                    // Asked, and nothing changes until the answer.
                    let asked = vault
                        .add_attachment(id, &name, &bytes)
                        .expect("a file of a size and name the entry holds is offerable");
                    let vault_core::Attached::Taken(clash) = asked else {
                        panic!("step {step} put {name:?} over the file of that name without asking");
                    };
                    prop_assert_eq!(&files(&vault), &before, "the question alone changed the files");

                    match answer {
                        Answer::Neither => Ok(()),
                        Answer::Both => {
                            let done = vault.keep_both(id, &name, &bytes);
                            if done.is_ok() {
                                expected.entry(id.to_string()).or_default().insert(clash.free, bytes);
                            }
                            done
                        }
                        Answer::Replace => {
                            let done = vault.replace_attachment(id, &name, &bytes);
                            if done.is_ok() {
                                expected.entry(id.to_string()).or_default().insert(name, bytes);
                            }
                            done
                        }
                    }
                }
                Act::Remove(entry, file) | Act::RemoveWithVersions(entry, file) => {
                    let id = pick(*entry);
                    let held: Vec<String> = before
                        .get(&id.to_string())
                        .map(|files| files.keys().cloned().collect())
                        .unwrap_or_default();
                    if held.is_empty() {
                        continue;
                    }
                    let name = held[*file % held.len()].clone();
                    let done = match act {
                        Act::Remove(..) => vault.remove_attachment(id, &name),
                        _ => vault.remove_attachment_and_versions(id, &name),
                    };
                    if done.is_ok() {
                        expected.entry(id.to_string()).or_default().remove(&name);
                    }
                    done
                }
                Act::Edit(entry) => vault.set_field(
                    pick(*entry),
                    "Notes",
                    vault_core::NewValue::Open(format!("note {step}")),
                ),
                Act::ClearHistory(entry) => vault.clear_history(pick(*entry)),
                Act::DeleteEntry(entry) => {
                    let id = pick(*entry);
                    // What the window would have shown for it, as the reader
                    // agreed to it.
                    let shown = vault
                        .entry(id)
                        .map_or(vault_core::model::Deletion::Bin, |entry| entry.deletion);
                    let done = vault.delete_entry(id, shown);
                    // Into the bin it stays an entry; out of the bin it is gone.
                    if done.is_ok() && vault.entry(id).is_none() {
                        expected.remove(&id.to_string());
                    }
                    done
                }
                Act::EmptyBin => {
                    let done = vault.empty_recycle_bin();
                    // Whatever could go has gone, refusal or not, so what the
                    // vault holds now is what the file has to come back with.
                    expected = files(&vault);
                    done
                }
            };
            let _ = outcome;

            // The one thing that may never fail. A save that refuses here is the
            // pool having lost its order, which is the corruption this is about.
            vault.save().unwrap_or_else(|error| {
                panic!("step {step} left a database that will not save: {error}")
            });
            drop(vault);

            let reopened = open(&path, BUILT_PASSWORD);
            prop_assert_eq!(
                files(&reopened),
                expected,
                "step {} ({:?}) changed what the file gives back",
                step,
                act
            );
            vault = reopened;
        }
    }
}
