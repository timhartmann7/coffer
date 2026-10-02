//! Moving entries and folders between folders: what a move writes into the
//! file, what it refuses, and taking one back.

use keepass::DatabaseKey;
use keepass::db::fields;
use vault_core::model::{Deletion, EntryId, GroupId, Move};
use vault_core::{Rescue, Vault, VaultError};

use crate::bin::{awkward, bin_of};
use crate::support::{
    self, BUILT_PASSWORD, RICH, SECRET, built, cheap, entry_titled, files, folder, holder, library,
    made, only_group, open,
};

/// The entries a move answered it moved, without the folders they left.
fn entries(moved: &[Move]) -> Vec<EntryId> {
    moved.iter().map(|moved| moved.entry).collect()
}

/// The entries a folder holds itself, in the order the file keeps them.
fn order(vault: &Vault, group: GroupId) -> Vec<EntryId> {
    folder(&vault.tree(), group)
        .map(|found| found.entries.iter().map(|row| row.id).collect())
        .unwrap_or_default()
}

/// An entry that was in the vault and has gone out of the file for good, so
/// that its id names nothing.
fn erased(vault: &mut Vault, group: GroupId) -> EntryId {
    let id = made(vault, group, "gone");
    vault
        .delete_entries(&[(id, Deletion::Bin)])
        .expect("it goes to the bin");
    vault
        .delete_entries(&[(id, Deletion::Forever)])
        .expect("and out of the file");
    id
}

/// A folder that was in the vault and has gone out of the file for good.
fn erased_folder(vault: &mut Vault, parent: GroupId) -> GroupId {
    let id = vault
        .create_group(parent, "gone")
        .expect("the folder is made");
    vault
        .delete_group(id, Deletion::Bin)
        .expect("it goes to the bin");
    vault
        .delete_group(id, Deletion::Forever)
        .expect("and out of the file");
    id
}

/// A move is the move KeePass and KeePassXC make when an entry is dragged
/// between folders, and nothing else: no version, the modification time where
/// it was, nothing in `DeletedObjects` - where a record would make every other
/// client delete the thing at its next merge - and the folder left and the
/// moment of the move written down. A folder moves the same way, and what is
/// inside it is not moved at all: it keeps the record of its own last move.
#[test]
fn a_move_is_neither_an_edit_nor_a_deletion() {
    let (_scratch, database) = support::scratch(RICH);
    {
        // Settled once, so the history the test counts is the one a save keeps.
        let mut vault = open(&database, SECRET);
        vault.save().expect("the database saves");
    }
    let before = library(&database, SECRET);

    let mut vault = open(&database, SECRET);
    let root = vault.tree().id;
    let versioned = entry_titled(&vault, "versioned");
    let personal = only_group(&vault, "Personal");
    let work = only_group(&vault, "Work");
    let projects = only_group(&vault, "Versioned");
    let inside = entry_titled(&vault, "attachments").id;
    assert!(versioned.versions > 0, "the fixture entry has history");

    assert_eq!(
        vault
            .move_entries(&[versioned.id], personal)
            .expect("it moves"),
        vec![Move {
            entry: versioned.id,
            from: projects
        }]
    );
    vault.move_group(work, personal).expect("the folder moves");
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, SECRET);
    let after = vault.entry(versioned.id).expect("it is there");
    assert_eq!(after.group, personal);
    assert_eq!(after.versions, versioned.versions, "a move wrote a version");
    assert_eq!(after.times.modified, versioned.times.modified);
    assert_eq!(holder(&vault.tree(), work), Some(personal));

    let written = library(&database, SECRET);
    assert_eq!(
        written.deleted_objects.len(),
        2,
        "the fixture's own two records, and no more"
    );
    let entry = written.entry(versioned.id).expect("the library finds it");
    assert_eq!(
        entry.previous_parent().map(|left| left.id()),
        Some(projects)
    );
    let was = before
        .entry(versioned.id)
        .and_then(|found| found.times.location_changed);
    assert!(
        entry.times.location_changed > was,
        "the entry's move was not dated"
    );
    let moved = written.group(work).expect("the library finds the folder");
    assert_eq!(moved.previous_parent().map(|left| left.id()), Some(root));
    let was = before
        .group(work)
        .and_then(|found| found.times.location_changed);
    assert!(
        moved.times.location_changed > was,
        "the folder's move was not dated"
    );
    assert_eq!(
        Some(moved.times.last_modification),
        before
            .group(work)
            .map(|found| found.times.last_modification),
        "the folder's move was written as a change to it"
    );

    let (was, is) = (
        before.entry(inside).expect("the library finds it"),
        written.entry(inside).expect("the library finds it"),
    );
    assert_eq!(
        is.previous_parent().map(|left| left.id()),
        was.previous_parent().map(|left| left.id()),
        "an entry inside the folder was written as moved"
    );
    assert_eq!(is.times.location_changed, was.times.location_changed);
    assert_eq!(is.times.last_modification, was.times.last_modification);
}

/// What Coffer writes for a move is what KeePassXC reads as one: the entry and
/// the folder are where they were moved to in KeePassXC's own listing, nothing
/// else came or went, and the folder an entry left is in its export.
#[test]
fn keepassxc_finds_what_moved_where_it_went() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let (_scratch, database) = support::scratch(RICH);
    let before = support::entry_paths(&tool, &database, SECRET, None);
    {
        let mut vault = open(&database, SECRET);
        let basic = entry_titled(&vault, "basic").id;
        let work = only_group(&vault, "Work");
        let personal = only_group(&vault, "Personal");
        let deep = only_group(&vault, "level 1");
        vault.move_entries(&[basic], work).expect("it moves");
        vault.move_group(deep, personal).expect("the folder moves");
        vault.save().expect("the database saves");
    }

    let after = support::entry_paths(&tool, &database, SECRET, None);
    assert_eq!(
        after.len(),
        before.len(),
        "a move changed which entries exist"
    );
    for (gone, there) in [
        ("Personal/basic", "Work/basic"),
        (
            "Work/level 1/level 2/level 3/extreme timestamps",
            "Personal/level 1/level 2/level 3/extreme timestamps",
        ),
    ] {
        assert!(before.iter().any(|path| path == gone), "{before:?}");
        assert!(!after.iter().any(|path| path == gone), "{after:?}");
        assert!(after.iter().any(|path| path == there), "{after:?}");
    }

    let exported = support::export(&tool, &database, SECRET, None);
    let at = exported
        .find("<UUID>ZS1iYXNpYy0tLS0tLS0tLQ==</UUID>")
        .expect("the entry is in the export");
    let rest = exported.get(at..).unwrap_or_default();
    let entry = rest.get(..rest.find("</Entry>").unwrap_or(rest.len()));
    assert!(
        entry.is_some_and(|entry| {
            entry.contains("<PreviousParentGroup>cGVyc29uYWwtLS0tLS0tLQ==</PreviousParentGroup>")
        }),
        "KeePassXC does not see which folder the entry left"
    );
}

/// The library takes a thing it is asked to move to the folder it is already
/// in out of its place, puts it at the end, and writes that folder as the one
/// it came from. Coffer leaves it alone: nothing moves, nothing is written,
/// and there is nothing to save.
#[test]
fn a_move_to_where_it_already_is_changes_nothing() {
    let (_scratch, path) = cheap("still.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let personal = vault
        .create_group(root, "Personal")
        .expect("the folder is made");
    let inner = vault
        .create_group(personal, "Inner")
        .expect("the folder is made");
    let first = made(&mut vault, personal, "first");
    let second = made(&mut vault, personal, "second");
    let third = made(&mut vault, personal, "third");
    vault.save().expect("the database saves");
    let edits = vault.edits();

    assert_eq!(
        vault
            .move_entries(&[first, first, second], personal)
            .expect("there is nothing to do"),
        Vec::new()
    );
    vault
        .move_group(inner, personal)
        .expect("there is nothing to do");
    vault
        .move_group(personal, root)
        .expect("there is nothing to do");
    vault
        .move_entries(&[], root)
        .expect("there is nothing to do");

    assert_eq!(
        vault.edits(),
        edits,
        "a move that found nothing to do moved the revision"
    );
    assert_eq!(
        order(&vault, personal),
        vec![first, second, third],
        "an entry was taken to the end of its own folder"
    );
    assert_eq!(
        vault.rescue(),
        Rescue::Nothing,
        "a move that found nothing to do left something to save"
    );

    vault.save().expect("the database saves");
    let written = library(&path, BUILT_PASSWORD);
    assert!(
        written
            .entry(first)
            .is_some_and(|entry| entry.previous_parent().is_none()),
        "the folder an entry is in was written as the one it came from"
    );
    assert!(
        written
            .group(inner)
            .is_some_and(|group| group.previous_parent().is_none()),
        "the folder a folder is in was written as the one it came from"
    );
}

/// The bin is closed at both ends. What is in it - an entry, an entry inside a
/// deleted folder, the deleted folder, the bin itself - comes out by being put
/// back, and nothing goes in but by being deleted. Each refusal moves nothing,
/// and leaves nothing to save.
#[test]
fn nothing_moves_into_or_out_of_the_bin() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = awkward(scratch.path(), "awkward.kdbx", true);
    let mut vault = open(&path, BUILT_PASSWORD);
    let tree = vault.tree();
    let edits = vault.edits();

    let binned = entry_titled(&vault, "binned").id;
    let inside = entry_titled(&vault, "inside").id;
    let bank = entry_titled(&vault, "bank").id;
    let bin = bin_of(&tree).id;
    let deleted = only_group(&vault, "Binned folder");
    let personal = only_group(&vault, "Personal");

    let refused = [
        (
            "an entry in the bin",
            vault.move_entries(&[binned], personal).map(drop),
            "in",
        ),
        (
            "an entry in a deleted folder",
            vault.move_entries(&[inside], personal).map(drop),
            "in",
        ),
        (
            "a batch with one entry from the bin in it",
            vault.move_entries(&[bank, binned], personal).map(drop),
            "in",
        ),
        (
            "a deleted folder",
            vault.move_group(deleted, personal),
            "in",
        ),
        ("the bin itself", vault.move_group(bin, personal), "in"),
        (
            "an entry into the bin",
            vault.move_entries(&[bank], bin).map(drop),
            "into",
        ),
        (
            "an entry into a deleted folder",
            vault.move_entries(&[bank], deleted).map(drop),
            "into",
        ),
        (
            "a folder into the bin",
            vault.move_group(personal, bin),
            "into",
        ),
        (
            "a folder into a deleted folder",
            vault.move_group(personal, deleted),
            "into",
        ),
    ];
    for (what, answer, expected) in refused {
        match (answer, expected) {
            (Err(VaultError::InRecycleBin), "in") | (Err(VaultError::IntoRecycleBin), "into") => {}
            (other, _) => panic!("{what} was answered {other:?}"),
        }
    }

    assert_eq!(vault.tree(), tree, "a refused move moved something");
    assert_eq!(vault.edits(), edits);
    assert_eq!(vault.rescue(), Rescue::Nothing);
}

/// A folder the bin sits inside is a folder like any other to move, and the bin
/// goes with it and is still the bin: a deletion still lands in it. The rule
/// that such a folder is erased rather than binned follows it to where it went.
#[test]
fn a_folder_the_bin_is_in_moves_and_the_bin_stays_the_bin() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = awkward(scratch.path(), "awkward.kdbx", true);
    let mut vault = open(&path, BUILT_PASSWORD);
    let archive = only_group(&vault, "Archive");
    let personal = only_group(&vault, "Personal");
    let bin = bin_of(&vault.tree()).id;
    let top = entry_titled(&vault, "top").id;

    vault
        .move_group(archive, personal)
        .expect("the folder moves");
    let tree = vault.tree();
    assert_eq!(bin_of(&tree).id, bin, "the bin is not the bin any more");
    assert_eq!(holder(&tree, bin), Some(archive));
    assert_eq!(holder(&tree, archive), Some(personal));
    assert_eq!(
        folder(&tree, personal).map(|found| found.deletion),
        Some(Deletion::Forever),
        "a folder the bin is now inside is offered the bin"
    );

    vault
        .delete_entries(&[(top, Deletion::Bin)])
        .expect("it goes to the bin");
    assert_eq!(vault.entry(top).map(|entry| entry.group), Some(bin));
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&path, BUILT_PASSWORD);
    assert_eq!(bin_of(&vault.tree()).id, bin);
}

/// A folder cannot be put inside itself, however far down the folder it is
/// aimed at sits, and the top of the vault goes nowhere. A hundred levels are
/// walked without taking anything down, and a refusal moves nothing.
#[test]
fn a_folder_cannot_go_inside_itself_however_deep() {
    let (_scratch, path) = cheap("deep.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let mut chain = Vec::new();
    let mut here = root;
    for depth in 0..100 {
        here = vault
            .create_group(here, &format!("level {depth}"))
            .expect("the folder is made");
        chain.push(here);
    }
    let top = *chain.first().expect("a hundred folders");
    let next = *chain.get(1).expect("a hundred folders");
    let bottom = *chain.last().expect("a hundred folders");
    let tree = vault.tree();
    let edits = vault.edits();

    for into in [top, next, bottom] {
        assert!(matches!(
            vault.move_group(top, into),
            Err(VaultError::CannotMoveIntoItself)
        ));
    }
    for into in [root, top, bottom] {
        assert!(matches!(
            vault.move_group(root, into),
            Err(VaultError::CannotMoveRoot)
        ));
    }
    assert_eq!(vault.tree(), tree);
    assert_eq!(vault.edits(), edits);

    vault
        .move_group(bottom, root)
        .expect("the deepest folder goes to the top");
    let tree = vault.tree();
    assert_eq!(holder(&tree, bottom), Some(root));
    let mut depth = 0;
    let mut walking = folder(&tree, top);
    while let Some(found) = walking {
        depth += 1;
        walking = found.sections.first();
    }
    assert_eq!(depth, 99, "the chain it left is one shorter");
    vault.save().expect("the database saves");
}

/// A batch is checked whole before anything in it moves. One entry this would
/// refuse - in the bin, gone from the file - or a folder that is not there
/// refuses the lot, and the rest stay exactly where they were. What does move
/// goes in the order given, and once however often it is named.
#[test]
fn a_batch_moves_all_or_nothing() {
    let (_scratch, path) = cheap("batch.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let personal = vault
        .create_group(root, "Personal")
        .expect("the folder is made");
    let work = vault
        .create_group(root, "Work")
        .expect("the folder is made");
    let a = made(&mut vault, personal, "a");
    let b = made(&mut vault, personal, "b");
    let binned = made(&mut vault, personal, "binned");
    vault
        .delete_entries(&[(binned, Deletion::Bin)])
        .expect("it goes to the bin");
    let gone = erased(&mut vault, personal);
    let nowhere = erased_folder(&mut vault, root);
    let edits = vault.edits();

    assert!(matches!(
        vault.move_entries(&[a, b, binned], work),
        Err(VaultError::InRecycleBin)
    ));
    assert!(matches!(
        vault.move_entries(&[a, gone], work),
        Err(VaultError::NoSuchEntry)
    ));
    assert!(matches!(
        vault.move_entries(&[a, b], nowhere),
        Err(VaultError::NoSuchGroup)
    ));
    assert_eq!(
        order(&vault, personal),
        vec![a, b],
        "a refused batch moved some of it"
    );
    assert_eq!(vault.edits(), edits, "a refused batch moved the revision");

    assert_eq!(
        entries(&vault.move_entries(&[a, a, b], work).expect("they move")),
        vec![a, b]
    );
    assert_eq!(order(&vault, work), vec![a, b]);
    assert!(order(&vault, personal).is_empty());

    assert_eq!(
        entries(&vault.move_entries(&[b, a], personal).expect("they move")),
        vec![b, a]
    );
    assert_eq!(
        order(&vault, personal),
        vec![b, a],
        "not in the order given"
    );
}

/// A select-all across folders sent to one of them holds entries already
/// there. Those stay in their places, and only the others are answered - the
/// answer is what an undo takes back, and an entry in it that never moved
/// would be sent to whatever folder it last came from, which the reader never
/// moved it out of.
#[test]
fn a_batch_answers_only_what_changed_folder_and_leaves_the_rest_in_place() {
    let (_scratch, path) = cheap("mixed.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let personal = vault
        .create_group(root, "Personal")
        .expect("the folder is made");
    let work = vault
        .create_group(root, "Work")
        .expect("the folder is made");
    let x = made(&mut vault, personal, "x");
    let y = made(&mut vault, work, "y");
    let a = made(&mut vault, personal, "a");
    vault.move_entries(&[x], work).expect("it moves");
    vault.save().expect("the database saves");
    let settled = library(&path, BUILT_PASSWORD);
    let was = settled.entry(x).expect("the library finds it");
    let (left, when) = (
        was.previous_parent().map(|group| group.id()),
        was.times.location_changed,
    );
    assert_eq!(order(&vault, work), vec![y, x]);

    let moved = vault.move_entries(&[x, a], work).expect("it moves");
    assert_eq!(
        moved,
        vec![Move {
            entry: a,
            from: personal
        }]
    );
    assert_eq!(order(&vault, work), vec![y, x, a]);
    vault.save().expect("the database saves");
    let written = library(&path, BUILT_PASSWORD);
    let stayed = written.entry(x).expect("the library finds it");
    assert_eq!(stayed.previous_parent().map(|group| group.id()), left);
    assert_eq!(stayed.times.location_changed, when);

    vault
        .move_entries_back(&moved, work)
        .expect("the move is taken back");
    assert_eq!(order(&vault, work), vec![y, x]);
    assert_eq!(order(&vault, personal), vec![a]);
}

/// A database Coffer will not write back moves nothing, whichever way it is
/// asked.
#[test]
fn a_read_only_vault_moves_nothing() {
    let (_scratch, database) = support::scratch(RICH);
    {
        let mut vault = open(&database, SECRET);
        vault.save().expect("the database saves");
    }
    let snapshot = vault_core::storage::snapshot::slot(&database, 1).expect("a slot has a name");
    let mut vault = open(&snapshot, SECRET);
    let basic = entry_titled(&vault, "basic").id;
    let personal = only_group(&vault, "Personal");
    let work = only_group(&vault, "Work");
    let tree = vault.tree();

    assert!(matches!(
        vault.move_entries(&[basic], work),
        Err(VaultError::ReadOnlySnapshot)
    ));
    assert!(matches!(
        vault.move_group(work, personal),
        Err(VaultError::ReadOnlySnapshot)
    ));
    let back = Move {
        entry: basic,
        from: work,
    };
    assert!(matches!(
        vault.move_entries_back(&[back], personal),
        Err(VaultError::ReadOnlySnapshot)
    ));
    assert!(matches!(
        vault.move_group_back(work, personal, vault.tree().id),
        Err(VaultError::ReadOnlySnapshot)
    ));
    assert_eq!(vault.tree(), tree);
}

/// Everything an entry has goes with it: thirty custom fields, files holding
/// every byte value, an SSH key and the blob KeeAgent reads it through, its
/// history, and dates in the years 1600 and 3000. Read back from the file,
/// each is the entry it was in another folder, and every file in the vault is
/// byte for byte what it was.
#[test]
fn a_moved_entry_takes_everything_it_has_along() {
    let (_scratch, database) = support::scratch(RICH);
    {
        // Settled once, so the history compared is the one a save keeps.
        let mut vault = open(&database, SECRET);
        vault.save().expect("the database saves");
    }
    let mut vault = open(&database, SECRET);
    let personal = only_group(&vault, "Personal");
    let moving: Vec<_> = [
        "attachments",
        "many custom fields",
        "ssh key",
        "versioned",
        "extreme timestamps",
    ]
    .into_iter()
    .map(|title| entry_titled(&vault, title))
    .collect();
    let held = files(&vault);
    let ids: Vec<EntryId> = moving.iter().map(|entry| entry.id).collect();

    assert_eq!(
        entries(&vault.move_entries(&ids, personal).expect("they move")),
        ids
    );
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, SECRET);
    for before in moving {
        let after = vault.entry(before.id).expect("it is there");
        assert_eq!(
            after,
            vault_core::model::Entry {
                group: personal,
                ..before
            }
        );
    }
    assert_eq!(files(&vault), held, "a file changed on the way");
}

/// A folder moved and moved back is where it was, with everything that was in
/// it, however deep.
#[test]
fn a_folder_moved_and_moved_back_is_whole() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let root = vault.tree().id;
    let work = only_group(&vault, "Work");
    let personal = only_group(&vault, "Personal");
    let inside = folder(&vault.tree(), work).cloned();

    vault.move_group(work, personal).expect("the folder moves");
    let tree = vault.tree();
    assert_eq!(
        holder(&tree, work),
        Some(personal),
        "the folder did not move"
    );
    assert_eq!(folder(&tree, work).cloned(), inside);
    vault.move_group(work, root).expect("and back");
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, SECRET);
    let tree = vault.tree();
    assert_eq!(holder(&tree, work), Some(root));
    assert_eq!(folder(&tree, work).cloned(), inside);
}

/// Fifty thousand entries leave one folder in one batch, which is what a
/// select-all of a large vault asks for, and come back in another. They arrive
/// in their order each time, and the file still saves. The cost is in
/// `docs/vault-core.md`; this is that it ends.
#[test]
fn fifty_thousand_entries_move_out_of_one_folder_and_back() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "many.kdbx", |database| {
        database.root_mut().add_group().name = "Archive".to_owned();
        for index in 0..50_000 {
            database
                .root_mut()
                .add_entry()
                .edit(|entry| entry.set_unprotected(fields::TITLE, format!("entry {index}")));
        }
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let archive = only_group(&vault, "Archive");
    let ids = order(&vault, root);
    assert_eq!(ids.len(), 50_000);

    let moved = vault.move_entries(&ids, archive).expect("they move");
    assert_eq!(entries(&moved), ids);
    assert!(order(&vault, root).is_empty());
    assert_eq!(order(&vault, archive), ids, "they did not arrive in order");

    vault
        .move_entries_back(&moved, archive)
        .expect("they come back");
    assert!(order(&vault, archive).is_empty());
    assert_eq!(order(&vault, root), ids, "they did not come back in order");
    vault.save().expect("the database saves");
    drop(vault);

    assert_eq!(open(&path, BUILT_PASSWORD).tree().entries.len(), 50_000);
}

/// Where a deleted thing goes back to is where it was when it was deleted,
/// moves included: an entry moved and then deleted goes back to the folder it
/// was moved into, and so does what was inside a folder moved and then
/// deleted.
#[test]
fn what_was_moved_and_then_deleted_goes_back_where_it_was_moved_to() {
    let (_scratch, path) = cheap("moved.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let personal = vault
        .create_group(root, "Personal")
        .expect("the folder is made");
    let work = vault
        .create_group(root, "Work")
        .expect("the folder is made");
    let cards = vault
        .create_group(personal, "Cards")
        .expect("the folder is made");
    let bank = made(&mut vault, personal, "Bank");
    let visa = made(&mut vault, cards, "Visa");

    vault.move_entries(&[bank], work).expect("it moves");
    vault
        .delete_entries(&[(bank, Deletion::Bin)])
        .expect("it goes to the bin");
    vault.move_group(cards, work).expect("the folder moves");
    vault
        .delete_group(cards, Deletion::Bin)
        .expect("it goes to the bin");
    vault.save().expect("the database saves");
    drop(vault);

    let mut vault = open(&path, BUILT_PASSWORD);
    assert_eq!(
        vault
            .entry(bank)
            .and_then(|entry| entry.binned)
            .and_then(|binned| binned.from),
        Some(work)
    );
    let went = vault
        .entry(visa)
        .and_then(|entry| entry.binned)
        .expect("it is in the bin");
    assert_eq!((went.within, went.from), (Some(cards), Some(work)));

    vault.put_back_entries(&[bank]).expect("it is put back");
    vault.put_back_entries(&[visa]).expect("it is put back");
    assert_eq!(vault.entry(bank).map(|entry| entry.group), Some(work));
    assert_eq!(vault.entry(visa).map(|entry| entry.group), Some(work));
}

/// The undo of a move. Entries from three folders went into one, and each goes
/// back into the folder it came from, read off what the move wrote - still no
/// version, still nothing in `DeletedObjects`.
#[test]
fn a_move_is_taken_back_to_where_each_entry_came_from() {
    let (_scratch, path) = cheap("back.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let personal = vault
        .create_group(root, "Personal")
        .expect("the folder is made");
    let work = vault
        .create_group(root, "Work")
        .expect("the folder is made");
    let banking = vault
        .create_group(root, "Banking")
        .expect("the folder is made");
    let mine = made(&mut vault, personal, "mine");
    let theirs = made(&mut vault, work, "theirs");
    let loose = made(&mut vault, root, "loose");
    let versions = vault.entry(mine).map(|entry| entry.versions);

    let moved = vault
        .move_entries(&[mine, theirs, loose], banking)
        .expect("they move");
    vault
        .move_entries_back(&moved, banking)
        .expect("they go back");

    for (id, home) in [(mine, personal), (theirs, work), (loose, root)] {
        assert_eq!(vault.entry(id).map(|entry| entry.group), Some(home));
    }
    assert!(order(&vault, banking).is_empty());
    assert_eq!(vault.entry(mine).map(|entry| entry.versions), versions);
    vault.save().expect("the database saves");
    assert!(library(&path, BUILT_PASSWORD).deleted_objects.is_empty());
}

/// An undo pressed after one of the entries moved on again would take back
/// the later move as well, so it is refused whole and moves nothing. The undo
/// of the later move is still the later move's.
#[test]
fn a_move_taken_back_after_one_of_them_moved_again_moves_nothing() {
    let (_scratch, path) = cheap("again.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let personal = vault
        .create_group(root, "Personal")
        .expect("the folder is made");
    let work = vault
        .create_group(root, "Work")
        .expect("the folder is made");
    let banking = vault
        .create_group(root, "Banking")
        .expect("the folder is made");
    let a = made(&mut vault, personal, "a");
    let b = made(&mut vault, personal, "b");

    let first = vault.move_entries(&[a, b], banking).expect("they move");
    let later = vault.move_entries(&[b], work).expect("it moves on");
    let edits = vault.edits();

    assert!(matches!(
        vault.move_entries_back(&first, banking),
        Err(VaultError::MoveSuperseded)
    ));
    assert_eq!(vault.entry(a).map(|entry| entry.group), Some(banking));
    assert_eq!(vault.entry(b).map(|entry| entry.group), Some(work));
    assert_eq!(vault.edits(), edits);

    vault
        .move_entries_back(&later, work)
        .expect("the later move is still its own to take back");
    assert_eq!(vault.entry(b).map(|entry| entry.group), Some(banking));
}

/// The undo goes by what the file says of the move. An entry moved on and
/// brought back by way of another folder is in the folder the move took it
/// to, and the file says it last came there from that other folder: the move
/// to take back is no longer the last that happened to it, and taking it back
/// would send the entry somewhere the reader never took it from. Refused whole,
/// whether the trip was the reader's own or another client's.
#[test]
fn a_move_taken_back_after_the_entry_went_away_and_came_back_is_refused() {
    let (_scratch, path) = cheap("trip.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let personal = vault
        .create_group(root, "Personal")
        .expect("the folder is made");
    let work = vault
        .create_group(root, "Work")
        .expect("the folder is made");
    let banking = vault
        .create_group(root, "Banking")
        .expect("the folder is made");
    let a = made(&mut vault, personal, "a");
    let b = made(&mut vault, personal, "b");

    let first = vault.move_entries(&[a, b], banking).expect("they move");
    let later = vault.move_entries(&[b], work).expect("it moves on");
    vault
        .move_entries_back(&later, work)
        .expect("the later move is taken back");
    let edits = vault.edits();
    assert!(matches!(
        vault.move_entries_back(&first, banking),
        Err(VaultError::MoveSuperseded)
    ));
    assert_eq!(order(&vault, banking), vec![a, b], "something moved");
    assert_eq!(vault.edits(), edits);

    // Another client's trip, read back by a reload. The library's own move
    // writes no `LocationChanged`, so the folder left is all that tells.
    let mine: Vec<Move> = first.into_iter().filter(|moved| moved.entry == a).collect();
    vault.save().expect("the database saves");
    {
        let key = || DatabaseKey::new().with_password(BUILT_PASSWORD);
        let mut other = library(&path, BUILT_PASSWORD);
        for into in [work, banking] {
            other
                .entry_mut(a)
                .expect("the other client finds it")
                .move_to(into)
                .expect("the folder is there");
        }
        let mut file = std::fs::File::create(&path).expect("the file is rewritten");
        other
            .save(&mut file, key())
            .expect("the other client saves");
    }
    vault.reload().expect("the file is read again");
    assert!(matches!(
        vault.move_entries_back(&mine, banking),
        Err(VaultError::MoveSuperseded)
    ));
    assert_eq!(vault.entry(a).map(|entry| entry.group), Some(banking));
}

/// An entry that has gone out of the file since the move is something done
/// after it, like a move on: the undo is refused whole, and the entry still
/// there stays where the move put it.
#[test]
fn a_move_taken_back_after_one_entry_was_erased_moves_nothing() {
    let (_scratch, path) = cheap("erased.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let personal = vault
        .create_group(root, "Personal")
        .expect("the folder is made");
    let banking = vault
        .create_group(root, "Banking")
        .expect("the folder is made");
    let a = made(&mut vault, personal, "a");
    let b = made(&mut vault, personal, "b");

    let moved = vault.move_entries(&[a, b], banking).expect("they move");
    vault
        .delete_entries(&[(b, Deletion::Bin)])
        .expect("it goes to the bin");
    vault
        .delete_entries(&[(b, Deletion::Forever)])
        .expect("and out of the file");
    let edits = vault.edits();

    let answer = vault.move_entries_back(&moved, banking);
    assert!(
        matches!(answer, Err(VaultError::MoveSuperseded)),
        "{answer:?}"
    );
    assert_eq!(order(&vault, banking), vec![a]);
    assert!(order(&vault, personal).is_empty());
    assert_eq!(vault.edits(), edits);
}

/// The other client that matters most moves the entry on. KeePassXC takes it
/// away and back again between Coffer's move and its undo, and writes the
/// folder it last came from as it does: the reload reads that in, and the
/// undo is refused rather than sending the entry to where it was two moves
/// ago.
#[test]
fn a_move_keepassxc_made_since_cannot_be_taken_back() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let basic = entry_titled(&vault, "basic").id;
    let personal = only_group(&vault, "Personal");
    let work = only_group(&vault, "Work");
    let moved = vault.move_entries(&[basic], work).expect("it moves");
    vault.save().expect("the database saves");

    let path = database.to_string_lossy().into_owned();
    for (entry, into) in [("Work/basic", "Versioned"), ("Versioned/basic", "Work")] {
        support::cli(&tool, SECRET, None, &["mv", "-q", &path, entry, into])
            .unwrap_or_else(|error| panic!("keepassxc-cli could not move {entry}: {error}"));
    }
    vault.reload().expect("the file is read again");
    assert_eq!(vault.entry(basic).map(|entry| entry.group), Some(work));

    let answer = vault.move_entries_back(&moved, work);
    assert!(
        matches!(answer, Err(VaultError::MoveSuperseded)),
        "{answer:?}"
    );
    assert_eq!(vault.entry(basic).map(|entry| entry.group), Some(work));
    assert_ne!(vault.entry(basic).map(|entry| entry.group), Some(personal));
}

/// An undo that names an entry twice takes it back once. A second move would
/// be a move to the folder it is already in, which the library answers by
/// taking it to the end of that folder and writing the folder as the one it
/// came from.
#[test]
fn a_move_taken_back_naming_an_entry_twice_moves_it_once() {
    let (_scratch, path) = cheap("twice.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let personal = vault
        .create_group(root, "Personal")
        .expect("the folder is made");
    let banking = vault
        .create_group(root, "Banking")
        .expect("the folder is made");
    let a = made(&mut vault, personal, "a");
    let b = made(&mut vault, personal, "b");

    let moved = vault.move_entries(&[a], banking).expect("it moves");
    let twice: Vec<Move> = moved.iter().chain(moved.iter()).copied().collect();
    vault
        .move_entries_back(&twice, banking)
        .expect("the move is taken back");
    assert_eq!(order(&vault, personal), vec![b, a]);
    vault.save().expect("the database saves");

    let written = library(&path, BUILT_PASSWORD);
    assert_eq!(
        written
            .entry(a)
            .and_then(|entry| entry.previous_parent().map(|left| left.id())),
        Some(banking),
        "the entry was moved again into the folder it was in"
    );
}

/// A folder that is not there neither moves nor takes one in, and the refusal
/// changes nothing.
#[test]
fn a_folder_that_is_not_there_neither_moves_nor_takes_one_in() {
    let (_scratch, path) = cheap("missing.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let work = vault
        .create_group(root, "Work")
        .expect("the folder is made");
    let gone = erased_folder(&mut vault, root);
    let tree = vault.tree();
    let edits = vault.edits();

    assert!(matches!(
        vault.move_group(gone, root),
        Err(VaultError::NoSuchGroup)
    ));
    assert!(matches!(
        vault.move_group(work, gone),
        Err(VaultError::NoSuchGroup)
    ));
    assert_eq!(vault.tree(), tree);
    assert_eq!(vault.edits(), edits);
}

/// Taking a move back puts things into folders, and the bin is closed to that
/// as it is to a move. The folder they were moved into went to the bin, the
/// folder one came from went there, or out of the file: each is refused whole,
/// and nothing moves.
#[test]
fn a_move_taken_back_out_of_or_into_a_binned_folder_moves_nothing() {
    type Since = fn(&mut Vault, GroupId, GroupId);
    let ways: [(&str, Since); 4] = [
        (
            "the folder they went into is in the bin",
            |vault, _, into| {
                vault
                    .delete_group(into, Deletion::Bin)
                    .expect("it goes to the bin");
            },
        ),
        (
            "the folder they came from is in the bin",
            |vault, from, _| {
                vault
                    .delete_group(from, Deletion::Bin)
                    .expect("it goes to the bin");
            },
        ),
        ("the folder they came from is gone", |vault, from, _| {
            vault
                .delete_group(from, Deletion::Bin)
                .expect("it goes to the bin");
            vault
                .delete_group(from, Deletion::Forever)
                .expect("and out of the file");
        }),
        ("the folder they went into is gone", |vault, _, into| {
            vault
                .delete_group(into, Deletion::Bin)
                .expect("it goes to the bin");
            vault
                .delete_group(into, Deletion::Forever)
                .expect("and out of the file");
        }),
    ];

    for (what, since) in ways {
        let (_scratch, path) = cheap("binned.kdbx");
        let mut vault = open(&path, BUILT_PASSWORD);
        let root = vault.tree().id;
        let personal = vault
            .create_group(root, "Personal")
            .expect("the folder is made");
        let banking = vault
            .create_group(root, "Banking")
            .expect("the folder is made");
        let a = made(&mut vault, personal, "a");
        let moved = vault.move_entries(&[a], banking).expect("it moves");
        since(&mut vault, personal, banking);
        let tree = vault.tree();
        let edits = vault.edits();

        let answer = vault.move_entries_back(&moved, banking);
        assert!(
            matches!(answer, Err(VaultError::MoveSuperseded)),
            "{what}: {answer:?}"
        );
        assert_eq!(vault.tree(), tree, "{what}: something moved");
        assert_eq!(vault.edits(), edits, "{what}");
    }
}

/// The vault an undo was offered over is not always the vault it reaches. A
/// reload brings in the file, where the move was never saved, or where another
/// client moved the entry on since: either way the move to take back is not
/// what happened last, and the undo is refused.
#[test]
fn a_move_taken_back_after_a_reload_is_refused() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let mut ids = None;
    let path = built(scratch.path(), "reloaded.kdbx", |database| {
        let mut root = database.root_mut();
        let mut add = |name: &str| {
            let mut group = root.add_group();
            group.name = name.to_owned();
            group.id()
        };
        let (personal, work, banking) = (add("Personal"), add("Work"), add("Banking"));
        let entry = database
            .group_mut(personal)
            .expect("the folder is there")
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "Chase"))
            .id();
        ids = Some((personal, work, banking, entry));
    });
    let (personal, work, banking, chase) = ids.expect("the vault is built");

    let mut vault = open(&path, BUILT_PASSWORD);
    let moved = vault.move_entries(&[chase], banking).expect("it moves");
    vault.reload().expect("the file is read again");
    assert!(matches!(
        vault.move_entries_back(&moved, banking),
        Err(VaultError::MoveSuperseded)
    ));
    assert_eq!(vault.entry(chase).map(|entry| entry.group), Some(personal));

    let moved = vault.move_entries(&[chase], banking).expect("it moves");
    vault.save().expect("the database saves");
    {
        let key = || DatabaseKey::new().with_password(BUILT_PASSWORD);
        let mut other = library(&path, BUILT_PASSWORD);
        other
            .entry_mut(chase)
            .expect("the other client finds it")
            .move_to(work)
            .expect("the folder is there");
        let mut file = std::fs::File::create(&path).expect("the file is rewritten");
        other
            .save(&mut file, key())
            .expect("the other client saves");
    }
    vault.reload().expect("the file is read again");
    assert!(matches!(
        vault.move_entries_back(&moved, banking),
        Err(VaultError::MoveSuperseded)
    ));
    assert_eq!(vault.entry(chase).map(|entry| entry.group), Some(work));
}

/// Another client that moved an entry to the folder it was already in wrote
/// that folder as the one it came from. There is nothing to take back to, and
/// the undo does not pretend there is.
#[test]
fn an_entry_whose_last_move_went_nowhere_is_not_taken_back_into_its_own_folder() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let mut ids = None;
    let path = built(scratch.path(), "nowhere.kdbx", |database| {
        let personal = {
            let mut root = database.root_mut();
            let mut group = root.add_group();
            group.name = "Personal".to_owned();
            group.id()
        };
        let mut group = database.group_mut(personal).expect("the folder is there");
        let mut entry = group.add_entry();
        entry.edit(|entry| entry.set_unprotected(fields::TITLE, "Chase"));
        entry.move_to(personal).expect("the folder is there");
        ids = Some((personal, entry.id()));
    });
    let (personal, chase) = ids.expect("the vault is built");

    let mut vault = open(&path, BUILT_PASSWORD);
    let claimed = Move {
        entry: chase,
        from: personal,
    };
    assert!(matches!(
        vault.move_entries_back(&[claimed], personal),
        Err(VaultError::MoveSuperseded)
    ));
    assert_eq!(vault.entry(chase).map(|entry| entry.group), Some(personal));
    assert_eq!(vault.rescue(), Rescue::Nothing);
}

/// The undo of a folder's move: it goes back to the folder it left, at the
/// end of it, with everything inside it, and that is still a move and not an
/// edit - nothing in `DeletedObjects`, and the folder written as having left
/// the one it was moved into.
#[test]
fn a_folder_move_is_taken_back_to_the_folder_it_left() {
    let (_scratch, path) = cheap("folder back.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let personal = vault
        .create_group(root, "Personal")
        .expect("the folder is made");
    let banking = vault
        .create_group(root, "Banking")
        .expect("the folder is made");
    let cards = vault
        .create_group(personal, "Cards")
        .expect("the folder is made");
    let notes = vault
        .create_group(personal, "Notes")
        .expect("the folder is made");
    let visa = made(&mut vault, cards, "Visa");
    let inside = folder(&vault.tree(), cards).cloned();

    vault.move_group(cards, banking).expect("the folder moves");
    vault
        .move_group_back(cards, personal, banking)
        .expect("and is taken back");

    let tree = vault.tree();
    assert_eq!(holder(&tree, cards), Some(personal));
    let order: Vec<GroupId> = folder(&tree, personal)
        .map(|found| found.sections.iter().map(|section| section.id).collect())
        .unwrap_or_default();
    assert_eq!(order, vec![notes, cards], "not at the end of the folder");
    assert_eq!(folder(&tree, cards).cloned(), inside);
    assert_eq!(vault.entry(visa).map(|entry| entry.group), Some(cards));
    vault.save().expect("the database saves");

    let written = library(&path, BUILT_PASSWORD);
    assert!(written.deleted_objects.is_empty());
    assert_eq!(
        written
            .group(cards)
            .and_then(|group| group.previous_parent().map(|left| left.id())),
        Some(banking)
    );
}

/// A folder's undo goes by what the file says of the folder, as an entry's
/// does. Whatever was done after the move - the folder moved on, taken away and
/// back, gone, either folder in the bin or gone, the folder it left moved
/// inside it, the move never saved and the file read again - the undo is
/// refused and moves nothing.
#[test]
fn a_folder_move_taken_back_after_something_else_happened_moves_nothing() {
    /// The folders of one vault: the one moved, the one it left, the one it
    /// went into, and one more.
    struct Folders {
        cards: GroupId,
        personal: GroupId,
        banking: GroupId,
        work: GroupId,
    }
    type Since = fn(&mut Vault, &Folders);
    let ways: [(&str, Since); 9] = [
        ("the folder moved on", |vault, at| {
            vault.move_group(at.cards, at.work).expect("it moves on");
        }),
        ("the folder went away and came back", |vault, at| {
            vault.move_group(at.cards, at.work).expect("it moves on");
            vault.move_group(at.cards, at.banking).expect("and back");
        }),
        ("the folder went to the bin", |vault, at| {
            vault
                .delete_group(at.cards, Deletion::Bin)
                .expect("it goes to the bin");
        }),
        ("the folder is gone", |vault, at| {
            vault
                .delete_group(at.cards, Deletion::Bin)
                .expect("it goes to the bin");
            vault
                .delete_group(at.cards, Deletion::Forever)
                .expect("and out of the file");
        }),
        ("the folder it went into went to the bin", |vault, at| {
            vault
                .delete_group(at.banking, Deletion::Bin)
                .expect("it goes to the bin");
        }),
        ("the folder it left went to the bin", |vault, at| {
            vault
                .delete_group(at.personal, Deletion::Bin)
                .expect("it goes to the bin");
        }),
        ("the folder it left is gone", |vault, at| {
            vault
                .delete_group(at.personal, Deletion::Bin)
                .expect("it goes to the bin");
            vault
                .delete_group(at.personal, Deletion::Forever)
                .expect("and out of the file");
        }),
        ("the folder it left was moved inside it", |vault, at| {
            vault
                .move_group(at.personal, at.cards)
                .expect("the folder it left moves into it");
        }),
        (
            "the move was never saved, and the file was read again",
            |vault, _| {
                vault.reload().expect("the file is read again");
            },
        ),
    ];

    for (what, since) in ways {
        let (_scratch, path) = cheap("folder since.kdbx");
        let mut vault = open(&path, BUILT_PASSWORD);
        let root = vault.tree().id;
        let mut add = |parent, name| {
            vault
                .create_group(parent, name)
                .expect("the folder is made")
        };
        let (personal, banking, work) = (
            add(root, "Personal"),
            add(root, "Banking"),
            add(root, "Work"),
        );
        let cards = add(personal, "Cards");
        vault.save().expect("the database saves");
        let at = Folders {
            cards,
            personal,
            banking,
            work,
        };

        vault.move_group(cards, banking).expect("the folder moves");
        since(&mut vault, &at);
        let tree = vault.tree();
        let edits = vault.edits();

        let answer = vault.move_group_back(cards, personal, banking);
        assert!(
            matches!(answer, Err(VaultError::MoveSuperseded)),
            "{what}: {answer:?}"
        );
        assert_eq!(vault.tree(), tree, "{what}: something moved");
        assert_eq!(vault.edits(), edits, "{what}");
    }
}

/// A claim about a folder the file does not bear out moves nothing: a folder
/// never moved, the top of the vault, and two another client wrote - the bin,
/// moved into a folder, which Coffer never moves, and a folder moved to where
/// it already was, which has nowhere to go back to.
#[test]
fn a_folder_move_the_file_does_not_bear_out_is_refused() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let mut ids = None;
    let path = built(scratch.path(), "claims.kdbx", |database| {
        let root = database.root().id();
        let mut add = |name: &str| {
            let mut root = database.root_mut();
            let mut group = root.add_group();
            group.name = name.to_owned();
            group.id()
        };
        let (archive, bin, personal, work) = (
            add("Archive"),
            add("Recycle Bin"),
            add("Personal"),
            add("Work"),
        );
        database.meta.recyclebin_enabled = Some(true);
        database.meta.recyclebin_uuid = Some(bin.uuid());
        for (moved, into) in [(bin, archive), (personal, root)] {
            database
                .group_mut(moved)
                .expect("the folder is there")
                .move_to(into)
                .expect("the other client moves it");
        }
        ids = Some((root, archive, bin, personal, work));
    });
    let (root, archive, bin, personal, work) = ids.expect("the vault is built");

    let mut vault = open(&path, BUILT_PASSWORD);
    assert_eq!(bin_of(&vault.tree()).id, bin);
    let tree = vault.tree();
    let edits = vault.edits();
    for (what, (id, from, into)) in [
        ("a folder never moved", (work, personal, root)),
        ("the top of the vault", (root, personal, root)),
        ("the bin", (bin, root, archive)),
        ("a folder moved to where it was", (personal, root, root)),
    ] {
        let answer = vault.move_group_back(id, from, into);
        assert!(
            matches!(answer, Err(VaultError::MoveSuperseded)),
            "{what}: {answer:?}"
        );
    }
    assert_eq!(vault.tree(), tree);
    assert_eq!(vault.edits(), edits);
    assert_eq!(vault.rescue(), Rescue::Nothing);
}
