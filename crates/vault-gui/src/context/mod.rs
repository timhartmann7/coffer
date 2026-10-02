//! The menus a right-click draws: what each one offers, worked out from the
//! open vault, and what each item sends back to the window.
//!
//! Decided here rather than in the window because the facts are here: whether
//! there is a password to copy, whether Coffer writes this vault back - for
//! whichever reason it does not - and reads its files out, what deleting a
//! thing does. An item that does not apply is drawn greyed out
//! rather than left out, so that a menu has the same lines for every row, and
//! a greyed item cannot be chosen: AppKit draws what it is told and does not
//! decide for itself (`setAutoenablesItems(false)` in muda 0.19.3).
//!
//! Every item is a press the window answers with the function its button
//! runs. Nothing here changes the vault, and a menu offers nothing that the
//! command behind the button would refuse.
//!
//! Nothing in this file knows AppKit or Tauri, so every menu is read in a test
//! from a real vault; `shown.rs` puts one under the pointer.

mod shown;

pub use shown::{Menus, ask, forget, picked, pop};

use vault_core::model::{self, Project};
use vault_core::{Vault, VaultError};

use crate::dto::{self, Chosen, Deleting, Deletion, FieldKind, Subject};
use crate::error::Failure;
use crate::menu::Command;

/// One line of a menu.
pub enum Item {
    /// Something to choose. One with no pick is drawn greyed out.
    Choice {
        label: String,
        pick: Option<Pick>,
    },
    Separator,
    /// A submenu, greyed out when nothing in it can be chosen.
    Menu {
        label: String,
        enabled: bool,
        items: Vec<Item>,
    },
}

/// What an item does, without the ids of what the menu is about: those are
/// the subject's. Move to for fifty thousand chosen entries is a line for
/// every folder, and none of those lines holds fifty thousand ids.
pub enum Pick {
    /// Copies a field whole, named as the file names it.
    Copy(String),
    /// Copies a revealed value, or the part of it selected.
    CopyValue,
    Show,
    Hide,
    Change,
    MakeOne,
    Remove,
    OpenAddress,
    Duplicate,
    /// Moves into the place with this id.
    MoveInto(String),
    /// Deletes, saying what deleting does: one for each entry of the subject,
    /// in its order, or one for a folder.
    Delete(Vec<Deletion>),
    PutBack,
    NewEntry,
    NewFolder,
    Rename,
    EmptyBin,
    Save,
    RemoveFile,
}

// The words of these menus. The ones that do what an item of the menu bar does
// are that item's (`Command::title`); every other is here, once.
const OPEN_ADDRESS: &str = "Open Address";
const MOVE_TO: &str = "Move to";
const FOREVER: &str = "Delete Forever…";
const PUT_BACK: &str = "Put Back";
const NEW_ENTRY: &str = "New Entry Here";
const NEW_FOLDER: &str = "New Folder Inside";
const RENAME: &str = "Rename";
/// A folder's, which asks first: everything in it goes with it.
const FOLDER_TO_BIN: &str = "Move to Recycle Bin…";
const EMPTY_BIN: &str = "Empty Recycle Bin…";
const SHOW: &str = "Show";
const HIDE: &str = "Hide";
const COPY: &str = "Copy";
const CHANGE: &str = "Change…";
/// Change, for a password that is not there yet.
const SET_ONE: &str = "Set One…";
const MAKE_ONE: &str = "Make a New One…";
const REMOVE: &str = "Remove";
const SAVE_TO: &str = "Save to…";
/// A file's, which asks first: nothing keeps a removed file.
const REMOVE_FILE: &str = "Remove…";

/// What a right-click on `subject` offers, in the order it is drawn.
///
/// Something the vault does not have - an entry, a folder, a field or a file
/// that has gone, or an id that is not one - is refused the way the command
/// behind its button would refuse it, and no menu is drawn.
pub fn offered(vault: &Vault, subject: &Subject) -> Result<Vec<Item>, Failure> {
    let writable = vault.read_only().is_none();
    match subject {
        Subject::Entry { entry, places } => Ok(one(&read(vault, entry)?, places, writable)),
        Subject::Entries { entries, places } => many(vault, entries, places, writable),
        Subject::Folder { group, places } => folder(&vault.tree(), group, places, writable),
        Subject::Bin => {
            let tree = vault.tree();
            let bin = groups(&tree)
                .find(|group| group.is_recycle_bin)
                .ok_or_else(Failure::no_such_group)?;
            Ok(emptying(bin, writable))
        }
        Subject::Field {
            entry,
            field: name,
            shown,
        } => field(&read(vault, entry)?, name, *shown, writable),
        Subject::File { entry, name } => {
            file(&read(vault, entry)?, name, writable, vault.files_readable())
        }
        // A version's value as well as the entry's, so the field need not be
        // one the entry has now; the entry has to be there.
        Subject::Value { entry, .. } => {
            read(vault, entry)?;
            Ok(vec![
                choice(COPY, Some(Pick::CopyValue)),
                Item::Separator,
                choice(HIDE, Some(Pick::Hide)),
            ])
        }
    }
}

/// The item `pick` of a menu about `subject`, with the ids it is about, or
/// nothing for a pick no menu about such a subject offers.
pub fn chosen(subject: &Subject, pick: &Pick) -> Option<Chosen> {
    let item = |entry: &str, field: &str| (entry.to_owned(), field.to_owned());
    Some(match (subject, pick) {
        (Subject::Entry { entry, .. } | Subject::Field { entry, .. }, Pick::Copy(field)) => {
            Chosen::CopyField {
                entry: entry.clone(),
                field: field.clone(),
            }
        }
        (
            Subject::Value {
                entry,
                field,
                range,
            },
            Pick::CopyValue,
        ) => Chosen::CopyValue {
            entry: entry.clone(),
            field: field.clone(),
            range: *range,
        },
        (
            Subject::Field { entry, field, .. } | Subject::Value { entry, field, .. },
            Pick::Show | Pick::Hide,
        ) => {
            let (entry, field) = item(entry, field);
            match pick {
                Pick::Show => Chosen::ShowField { entry, field },
                _ => Chosen::HideField { entry, field },
            }
        }
        (Subject::Field { entry, field, .. }, Pick::Change | Pick::MakeOne | Pick::Remove) => {
            let (entry, field) = item(entry, field);
            match pick {
                Pick::Change => Chosen::ChangeField { entry, field },
                Pick::MakeOne => Chosen::MakeOne { entry, field },
                _ => Chosen::RemoveField { entry, field },
            }
        }
        (Subject::Entry { entry, .. }, Pick::OpenAddress) => Chosen::OpenAddress {
            entry: entry.clone(),
        },
        (Subject::Entry { entry, .. }, Pick::Duplicate) => Chosen::Duplicate {
            entry: entry.clone(),
        },
        (Subject::Entry { entry, .. }, Pick::MoveInto(into)) => Chosen::MoveEntries {
            entries: vec![entry.clone()],
            into: into.clone(),
        },
        (Subject::Entries { entries, .. }, Pick::MoveInto(into)) => Chosen::MoveEntries {
            entries: entries.clone(),
            into: into.clone(),
        },
        (Subject::Folder { group, .. }, Pick::MoveInto(into)) => Chosen::MoveFolder {
            group: group.clone(),
            into: into.clone(),
        },
        (Subject::Entry { entry, .. }, Pick::Delete(deletions)) => {
            deleting(std::slice::from_ref(entry), deletions)?
        }
        (Subject::Entries { entries, .. }, Pick::Delete(deletions)) => {
            deleting(entries, deletions)?
        }
        (Subject::Folder { group, .. }, Pick::Delete(deletions)) => match deletions.as_slice() {
            [deletion] => Chosen::DeleteFolder {
                group: group.clone(),
                deletion: *deletion,
            },
            _ => return None,
        },
        (Subject::Entry { entry, .. }, Pick::PutBack) => Chosen::PutBackEntries {
            entries: vec![entry.clone()],
        },
        (Subject::Entries { entries, .. }, Pick::PutBack) => Chosen::PutBackEntries {
            entries: entries.clone(),
        },
        (Subject::Folder { group, .. }, Pick::PutBack) => Chosen::PutBackFolder {
            group: group.clone(),
        },
        (Subject::Folder { group, .. }, Pick::NewEntry) => Chosen::NewEntryIn {
            group: group.clone(),
        },
        (Subject::Folder { group, .. }, Pick::NewFolder) => Chosen::NewFolderIn {
            group: group.clone(),
        },
        (Subject::Folder { group, .. }, Pick::Rename) => Chosen::RenameFolder {
            group: group.clone(),
        },
        // The bin is a folder too, in the tree and in the bin's own list.
        (Subject::Bin | Subject::Folder { .. }, Pick::EmptyBin) => Chosen::EmptyBin,
        (Subject::File { entry, name }, Pick::Save) => Chosen::SaveFile {
            entry: entry.clone(),
            name: name.clone(),
        },
        (Subject::File { entry, name }, Pick::RemoveFile) => Chosen::RemoveFile {
            entry: entry.clone(),
            name: name.clone(),
        },
        _ => return None,
    })
}

/// Each entry with what the menu said deleting it does, or nothing when the
/// two lists do not pair off.
fn deleting(entries: &[String], deletions: &[Deletion]) -> Option<Chosen> {
    (entries.len() == deletions.len()).then(|| Chosen::DeleteEntries {
        entries: entries
            .iter()
            .zip(deletions)
            .map(|(entry, deletion)| Deleting {
                entry: entry.clone(),
                deletion: *deletion,
            })
            .collect(),
    })
}

fn choice(label: impl Into<String>, pick: Option<Pick>) -> Item {
    Item::Choice {
        label: label.into(),
        pick,
    }
}

/// A submenu, which can be chosen from when anything in it can.
fn submenu(label: String, items: Vec<Item>) -> Item {
    let enabled = items.iter().any(|item| match item {
        Item::Choice { pick, .. } => pick.is_some(),
        Item::Menu { enabled, .. } => *enabled,
        Item::Separator => false,
    });
    Item::Menu {
        label,
        enabled,
        items,
    }
}

/// An entry as the pane would draw it, or the refusal the pane would get.
fn read(vault: &Vault, id: &str) -> Result<dto::Entry, Failure> {
    vault
        .entry(dto::entry_id(id)?)
        .map(|entry| dto::Entry::of(&entry))
        .ok_or_else(Failure::no_such_entry)
}

/// Whether nothing on the entry may be changed: a vault Coffer does not write
/// back, or an entry in the recycle bin. The pane is drawn read only on the
/// same two facts (`untouchable` in `model.ts`), so the menu greys out what
/// the pane does not draw.
fn untouchable(entry: &dto::Entry, writable: bool) -> bool {
    !writable || entry.binned.is_some()
}

/// What deleting an entry is called, by what it does: the menu bar's item, or
/// the one that asks first.
fn deletion_label(deletion: Deletion) -> &'static str {
    match deletion {
        Deletion::Bin => Command::MoveToBin.title(),
        Deletion::Forever => FOREVER,
    }
}

/// A row of the list: its copies and its address, then what can be done to
/// the entry where it stands. In the bin, only putting it back and letting it
/// go.
fn one(entry: &dto::Entry, places: &[dto::Place], writable: bool) -> Vec<Item> {
    // The rule the pane's own copy buttons and the menu bar's items are drawn
    // on: a value Rust holds, whether or not the file protects it.
    let filled = |kind: FieldKind| {
        entry
            .fields
            .iter()
            .find(|field| field.kind == kind && !field.empty)
            .map(|field| Pick::Copy(field.name.clone()))
    };
    let opens = entry
        .fields
        .iter()
        .any(|field| field.kind == FieldKind::Url && field.openable);

    let mut items = vec![
        choice(Command::CopyLogin.title(), filled(FieldKind::Username)),
        choice(Command::CopyPassword.title(), filled(FieldKind::Password)),
        choice(OPEN_ADDRESS, opens.then_some(Pick::OpenAddress)),
        Item::Separator,
    ];
    if entry.binned.is_some() {
        items.push(choice(PUT_BACK, writable.then_some(Pick::PutBack)));
    } else {
        items.push(choice(
            Command::Duplicate.title(),
            (!untouchable(entry, writable)).then_some(Pick::Duplicate),
        ));
        items.push(moving(MOVE_TO, places, writable));
        items.push(Item::Separator);
    }
    items.push(choice(
        deletion_label(entry.deletion),
        writable.then(|| Pick::Delete(vec![entry.deletion])),
    ));
    items
}

/// The rows chosen in the list, as their bar has them: moved and deleted
/// together, and never copied, because Rust copies one field of one entry. A
/// choice of one is a row.
fn many(
    vault: &Vault,
    ids: &[String],
    places: &[dto::Place],
    writable: bool,
) -> Result<Vec<Item>, Failure> {
    match ids {
        [] => return Err(Failure::no_such_entry()),
        [only] => return Ok(one(&read(vault, only)?, places, writable)),
        _ => {}
    }

    let entries = ids
        .iter()
        .map(|id| {
            vault
                .entry(dto::entry_id(id)?)
                .ok_or_else(Failure::no_such_entry)
        })
        .collect::<Result<Vec<model::Entry>, Failure>>()?;
    Ok(together(&entries, places, writable))
}

/// Several chosen rows, as the vault answered for each: in the bin or not,
/// and what deleting it does.
fn together(entries: &[model::Entry], places: &[dto::Place], writable: bool) -> Vec<Item> {
    let count = entries.len();
    let binned = entries
        .iter()
        .filter(|entry| entry.binned.is_some())
        .count();
    let deletions: Vec<Deletion> = entries
        .iter()
        .map(|entry| Deletion::of(entry.deletion))
        .collect();

    let delete = choice(
        if deletions.contains(&Deletion::Forever) {
            format!("Delete {count} Entries Forever…")
        } else {
            format!("Move {count} Entries to Recycle Bin")
        },
        writable.then_some(Pick::Delete(deletions)),
    );
    if binned == 0 {
        vec![
            moving(&format!("Move {count} Entries to"), places, writable),
            Item::Separator,
            delete,
        ]
    } else if binned == count {
        vec![
            choice(
                format!("Put Back {count} Entries"),
                writable.then_some(Pick::PutBack),
            ),
            delete,
        ]
    } else {
        vec![delete]
    }
}

/// A folder: what can be made in it, then what can be done to it. The top of
/// the vault, which the window never sends, can only be made in; the bin is
/// emptied; a folder in the bin is put back or let go.
fn folder(
    tree: &Project,
    id: &str,
    places: &[dto::Place],
    writable: bool,
) -> Result<Vec<Item>, Failure> {
    let id = dto::group_id(id)?;
    let found = groups(tree)
        .find(|group| group.id == id)
        .ok_or_else(Failure::no_such_group)?;
    if found.is_recycle_bin {
        return Ok(emptying(found, writable));
    }
    let deletion = Deletion::of(found.deletion);
    let delete = writable.then(|| Pick::Delete(vec![deletion]));
    if found.binned.is_some() {
        return Ok(vec![
            choice(PUT_BACK, writable.then_some(Pick::PutBack)),
            choice(FOREVER, delete),
        ]);
    }

    let mut items = vec![
        choice(NEW_ENTRY, writable.then_some(Pick::NewEntry)),
        choice(NEW_FOLDER, writable.then_some(Pick::NewFolder)),
    ];
    if found.id != tree.id {
        items.extend([
            Item::Separator,
            choice(RENAME, writable.then_some(Pick::Rename)),
            moving(MOVE_TO, places, writable),
            Item::Separator,
            choice(
                match deletion {
                    Deletion::Bin => FOLDER_TO_BIN,
                    Deletion::Forever => FOREVER,
                },
                delete,
            ),
        ]);
    }
    Ok(items)
}

/// The bin's own row: emptied, after the question under it, when there is
/// anything in it.
fn emptying(bin: &Project, writable: bool) -> Vec<Item> {
    let holds = !bin.entries.is_empty() || !bin.sections.is_empty();
    vec![choice(
        EMPTY_BIN,
        (writable && holds).then_some(Pick::EmptyBin),
    )]
}

/// A field's row, offering what the row's own buttons do.
///
/// A standard field the file left out is one with nothing in it - the password
/// row of an entry with no password is drawn all the same - and a field of the
/// reader's own that is not there is refused.
fn field(
    entry: &dto::Entry,
    name: &str,
    shown: bool,
    writable: bool,
) -> Result<Vec<Item>, Failure> {
    let (kind, protected, empty) = match entry.fields.iter().find(|field| field.name == name) {
        Some(found) => (found.kind, found.protected, found.empty),
        None if FieldKind::of(name) != FieldKind::Custom => (FieldKind::of(name), false, true),
        None => return Err(VaultError::NoSuchField.into()),
    };
    let changes = !untouchable(entry, writable);
    let eye = choice(
        if shown { HIDE } else { SHOW },
        (!empty).then_some(if shown { Pick::Hide } else { Pick::Show }),
    );
    let copy = choice(COPY, (!empty).then(|| Pick::Copy(name.to_owned())));
    let remove = choice(REMOVE, changes.then_some(Pick::Remove));

    Ok(match kind {
        // A hidden field of the reader's own is replaced in a Change of its
        // own while it holds something; an empty one is typed into where it
        // stands, and has none.
        FieldKind::Custom if protected => vec![
            eye,
            copy,
            Item::Separator,
            choice(CHANGE, (changes && !empty).then_some(Pick::Change)),
            choice(MAKE_ONE, changes.then_some(Pick::MakeOne)),
            Item::Separator,
            remove,
        ],
        FieldKind::Custom => vec![copy, Item::Separator, remove],
        FieldKind::Password => vec![
            eye,
            copy,
            Item::Separator,
            choice(
                if empty { SET_ONE } else { CHANGE },
                changes.then_some(Pick::Change),
            ),
            choice(MAKE_ONE, changes.then_some(Pick::MakeOne)),
        ],
        _ => vec![eye, copy],
    })
}

/// A file on an entry: written out through the save panel, or taken off after
/// the question in its row. Written out only from a vault whose files can be
/// read: in a KDBX 3 one the save would be refused before its panel opened.
fn file(
    entry: &dto::Entry,
    name: &str,
    writable: bool,
    readable: bool,
) -> Result<Vec<Item>, Failure> {
    if !entry
        .attachments
        .iter()
        .any(|attachment| attachment.name == name)
    {
        return Err(VaultError::NoSuchAttachment.into());
    }
    Ok(vec![
        choice(SAVE_TO, readable.then_some(Pick::Save)),
        Item::Separator,
        choice(
            REMOVE_FILE,
            (!untouchable(entry, writable)).then_some(Pick::RemoveFile),
        ),
    ])
}

/// Move to, from the places the window sent: a folder with folders under it
/// is a submenu headed by the folder itself, and each place can be chosen when
/// the window said a move there would be taken and the vault is one Coffer
/// writes back. A name comes set apart and cut to length already (`isolated`
/// in `format.ts`), and what AppKit makes of it is `shown.rs`'s to answer.
///
/// Built with a stack, as the window's own lists are walked, so a hundred
/// folders deep is a hundred submenus and no deeper a call. A place deeper
/// than one below the place before it is taken as one below: it names a
/// folder whose parent was never sent.
fn moving(label: &str, places: &[dto::Place], writable: bool) -> Item {
    let mut open: Vec<(String, Vec<Item>)> = vec![(label.to_owned(), Vec::new())];
    for (at, place) in places.iter().enumerate() {
        let depth = place.depth.min(open.len().saturating_sub(1));
        while open.len() > depth + 1 {
            close(&mut open);
        }
        let line = choice(
            place.name.clone(),
            (writable && place.open).then(|| Pick::MoveInto(place.id.clone())),
        );
        if places.get(at + 1).is_some_and(|next| next.depth > depth) {
            open.push((place.name.clone(), vec![line, Item::Separator]));
        } else if let Some((_, items)) = open.last_mut() {
            items.push(line);
        }
    }
    while open.len() > 1 {
        close(&mut open);
    }
    let (label, items) = open.pop().unwrap_or_default();
    submenu(label, items)
}

/// Ends the submenu being filled, as a line of the one around it.
fn close(open: &mut Vec<(String, Vec<Item>)>) {
    if let Some((label, items)) = open.pop()
        && let Some((_, around)) = open.last_mut()
    {
        around.push(submenu(label, items));
    }
}

/// Every folder of the tree, the top and the bin included, walked with a
/// stack.
fn groups(root: &Project) -> impl Iterator<Item = &Project> {
    let mut pending = vec![root];
    std::iter::from_fn(move || {
        let here = pending.pop()?;
        pending.extend(here.sections.iter());
        Some(here)
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use vault_core::model::{EntryId, GroupId};
    use vault_core::{LockPolicy, ReadOnly};

    use super::*;
    use crate::fixtures::{RICH, SECRET, entry_titled, fixture, password, unlocked};
    use crate::session::Session;

    /// A KDBX 3.1 file with files in it: Coffer reads it and does not write it
    /// back, and it holds what the rich one holds.
    const READ_ONLY: &str = "rich-kdbx31.kdbx";

    /// The rich vault as one of its own backups, open: read only because of
    /// where it is rather than what it is.
    fn backup() -> (tempfile::TempDir, Session) {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let taken = directory.path().join("rich.kdbx.1.bak");
        std::fs::copy(fixture(RICH), &taken).expect("the fixture copies");
        let session = Session::new(Some(taken), None);
        session
            .unlock(password(SECRET), LockPolicy::Respect)
            .expect("the backup opens");
        (directory, session)
    }

    fn subject(sent: Value) -> Subject {
        serde_json::from_value(sent).expect("the window's subject reads")
    }

    fn menu(session: &Session, sent: Value) -> Result<Vec<Item>, Failure> {
        session
            .with(|vault| offered(vault, &subject(sent)))
            .expect("the vault is open")
    }

    fn opened(session: &Session, sent: Value) -> Vec<Item> {
        menu(session, sent).unwrap_or_else(|failure| panic!("refused: {failure:?}"))
    }

    fn code(failure: Failure) -> String {
        serde_json::to_value(failure).expect("a failure serialises")["code"]
            .as_str()
            .expect("it has a code")
            .to_owned()
    }

    /// What a line says, a separator being a dash.
    fn said(item: &Item) -> String {
        match item {
            Item::Choice { label, .. } | Item::Menu { label, .. } => label.clone(),
            Item::Separator => "-".to_owned(),
        }
    }

    fn lines(items: &[Item]) -> Vec<String> {
        items.iter().map(said).collect()
    }

    /// The line saying `label`, at the top of the menu.
    fn line<'a>(items: &'a [Item], label: &str) -> &'a Item {
        items
            .iter()
            .find(|item| said(item) == label)
            .unwrap_or_else(|| panic!("no {label:?} in {:?}", lines(items)))
    }

    /// What the line saying `label` does, or nothing for a greyed one.
    fn pick<'a>(items: &'a [Item], label: &str) -> Option<&'a Pick> {
        match line(items, label) {
            Item::Choice { pick, .. } => pick.as_ref(),
            _ => panic!("{label:?} is not a choice"),
        }
    }

    fn inside<'a>(items: &'a [Item], label: &str) -> &'a [Item] {
        match line(items, label) {
            Item::Menu { items, .. } => items,
            _ => panic!("{label:?} is not a submenu"),
        }
    }

    /// Every choice in a menu and in every menu inside it.
    fn choices(items: &[Item]) -> Vec<(&str, Option<&Pick>)> {
        let mut found = Vec::new();
        let mut pending: Vec<&Item> = items.iter().rev().collect();
        while let Some(item) = pending.pop() {
            match item {
                Item::Choice { label, pick } => found.push((label.as_str(), pick.as_ref())),
                Item::Menu { items, .. } => pending.extend(items.iter().rev()),
                Item::Separator => {}
            }
        }
        found
    }

    fn changes(pick: &Pick) -> bool {
        !matches!(
            pick,
            Pick::Copy(_)
                | Pick::CopyValue
                | Pick::Show
                | Pick::Hide
                | Pick::OpenAddress
                | Pick::Save
        )
    }

    fn tree(session: &Session) -> Project {
        session.tree().expect("the tree comes back")
    }

    fn group_named(session: &Session, name: &str) -> Project {
        groups(&tree(session))
            .find(|group| group.name == name)
            .cloned()
            .unwrap_or_else(|| panic!("no folder called {name:?}"))
    }

    fn bin(session: &Session) -> Project {
        groups(&tree(session))
            .find(|group| group.is_recycle_bin)
            .cloned()
            .expect("the fixture keeps a bin")
    }

    /// A folder called `name` made at the top of the vault and moved to the
    /// bin, which the fixture holds none of.
    fn binned_folder(session: &Session, name: &str) -> GroupId {
        let top = tree(session).id;
        session
            .with_mut(|vault| -> Result<GroupId, VaultError> {
                let made = vault.create_group(top, name)?;
                vault.delete_group(made, model::Deletion::Bin)?;
                Ok(made)
            })
            .expect("the vault is open")
            .expect("the folder goes to the bin")
    }

    /// The places the window sends for a thing in `from`: the top, then every
    /// folder outside the bin in the tree's order, each `depth` down from the
    /// top, open everywhere but `from` - the window's rule for entries.
    fn places(session: &Session, from: GroupId) -> Value {
        fn walk(group: &Project, depth: usize, from: GroupId, into: &mut Vec<Value>) {
            for section in &group.sections {
                if section.is_recycle_bin {
                    continue;
                }
                into.push(json!({
                    "id": section.id.to_string(),
                    "name": section.name,
                    "open": section.id != from,
                    "depth": depth,
                }));
                walk(section, depth + 1, from, into);
            }
        }
        let root = tree(session);
        let mut found = vec![json!({
            "id": root.id.to_string(),
            "name": "Top of the vault",
            "open": root.id != from,
            "depth": 0,
        })];
        walk(&root, 0, from, &mut found);
        Value::Array(found)
    }

    fn row(session: &Session, title: &str) -> Value {
        let entry = entry_titled(session, title);
        json!({
            "kind": "entry",
            "entry": entry.id.to_string(),
            "places": places(session, entry.group),
        })
    }

    /// A fresh entry has no login and no password, and its copies are drawn
    /// greyed out rather than left out; a filled one copies each by the name
    /// the file gives it.
    #[test]
    fn copying_is_offered_only_for_a_value_that_is_there() {
        let (_scratch, session) = unlocked(RICH);
        let personal = group_named(&session, "Personal").id;
        let fresh = session
            .with_mut(|vault| vault.create_entry(personal, vault_core::kind::Kind::Login))
            .expect("the vault is open")
            .expect("an entry is made");

        let empty = opened(
            &session,
            json!({ "kind": "entry", "entry": fresh.to_string(), "places": places(&session, personal) }),
        );
        assert!(pick(&empty, "Copy Login").is_none());
        assert!(pick(&empty, "Copy Password").is_none());

        let basic = opened(&session, row(&session, "basic"));
        assert!(matches!(pick(&basic, "Copy Login"), Some(Pick::Copy(name)) if name == "UserName"));
        assert!(
            matches!(pick(&basic, "Copy Password"), Some(Pick::Copy(name)) if name == "Password")
        );
        assert_eq!(
            lines(&basic),
            [
                "Copy Login",
                "Copy Password",
                "Open Address",
                "-",
                "Duplicate",
                "Move to",
                "-",
                "Move to Recycle Bin"
            ]
        );
    }

    /// The address is offered to be opened exactly when the pane draws its
    /// opener, which is when Rust would hand it to the system.
    #[test]
    fn an_address_is_offered_only_when_coffer_would_open_it() {
        let (_scratch, session) = unlocked(RICH);
        let dangerous = entry_titled(&session, "dangerous urls").id;

        for (address, opens) in [
            ("javascript:alert(document.domain)", false),
            ("JaVaScRiPt:alert(1)", false),
            ("  javascript:alert(1)", false),
            ("data:text/html;base64,PHNjcmlwdD4=", false),
            ("file:///etc/passwd", false),
            ("vbscript:msgbox(1)", false),
            ("https://example.com", true),
        ] {
            session
                .with_mut(|vault| {
                    vault.set_field(
                        dangerous,
                        model::fields::URL,
                        vault_core::NewValue::Open(address.to_owned()),
                    )
                })
                .expect("the vault is open")
                .expect("the address is written");
            let items = opened(&session, row(&session, "dangerous urls"));
            assert_eq!(pick(&items, "Open Address").is_some(), opens, "{address}");
            assert_eq!(
                opens,
                vault_core::url::openable(address).is_some(),
                "{address}: the menu's answer is not the opener's"
            );
        }
    }

    /// A vault Coffer will not write back is read, whatever the reason it is
    /// not written: every menu in it copies, opens and shows, and nothing in
    /// any of them changes the vault. A file is saved out of it only where the
    /// vault's files can be read - out of a backup, and not out of a KDBX 3
    /// vault, whose save would be refused before its panel opened.
    #[test]
    fn nothing_that_writes_is_offered_by_a_vault_coffer_will_not_write() {
        let (_format, format) = unlocked(READ_ONLY);
        let (_taken, taken) = backup();
        for (session, why, saves) in [
            (&format, ReadOnly::Kdbx3Attachments, false),
            (&taken, ReadOnly::Snapshot, true),
        ] {
            assert_eq!(
                session.with(Vault::read_only).expect("the vault is open"),
                Some(why)
            );
            let basic = entry_titled(session, "basic");
            let fields = entry_titled(session, "many custom fields");
            let files = entry_titled(session, "attachments");
            let attached = files
                .attachments
                .first()
                .expect("the entry carries files")
                .name
                .clone();
            let deleted = entry_titled(session, "deleted entry");
            let work = group_named(session, "Work");

            let subjects = [
                row(session, "basic"),
                row(session, "deleted entry"),
                json!({
                    "kind": "entries",
                    "entries": [basic.id.to_string(), fields.id.to_string()],
                    "places": places(session, basic.group),
                }),
                json!({ "kind": "folder", "group": work.id.to_string(), "places": places(session, work.id) }),
                json!({ "kind": "bin" }),
                json!({ "kind": "field", "entry": basic.id.to_string(), "field": "Password", "shown": false }),
                json!({ "kind": "field", "entry": fields.id.to_string(), "field": "custom-003", "shown": true }),
                json!({ "kind": "field", "entry": fields.id.to_string(), "field": "custom-001", "shown": false }),
                json!({ "kind": "field", "entry": deleted.id.to_string(), "field": "Password", "shown": false }),
                json!({ "kind": "file", "entry": files.id.to_string(), "name": attached }),
                json!({ "kind": "value", "entry": basic.id.to_string(), "field": "Password", "range": null }),
            ];
            for sent in subjects {
                let items = opened(session, sent.clone());
                for (label, pick) in choices(&items) {
                    assert!(
                        !pick.is_some_and(changes),
                        "{label:?} changes a read-only vault ({why:?}) in {sent}"
                    );
                }
                for item in &items {
                    if let Item::Menu { label, enabled, .. } = item {
                        assert!(!enabled, "{label:?} can be chosen ({why:?}) in {sent}");
                    }
                }
            }

            let read = opened(session, row(session, "basic"));
            assert!(pick(&read, "Copy Password").is_some(), "{why:?}");
            assert!(pick(&read, "Open Address").is_some(), "{why:?}");
            let shown = opened(
                session,
                json!({ "kind": "field", "entry": basic.id.to_string(), "field": "Password", "shown": false }),
            );
            assert!(matches!(pick(&shown, "Show"), Some(Pick::Show)), "{why:?}");
            let file = opened(
                session,
                json!({ "kind": "file", "entry": files.id.to_string(), "name": attached }),
            );
            assert_eq!(
                matches!(pick(&file, "Save to…"), Some(Pick::Save)),
                saves,
                "{why:?}"
            );
            assert_eq!(
                session
                    .with(|vault| vault.attachment(files.id, &attached).is_ok())
                    .expect("the vault is open"),
                saves,
                "{why:?}: the menu's answer is not the save's"
            );
        }

        // The fixture Coffer reads only has no folder in its bin, so one is
        // put there in a vault Coffer writes, and its menu worked out as for a
        // vault it does not.
        let (_other, writes) = unlocked(RICH);
        let old = binned_folder(&writes, "Old projects");
        let binned = folder(&tree(&writes), &old.to_string(), &[], false)
            .expect("the folder is in the vault");
        assert_eq!(lines(&binned), ["Put Back", "Delete Forever…"]);
        assert!(choices(&binned).iter().all(|(_, pick)| pick.is_none()));
    }

    /// What is in the bin is there to be put back or let go: it is moved
    /// nowhere and copied nowhere new, and letting it go is for good.
    #[test]
    fn an_entry_in_the_bin_is_put_back_or_erased_and_nothing_else() {
        let (_scratch, session) = unlocked(RICH);
        let items = opened(&session, row(&session, "deleted entry"));

        assert_eq!(
            lines(&items),
            [
                "Copy Login",
                "Copy Password",
                "Open Address",
                "-",
                "Put Back",
                "Delete Forever…"
            ]
        );
        assert!(matches!(pick(&items, "Put Back"), Some(Pick::PutBack)));
        assert!(matches!(
            pick(&items, "Delete Forever…"),
            Some(Pick::Delete(deletions)) if deletions.as_slice() == [Deletion::Forever]
        ));
        assert!(matches!(pick(&items, "Copy Password"), Some(Pick::Copy(_))));
    }

    /// Move to draws the places it is sent, in the order sent: a place sent
    /// closed - where the folder is now, the folder itself - is greyed out, a
    /// place with deeper places after it heads a submenu of its own, and a
    /// pick carries its place's id. Which places are closed, and that nothing
    /// under the folder is sent, is the window's rule (`placesFor` in
    /// `places.ts`), sent here as it answers for "level 1"; a place it was
    /// wrong about is refused by the move itself
    /// (`a_place_the_page_lies_about_is_refused_by_the_move`).
    #[test]
    fn move_to_greys_a_place_sent_closed_and_heads_a_submenu_with_its_folder() {
        let (_scratch, session) = unlocked(RICH);
        let level = group_named(&session, "level 1");
        let work = group_named(&session, "Work");
        let mut sent: Vec<Value> = places(&session, work.id)
            .as_array()
            .expect("places are a list")
            .iter()
            .filter(|place| {
                let id = place["id"].as_str().expect("an id");
                id != group_named(&session, "level 2").id.to_string()
                    && id != group_named(&session, "level 3").id.to_string()
            })
            .cloned()
            .collect();
        for place in &mut sent {
            if place["id"] == level.id.to_string() {
                place["open"] = json!(false);
            }
        }

        let items = opened(
            &session,
            json!({ "kind": "folder", "group": level.id.to_string(), "places": sent }),
        );
        assert_eq!(
            lines(&items),
            [
                "New Entry Here",
                "New Folder Inside",
                "-",
                "Rename",
                "Move to",
                "-",
                "Move to Recycle Bin…"
            ]
        );
        let to = inside(&items, "Move to");
        assert!(pick(to, "Top of the vault").is_some());
        let at_work = inside(to, "Work");
        assert!(
            matches!(at_work.first(), Some(Item::Choice { label, .. }) if label == "Work"),
            "the submenu is not headed by its folder"
        );
        assert!(pick(at_work, "Work").is_none(), "where it is now");
        assert!(pick(at_work, "level 1").is_none(), "itself");
        assert!(matches!(
            pick(to, "Personal"),
            Some(Pick::MoveInto(id)) if *id == group_named(&session, "Personal").id.to_string()
        ));
    }

    /// A folder in the bin is put back or let go for good, as an entry there
    /// is: nothing is made in it, it is not renamed, and it is moved nowhere.
    #[test]
    fn a_folder_in_the_bin_is_put_back_or_erased_and_nothing_else() {
        let (_scratch, session) = unlocked(RICH);
        let old = binned_folder(&session, "Old projects").to_string();
        let sent = json!({ "kind": "folder", "group": old, "places": [] });

        let items = opened(&session, sent.clone());
        assert_eq!(lines(&items), ["Put Back", "Delete Forever…"]);
        assert!(matches!(pick(&items, "Put Back"), Some(Pick::PutBack)));
        assert!(matches!(
            pick(&items, "Delete Forever…"),
            Some(Pick::Delete(deletions)) if deletions.as_slice() == [Deletion::Forever]
        ));

        let about = subject(sent);
        let told = |pick: &Pick| {
            serde_json::to_value(chosen(&about, pick).expect("the item names the folder"))
                .expect("it serialises")
        };
        assert_eq!(
            told(&Pick::PutBack),
            json!({ "item": "putBackFolder", "group": old })
        );
        assert_eq!(
            told(&Pick::Delete(vec![Deletion::Forever])),
            json!({ "item": "deleteFolder", "group": old, "deletion": "forever" })
        );
    }

    /// A bin with nothing in it but a folder is not empty.
    #[test]
    fn the_bin_with_only_a_folder_in_it_can_be_emptied() {
        let (_scratch, session) = unlocked(RICH);
        session
            .with_mut(Vault::empty_recycle_bin)
            .expect("the vault is open")
            .expect("the bin empties");
        let empty = opened(&session, json!({ "kind": "bin" }));
        assert!(pick(&empty, "Empty Recycle Bin…").is_none());

        binned_folder(&session, "Old projects");
        let held = bin(&session);
        assert!(held.entries.is_empty() && held.sections.len() == 1);
        let full = opened(&session, json!({ "kind": "bin" }));
        assert!(matches!(
            pick(&full, "Empty Recycle Bin…"),
            Some(Pick::EmptyBin)
        ));
    }

    /// A vault that keeps no bin answers that every deletion is for good, and
    /// every menu says so: a row, a folder, and rows chosen together, none of
    /// them in a bin. No fixture keeps no bin - `vault-core` reads that answer
    /// out of such a vault in its own suite (`batch.rs`) - so the answer is put
    /// on what the rich vault reads, and the menus are worked out from it.
    #[test]
    fn a_vault_without_a_bin_says_every_deletion_is_for_good() {
        fn forever(mut entry: model::Entry) -> model::Entry {
            entry.deletion = model::Deletion::Forever;
            entry
        }
        fn erased(group: &mut Project, id: GroupId) {
            if group.id == id {
                group.deletion = model::Deletion::Forever;
            }
            for section in &mut group.sections {
                erased(section, id);
            }
        }
        fn deletes(items: &[Item], label: &str, count: usize) {
            assert_eq!(lines(items).last().map(String::as_str), Some(label));
            assert!(matches!(
                pick(items, label),
                Some(Pick::Delete(deletions))
                    if deletions.len() == count && deletions.iter().all(|each| *each == Deletion::Forever)
            ));
        }

        let (_scratch, session) = unlocked(RICH);
        let basic = forever(entry_titled(&session, "basic"));
        let fields = forever(entry_titled(&session, "many custom fields"));

        let row = one(&dto::Entry::of(&basic), &[], true);
        assert!(
            lines(&row).contains(&"Move to".to_owned()),
            "a row out of the bin"
        );
        deletes(&row, "Delete Forever…", 1);

        let both = together(&[basic, fields], &[], true);
        assert_eq!(lines(&both)[0], "Move 2 Entries to");
        deletes(&both, "Delete 2 Entries Forever…", 2);

        let work = group_named(&session, "Work").id;
        let mut kept = tree(&session);
        erased(&mut kept, work);
        let made = folder(&kept, &work.to_string(), &[], true).expect("the folder is there");
        assert_eq!(
            lines(&made),
            [
                "New Entry Here",
                "New Folder Inside",
                "-",
                "Rename",
                "Move to",
                "-",
                "Delete Forever…"
            ]
        );
        deletes(&made, "Delete Forever…", 1);
    }

    /// Fifty thousand rows chosen together: the menu is worked out, its
    /// deletion holds one for each in the order they were sent, and both are
    /// done in the time a right-click can wait.
    #[test]
    fn a_selection_of_fifty_thousand_entries_is_offered_and_composed() {
        let (_scratch, session) = unlocked(RICH);
        let personal = group_named(&session, "Personal").id;
        let ids: Vec<String> = session
            .with_mut(|vault| {
                (0..50_000)
                    .map(|_| {
                        vault
                            .create_entry(personal, vault_core::kind::Kind::Login)
                            .map(|id| id.to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .expect("the vault is open")
            .expect("the entries are made");
        let sent =
            json!({ "kind": "entries", "entries": ids, "places": places(&session, personal) });

        let started = std::time::Instant::now();
        let items = opened(&session, sent.clone());
        let Some(Pick::Delete(deletions)) = pick(&items, "Move 50000 Entries to Recycle Bin")
        else {
            panic!("the deletion is not offered: {:?}", lines(&items));
        };
        let told = serde_json::to_value(
            chosen(&subject(sent), &Pick::Delete(deletions.clone()))
                .expect("the deletion names its entries"),
        )
        .expect("it serialises");
        let took = started.elapsed();

        let named: Vec<&str> = told["entries"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|each| each["entry"].as_str().expect("an id"))
            .collect();
        assert_eq!(named, ids);
        assert!(
            took < std::time::Duration::from_secs(5),
            "fifty thousand rows took {took:?}"
        );
    }

    /// A hundred folders, each inside the last: Move to reaches the deepest
    /// through a hundred submenus, each headed by its folder.
    #[test]
    fn a_hundred_folders_deep_is_offered_whole() {
        let (_scratch, session) = unlocked(RICH);
        let mut parent = tree(&session).id;
        let mut chain = Vec::new();
        for level in 0..100 {
            parent = session
                .with_mut(|vault| vault.create_group(parent, &format!("deep {level}")))
                .expect("the vault is open")
                .expect("the folder is made");
            chain.push(parent);
        }

        let items = opened(&session, row(&session, "basic"));
        let mut here = inside(&items, "Move to");
        for level in 0..99 {
            let name = format!("deep {level}");
            let below = inside(here, &name);
            assert!(matches!(below.first(), Some(Item::Choice { label, .. }) if *label == name));
            here = below;
        }
        let deepest = chain.last().expect("a hundred were made").to_string();
        assert!(matches!(pick(here, "deep 99"), Some(Pick::MoveInto(id)) if *id == deepest));
    }

    /// A thousand chosen entries are moved and deleted as one, each with what
    /// deleting it does, and no line copies anything; a choice of one is a
    /// row, and a choice reaching into the bin and out of it can only be
    /// deleted.
    #[test]
    fn a_selection_is_moved_and_deleted_as_one_and_never_copied() {
        let (_scratch, session) = unlocked(RICH);
        let personal = group_named(&session, "Personal").id;
        let made: Vec<EntryId> = (0..1000)
            .map(|_| {
                session
                    .with_mut(|vault| vault.create_entry(personal, vault_core::kind::Kind::Login))
                    .expect("the vault is open")
                    .expect("an entry is made")
            })
            .collect();
        let ids: Vec<String> = made.iter().map(ToString::to_string).collect();
        let sent =
            json!({ "kind": "entries", "entries": ids, "places": places(&session, personal) });

        let items = opened(&session, sent.clone());
        assert_eq!(
            lines(&items),
            [
                "Move 1000 Entries to",
                "-",
                "Move 1000 Entries to Recycle Bin"
            ]
        );
        assert!(
            choices(&items)
                .iter()
                .all(|(_, pick)| !matches!(pick, Some(Pick::Copy(_))))
        );
        let Some(Pick::Delete(deletions)) = pick(&items, "Move 1000 Entries to Recycle Bin") else {
            panic!("the deletion is not offered");
        };
        assert_eq!(deletions.len(), 1000);
        let chosen = chosen(&subject(sent.clone()), &Pick::Delete(deletions.clone()))
            .expect("the deletion names its entries");
        let told = serde_json::to_value(chosen).expect("it serialises");
        assert_eq!(told["entries"].as_array().map(Vec::len), Some(1000));
        assert_eq!(told["entries"][999]["entry"], json!(ids[999]));
        assert_eq!(told["entries"][0]["deletion"], json!("bin"));

        let alone = opened(
            &session,
            json!({ "kind": "entries", "entries": [ids[0]], "places": places(&session, personal) }),
        );
        assert_eq!(
            lines(&alone).first().map(String::as_str),
            Some("Copy Login")
        );

        let deleted = entry_titled(&session, "deleted entry").id.to_string();
        let mixed = opened(
            &session,
            json!({ "kind": "entries", "entries": [ids[0], deleted], "places": [] }),
        );
        assert_eq!(lines(&mixed), ["Delete 2 Entries Forever…"]);
        let binned = opened(
            &session,
            json!({ "kind": "entries", "entries": [deleted, deleted], "places": [] }),
        );
        assert_eq!(
            lines(&binned),
            ["Put Back 2 Entries", "Delete 2 Entries Forever…"]
        );
    }

    /// A menu is drawn only for something the vault has. Each of these is
    /// refused, the way the command behind the button would refuse it, and
    /// nothing panics: a folder's id sent as an entry's, and an entry's as a
    /// folder's, among them.
    #[test]
    fn a_menu_for_something_the_vault_does_not_have_is_refused() {
        let (_scratch, session) = unlocked(RICH);
        let basic = entry_titled(&session, "basic").id.to_string();
        let files = entry_titled(&session, "attachments").id.to_string();
        let nobody = EntryId::from_uuid(uuid::Uuid::from_u128(7)).to_string();
        let megabyte = "x".repeat(1 << 20);

        let work = group_named(&session, "Work").id.to_string();
        let mut refused = vec![
            json!({ "kind": "entry", "entry": nobody, "places": [] }),
            json!({ "kind": "entry", "entry": work, "places": [] }),
            json!({ "kind": "entries", "entries": [basic, work], "places": [] }),
            json!({ "kind": "folder", "group": basic, "places": [] }),
            json!({ "kind": "field", "entry": work, "field": "Password", "shown": false }),
            json!({ "kind": "file", "entry": work, "name": "a" }),
            json!({ "kind": "value", "entry": work, "field": "Password", "range": null }),
            json!({ "kind": "entry", "entry": "", "places": [] }),
            json!({ "kind": "entry", "entry": "../../etc/passwd", "places": [] }),
            json!({ "kind": "entries", "entries": [], "places": [] }),
            json!({ "kind": "entries", "entries": [basic, nobody], "places": [] }),
            json!({ "kind": "entries", "entries": [basic, "not an id"], "places": [] }),
            json!({ "kind": "folder", "group": nobody, "places": [] }),
            json!({ "kind": "folder", "group": "", "places": [] }),
            json!({ "kind": "value", "entry": nobody, "field": "Password", "range": null }),
        ];
        for name in ["", "/", "..", "a\u{0}b", megabyte.as_str(), "url-data "] {
            refused.push(json!({ "kind": "field", "entry": basic, "field": name, "shown": false }));
            refused.push(json!({ "kind": "file", "entry": files, "name": name }));
        }
        for sent in refused {
            match menu(&session, sent.clone()) {
                Ok(items) => panic!("drew {:?} for a {}", lines(&items), sent["kind"]),
                Err(failure) => assert_eq!(code(failure), "noSuchEntry", "a {}", sent["kind"]),
            }
        }

        for wrong in [
            json!({ "kind": "entry" }),
            json!({ "kind": "folder", "group": basic }),
            json!({ "kind": "trash" }),
            json!({ "kind": "field", "entry": basic, "field": "Password", "shown": "yes" }),
            json!({ "kind": "value", "entry": basic, "field": "Password", "range": { "from": -1, "to": 2 } }),
        ] {
            assert!(
                serde_json::from_value::<Subject>(wrong.clone()).is_err(),
                "{wrong}"
            );
        }
    }

    /// A vault that keeps no bin has none to empty.
    #[test]
    fn a_vault_with_no_bin_has_no_bin_menu() {
        let (_scratch, session) = unlocked("minimal-kdbx41.kdbx");
        assert!(groups(&tree(&session)).all(|group| !group.is_recycle_bin));
        let refused = menu(&session, json!({ "kind": "bin" })).err().map(code);
        assert_eq!(refused.as_deref(), Some("noSuchEntry"));
    }

    /// The bin is emptied only when something is in it, after its question.
    #[test]
    fn the_bin_offers_to_be_emptied_only_with_something_in_it() {
        let (_scratch, session) = unlocked(RICH);
        let full = opened(&session, json!({ "kind": "bin" }));
        assert!(matches!(
            pick(&full, "Empty Recycle Bin…"),
            Some(Pick::EmptyBin)
        ));
        let by_folder = opened(
            &session,
            json!({ "kind": "folder", "group": bin(&session).id.to_string(), "places": [] }),
        );
        assert_eq!(lines(&by_folder), ["Empty Recycle Bin…"]);

        session
            .with_mut(Vault::empty_recycle_bin)
            .expect("the vault is open")
            .expect("the bin empties");
        let empty = opened(&session, json!({ "kind": "bin" }));
        assert!(pick(&empty, "Empty Recycle Bin…").is_none());
    }

    /// Every pick a menu offers composes an item about exactly what the menu
    /// was about: the subject's ids, and a move's place. A pick no menu about
    /// a subject offers composes nothing with it.
    #[test]
    fn every_item_names_what_the_menu_was_about() {
        let (_scratch, session) = unlocked(RICH);
        let basic = entry_titled(&session, "basic");
        let fields = entry_titled(&session, "many custom fields");
        let files = entry_titled(&session, "attachments");
        let work = group_named(&session, "Work");
        let id = basic.id.to_string();

        let subjects = [
            row(&session, "basic"),
            row(&session, "deleted entry"),
            json!({ "kind": "entries", "entries": [id, fields.id.to_string()], "places": places(&session, basic.group) }),
            json!({ "kind": "folder", "group": work.id.to_string(), "places": places(&session, work.id) }),
            json!({ "kind": "folder", "group": bin(&session).id.to_string(), "places": [] }),
            json!({ "kind": "bin" }),
            json!({ "kind": "field", "entry": id, "field": "Password", "shown": true }),
            json!({ "kind": "field", "entry": fields.id.to_string(), "field": "custom-003", "shown": false }),
            json!({ "kind": "field", "entry": fields.id.to_string(), "field": "custom-001", "shown": false }),
            json!({ "kind": "file", "entry": files.id.to_string(), "name": "../../escape.txt" }),
            json!({ "kind": "value", "entry": id, "field": "Password", "range": { "from": 2, "to": 5 } }),
        ];
        let mut told = 0;
        for sent in subjects {
            let about = subject(sent.clone());
            for (label, pick) in choices(&opened(&session, sent.clone())) {
                let Some(pick) = pick else { continue };
                let item = chosen(&about, pick)
                    .unwrap_or_else(|| panic!("{label:?} in {sent} names nothing"));
                let item = serde_json::to_value(item).expect("it serialises");
                told += 1;

                for key in ["entry", "group", "name", "range"] {
                    if let Some(value) = item.get(key) {
                        assert_eq!(Some(value), sent.get(key), "{label:?} in {sent}: {key}");
                    }
                }
                if let (Some(value), Some(field)) = (item.get("field"), sent.get("field")) {
                    assert_eq!(value, field, "{label:?} in {sent}");
                }
                if let Some(Value::Array(entries)) = item.get("entries") {
                    let named: Vec<&Value> = entries
                        .iter()
                        .map(|each| each.get("entry").unwrap_or(each))
                        .collect();
                    let asked: Vec<&Value> = match sent.get("entries") {
                        Some(Value::Array(ids)) => ids.iter().collect(),
                        _ => vec![&sent["entry"]],
                    };
                    assert_eq!(named, asked, "{label:?} in {sent}");
                }
                if let Pick::MoveInto(into) = pick {
                    assert_eq!(item["into"], json!(into), "{label:?} in {sent}");
                }
            }
        }
        assert!(told > 30, "only {told} items were read");

        let entry = subject(row(&session, "basic"));
        let value =
            subject(json!({ "kind": "value", "entry": id, "field": "Password", "range": null }));
        let file = subject(json!({ "kind": "file", "entry": id, "name": "a" }));
        let folder =
            subject(json!({ "kind": "folder", "group": work.id.to_string(), "places": [] }));
        let field =
            subject(json!({ "kind": "field", "entry": id, "field": "Password", "shown": false }));
        for (about, pick) in [
            (&Subject::Bin, Pick::Copy("Password".to_owned())),
            (&file, Pick::Show),
            (&value, Pick::Remove),
            (&value, Pick::Copy("Password".to_owned())),
            (&field, Pick::CopyValue),
            (&field, Pick::MoveInto(work.id.to_string())),
            (&entry, Pick::EmptyBin),
            (&entry, Pick::Rename),
            (&entry, Pick::Show),
            (&folder, Pick::Copy("Password".to_owned())),
            (&folder, Pick::Duplicate),
            (&folder, Pick::Delete(Vec::new())),
            (&folder, Pick::Delete(vec![Deletion::Bin, Deletion::Bin])),
            (&entry, Pick::Delete(vec![Deletion::Bin, Deletion::Bin])),
            (&entry, Pick::Delete(Vec::new())),
        ] {
            assert!(chosen(about, &pick).is_none());
        }
    }

    /// The password's row and a field of the reader's own offer what their
    /// buttons do: Show reads Hide while the value is shown, a password that
    /// is not there is set rather than changed, and a field kept in the open
    /// is copied and removed and nothing else.
    #[test]
    fn the_password_and_a_field_of_the_readers_own_offer_what_their_rows_offer() {
        let (_scratch, session) = unlocked(RICH);
        let basic = entry_titled(&session, "basic").id.to_string();
        let fields = entry_titled(&session, "many custom fields").id.to_string();
        let bare = entry_titled(&session, "extreme timestamps").id.to_string();
        let field = |entry: &str, name: &str, shown: bool| {
            opened(
                &session,
                json!({ "kind": "field", "entry": entry, "field": name, "shown": shown }),
            )
        };

        let password = field(&basic, "Password", false);
        assert_eq!(
            lines(&password),
            ["Show", "Copy", "-", "Change…", "Make a New One…"]
        );
        assert!(matches!(pick(&password, "Copy"), Some(Pick::Copy(name)) if name == "Password"));
        assert_eq!(lines(&field(&basic, "Password", true))[0], "Hide");

        // An entry whose file holds no password field at all.
        let none = field(&bare, "Password", false);
        assert_eq!(
            lines(&none),
            ["Show", "Copy", "-", "Set One…", "Make a New One…"]
        );
        assert!(pick(&none, "Show").is_none());
        assert!(pick(&none, "Copy").is_none());
        assert!(matches!(pick(&none, "Set One…"), Some(Pick::Change)));

        let hidden = field(&fields, "custom-003", false);
        assert_eq!(
            lines(&hidden),
            [
                "Show",
                "Copy",
                "-",
                "Change…",
                "Make a New One…",
                "-",
                "Remove"
            ]
        );
        assert!(choices(&hidden).iter().all(|(_, pick)| pick.is_some()));

        let empty = field(&fields, "empty-protected", false);
        assert!(pick(&empty, "Show").is_none());
        assert!(pick(&empty, "Copy").is_none());
        assert!(
            pick(&empty, "Change…").is_none(),
            "typed into where it stands"
        );
        assert!(matches!(
            pick(&empty, "Make a New One…"),
            Some(Pick::MakeOne)
        ));

        let open = field(&fields, "custom-001", false);
        assert_eq!(lines(&open), ["Copy", "-", "Remove"]);
        let spaced = field(&fields, "key with spaces & symbols", false);
        assert!(
            matches!(pick(&spaced, "Copy"), Some(Pick::Copy(name)) if name == "key with spaces & symbols")
        );

        let login = field(&basic, "UserName", false);
        assert_eq!(lines(&login), ["Show", "Copy"]);

        let deleted = entry_titled(&session, "deleted entry").id.to_string();
        let binned = field(&deleted, "Password", false);
        assert!(
            pick(&binned, "Change…").is_none(),
            "nothing in the bin is changed"
        );
        assert!(pick(&binned, "Copy").is_some());
    }

    /// A file is saved from any entry of a vault whose files can be read, and
    /// removed only where the entry may be changed. Its name is the file's
    /// own, `..` and `/` included.
    #[test]
    fn a_file_is_saved_anywhere_and_removed_only_where_the_entry_changes() {
        let (_scratch, session) = unlocked(RICH);
        let files = entry_titled(&session, "attachments").id.to_string();
        for name in ["../../escape.txt", "nested/path/name.txt", "zero-byte.txt"] {
            let items = opened(
                &session,
                json!({ "kind": "file", "entry": files, "name": name }),
            );
            assert_eq!(lines(&items), ["Save to…", "-", "Remove…"]);
            let chose = chosen(
                &subject(json!({ "kind": "file", "entry": files, "name": name })),
                &Pick::RemoveFile,
            )
            .map(|item| serde_json::to_value(item).expect("it serialises"));
            assert_eq!(
                chose,
                Some(json!({ "item": "removeFile", "entry": files, "name": name }))
            );
        }
    }

    /// Whatever the window sends as a place - the bin, a folder inside the one
    /// being moved, an id that is not one, ten thousand of them, a hundred deep
    /// in a flat list, a name of a megabyte - the menu is built and nothing
    /// panics; and a place it was wrong about is refused by the move itself.
    #[test]
    fn a_place_the_page_lies_about_is_refused_by_the_move() {
        let (_scratch, session) = unlocked(RICH);
        let basic = entry_titled(&session, "basic");
        let bin = bin(&session).id;
        let work = group_named(&session, "Work").id;
        let level = group_named(&session, "level 1").id;
        let megabyte = "&".repeat(1 << 20);

        let mut lies = vec![
            json!({ "id": bin.to_string(), "name": "Recycle Bin", "open": true, "depth": 0 }),
            json!({ "id": level.to_string(), "name": "level 1", "open": true, "depth": 7 }),
            json!({ "id": "not an id", "name": megabyte, "open": true, "depth": 0 }),
        ];
        for at in 0..100 {
            lies.push(json!({ "id": format!("deep-{at}"), "name": format!("deep {at}"), "open": true, "depth": at }));
        }
        for at in 0..10_000 {
            lies.push(
                json!({ "id": format!("wide-{at}"), "name": "wide", "open": true, "depth": 0 }),
            );
        }

        let entry = opened(
            &session,
            json!({ "kind": "entry", "entry": basic.id.to_string(), "places": lies }),
        );
        let to = inside(&entry, "Move to");
        assert_eq!(choices(to).len(), 3 + 100 + 10_000);
        let long = choices(to)
            .into_iter()
            .find(|(label, _)| label.len() >= 1 << 20)
            .expect("the long name is a line");
        assert!(long.0 == megabyte, "the name is not drawn whole");
        let folder = opened(
            &session,
            json!({ "kind": "folder", "group": work.to_string(), "places": [
                { "id": level.to_string(), "name": "level 1", "open": true, "depth": 0 }
            ] }),
        );
        assert!(matches!(
            pick(inside(&folder, "Move to"), "level 1"),
            Some(Pick::MoveInto(id)) if *id == level.to_string()
        ));

        let refused = session
            .with_mut(|vault| {
                [
                    vault.move_entries(&[basic.id], bin).is_err(),
                    vault.move_group(work, level).is_err(),
                    dto::group_id("not an id").is_err(),
                ]
            })
            .expect("the vault is open");
        assert_eq!(refused, [true, true, true]);
        assert_eq!(entry_titled(&session, "basic").group, basic.group);
    }

    /// A place deeper than one below the place before it names a folder
    /// whose parent never came: it is taken as one below, and nothing is lost.
    #[test]
    fn a_place_that_skips_a_level_is_taken_one_below_the_last() {
        let sent: Vec<dto::Place> = serde_json::from_value(json!([
            { "id": "a", "name": "a", "open": true, "depth": 0 },
            { "id": "b", "name": "b", "open": true, "depth": 5 },
            { "id": "c", "name": "c", "open": false, "depth": 9 },
            { "id": "d", "name": "d", "open": true, "depth": 0 },
        ]))
        .expect("places read");
        let built = moving(MOVE_TO, &sent, true);
        let Item::Menu { items, enabled, .. } = &built else {
            panic!("Move to is a submenu");
        };
        assert!(enabled);
        assert_eq!(lines(items), ["a", "d"]);
        assert_eq!(lines(inside(items, "a")), ["a", "-", "b"]);
        assert_eq!(lines(inside(inside(items, "a"), "b")), ["b", "-", "c"]);
        assert!(pick(inside(inside(items, "a"), "b"), "c").is_none());

        let Item::Menu { enabled, .. } = moving(MOVE_TO, &sent, false) else {
            panic!("Move to is a submenu");
        };
        assert!(!enabled, "nothing moves in a vault Coffer does not write");
        let Item::Menu { enabled, items, .. } = moving(MOVE_TO, &[], true) else {
            panic!("Move to is a submenu");
        };
        assert!(!enabled && items.is_empty());
    }
}
