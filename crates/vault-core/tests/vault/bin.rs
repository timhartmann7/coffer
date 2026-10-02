//! The recycle bin: what a deletion says it will do before it happens, what the
//! bin remembers about what it holds, and putting things back out of it.

use std::path::{Path, PathBuf};

use keepass::DatabaseKey;
use keepass::db::fields;
use vault_core::model::{Deletion, EntryId, GroupId, Project};
use vault_core::{Rescue, Vault, VaultError};

use crate::support::{
    self, BUILT_PASSWORD, RICH, SECRET, built, cheap, entry_titled, folder, holder, library, made,
    only_group, open,
};

/// The recycle bin, wherever the tree holds it.
pub fn bin_of(tree: &Project) -> &Project {
    fn walk(group: &Project) -> Option<&Project> {
        if group.is_recycle_bin {
            return Some(group);
        }
        group.sections.iter().find_map(walk)
    }
    walk(tree).expect("the vault has a recycle bin")
}

/// Puts an entry back, and says which folder it is in afterwards.
fn put_back(vault: &mut Vault, id: EntryId) -> GroupId {
    vault.put_back_entry(id).expect("it is put back");
    vault.entry(id).expect("it is still in the file").group
}

/// Puts a folder back, and says which folder holds it afterwards.
fn put_back_folder(vault: &mut Vault, id: GroupId) -> GroupId {
    vault.put_back_group(id).expect("it is put back");
    holder(&vault.tree(), id).expect("it is still in the file")
}

/// The whole round a reader makes: an entry goes to the bin, the file is
/// written and read again, and putting it back takes it to the folder it left.
/// The folder is known only through `PreviousParentGroup`, so it has to survive
/// the file.
#[test]
fn an_entry_put_back_goes_home_to_the_folder_it_was_deleted_from() {
    let (_scratch, path) = cheap("home.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let personal = vault
        .create_group(root, "Personal")
        .expect("the folder is made");
    let bank = made(&mut vault, personal, "Bank");
    assert_eq!(
        vault.entry(bank).expect("it is there").deletion,
        Deletion::Bin
    );

    vault
        .delete_entry(bank, Deletion::Bin)
        .expect("it goes to the bin");
    let binned = vault.entry(bank).expect("it is still in the file");
    let known = binned.binned.expect("the entry says it is in the bin");
    assert_eq!(known.from, Some(personal));
    assert!(
        known.since.is_some(),
        "the bin does not say when it went in"
    );
    assert_eq!(binned.deletion, Deletion::Forever);

    vault.save().expect("the database saves");
    drop(vault);

    let mut vault = open(&path, BUILT_PASSWORD);
    assert_eq!(
        vault
            .entry(bank)
            .and_then(|entry| entry.binned)
            .and_then(|binned| binned.from),
        Some(personal),
        "the folder it came from did not survive the file"
    );

    assert_eq!(put_back(&mut vault, bank), personal);
    let back = vault.entry(bank).expect("it is there");
    assert_eq!(back.group, personal);
    assert_eq!(back.binned, None);
    assert_eq!(back.deletion, Deletion::Bin);

    vault.save().expect("the database saves");
    drop(vault);
    let vault = open(&path, BUILT_PASSWORD);
    assert_eq!(vault.entry(bank).map(|entry| entry.group), Some(personal));
}

/// What Coffer writes as the folder an entry came from is what KeePassXC reads
/// as one, so a deletion made here can be put back there and the other way
/// round.
#[test]
fn keepassxc_reads_the_folder_a_deleted_entry_came_from() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let (_scratch, database) = support::scratch(RICH);

    {
        let mut vault = open(&database, SECRET);
        let basic = entry_titled(&vault, "basic").id;
        vault
            .delete_entry(basic, Deletion::Bin)
            .expect("it goes to the bin");
        vault.save().expect("the database saves");
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
        "KeePassXC does not see where the entry came from"
    );
}

/// The fixture's own deleted entry was put in the bin by KeePassXC, which
/// wrote down where from. Coffer reads that and takes it back there.
#[test]
fn an_entry_keepassxc_deleted_goes_back_where_keepassxc_says_it_came_from() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let work = only_group(&vault, "Work");
    let deleted = entry_titled(&vault, "deleted entry");

    let binned = deleted.binned.expect("it is in the bin");
    assert_eq!(binned.from, Some(work));

    assert_eq!(put_back(&mut vault, deleted.id), work);
    assert_eq!(vault.entry(deleted.id).map(|entry| entry.group), Some(work));
}

/// Going to the bin and coming back are moves. A move writes no version, does
/// not touch the modification time, and records nothing in `DeletedObjects`,
/// where a record would make every other client delete the entry on its next
/// merge.
#[test]
fn a_move_to_the_bin_and_back_is_neither_an_edit_nor_a_deletion() {
    let (_scratch, database) = support::scratch(RICH);
    {
        // Settled once, so the history the test counts is the one a save keeps.
        let mut vault = open(&database, SECRET);
        vault.save().expect("the database saves");
    }

    let mut vault = open(&database, SECRET);
    let versioned = entry_titled(&vault, "versioned");
    let versions = vault.versions(versioned.id).len();
    assert!(
        versions > 0,
        "the fixture entry is supposed to have history"
    );
    let work = only_group(&vault, "Work");

    vault
        .delete_entry(versioned.id, Deletion::Bin)
        .expect("it goes to the bin");
    vault
        .delete_group(work, Deletion::Bin)
        .expect("it goes to the bin");
    assert_eq!(vault.versions(versioned.id).len(), versions);
    vault.save().expect("the database saves");

    vault.put_back_entry(versioned.id).expect("it comes back");
    vault.put_back_group(work).expect("it comes back");
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, SECRET);
    let back = vault.entry(versioned.id).expect("it is there");
    assert_eq!(back.versions, versions, "a move wrote a version");
    assert_eq!(back.times.modified, versioned.times.modified);

    let written = library(&database, SECRET);
    for moved in [versioned.id.uuid(), work.uuid()] {
        assert!(
            !written.deleted_objects.contains_key(&moved),
            "a move was recorded as a deletion"
        );
    }
    assert_eq!(
        written.deleted_objects.len(),
        2,
        "the fixture's own two records"
    );
}

/// A deleted folder is a folder in the bin, with its own folders inside it,
/// rather than a heap of entries with nothing to say where they came from. It
/// says where it goes back to; what is inside it says when it went in, which
/// is when the folder did.
#[test]
fn a_deleted_folder_stays_a_folder_in_the_bin_and_comes_back_whole() {
    let (_scratch, path) = cheap("folders.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let personal = vault
        .create_group(root, "Personal")
        .expect("the folder is made");
    let banking = vault
        .create_group(personal, "Banking")
        .expect("the folder is made");
    let cards = vault
        .create_group(banking, "Cards")
        .expect("the folder is made");
    let bank = made(&mut vault, banking, "Bank");
    let visa = made(&mut vault, cards, "Visa");

    vault
        .delete_group(banking, Deletion::Bin)
        .expect("it goes to the bin");

    let tree = vault.tree();
    let bin = bin_of(&tree);
    assert!(
        bin.entries.is_empty(),
        "the folder's entries were flattened"
    );
    let gone = folder(bin, banking).expect("the folder is in the bin");
    let inner = folder(gone, cards).expect("its folder is still inside it");
    assert_eq!(
        inner
            .entries
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>(),
        vec![visa]
    );

    let went = gone.binned.expect("the folder says it is in the bin");
    assert_eq!(went.from, Some(personal));
    assert!(went.since.is_some());
    assert_eq!(gone.deletion, Deletion::Forever);

    let within = inner.binned.expect("a folder inside it is in the bin too");
    assert_eq!(within.since, went.since, "it went in when its folder did");
    assert_eq!(within.within, Some(banking), "it went in with its folder");
    assert_eq!(went.within, None, "the folder went in on its own");
    assert_eq!(
        within.from,
        Some(personal),
        "it goes where its folder came from"
    );
    assert_eq!(inner.deletion, Deletion::Forever);
    for row in gone.entries.iter().chain(&inner.entries) {
        assert_eq!(row.binned.map(|binned| binned.since), Some(went.since));
    }
    assert_eq!(
        vault.entry(visa).map(|entry| entry.deletion),
        Some(Deletion::Forever)
    );

    assert_eq!(put_back_folder(&mut vault, banking), personal);
    let tree = vault.tree();
    let home = folder(&tree, personal).expect("the folder it went back to");
    let back = folder(home, banking).expect("the folder is back where it was");
    assert_eq!(back.binned, None);
    assert_eq!(back.deletion, Deletion::Bin);
    let inner = folder(back, cards).expect("with its folder inside it");
    assert_eq!(inner.binned, None);
    assert!(back.entries.iter().any(|entry| entry.id == bank));
    assert!(inner.entries.iter().all(|entry| entry.binned.is_none()));
    assert!(bin_of(&tree).sections.is_empty());
    assert_eq!(
        vault.entry(visa).map(|entry| entry.deletion),
        Some(Deletion::Bin)
    );
}

/// What went into the bin with a deleted folder goes back where that folder
/// came from, and says it went in with it.
///
/// Not where its own `PreviousParentGroup` points. KeePass and KeePassXC write
/// one on every drag between folders, so an entry filed from Personal into
/// Banking last year carries Personal, and deleting Banking does not move it.
/// Read as "deleted from", that sent it back into a folder it left long ago,
/// and a reader with no way to move an entry had no way to put it right.
#[test]
fn what_went_in_with_a_folder_goes_back_where_the_folder_came_from() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let mut ids = None;
    let path = built(scratch.path(), "filed.kdbx", |database| {
        let mut root = database.root_mut();
        let mut add = |name: &str| {
            let mut group = root.add_group();
            group.name = name.to_owned();
            group.id()
        };
        let (personal, home, cards) = (add("Personal"), add("Home"), add("Cards"));
        let banking = {
            let mut home = database.group_mut(home).expect("the folder is there");
            let mut banking = home.add_group();
            banking.name = "Banking".to_owned();
            banking.id()
        };
        let mut personal_group = database.group_mut(personal).expect("the folder is there");
        let mut filed = |title: &str| {
            let mut entry = personal_group.add_entry();
            entry.edit(|entry| entry.set_unprotected(fields::TITLE, title));
            entry.id()
        };
        let (chase, visa) = (filed("Chase"), filed("Visa"));
        for entry in [chase, visa] {
            database
                .entry_mut(entry)
                .expect("the entry is there")
                .move_to(banking)
                .expect("the folder is there");
        }
        database
            .group_mut(cards)
            .expect("the folder is there")
            .move_to(banking)
            .expect("the folder is there");
        ids = Some((personal, home, banking, cards, chase, visa));
    });
    let (personal, home, banking, cards, chase, visa) = ids.expect("the vault is built");

    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    vault
        .delete_group(banking, Deletion::Bin)
        .expect("it goes to the bin");
    vault.save().expect("the database saves");
    drop(vault);

    let mut vault = open(&path, BUILT_PASSWORD);
    let went = vault
        .entry(chase)
        .and_then(|entry| entry.binned)
        .expect("it is in the bin");
    assert_eq!(went.within, Some(banking));
    assert_eq!(
        went.from,
        Some(home),
        "it claims to come from a folder it left before the deletion"
    );
    assert_ne!(went.from, Some(personal));
    let tree = vault.tree();
    let deleted = folder(&tree, banking).and_then(|found| found.binned);
    assert_eq!(
        deleted.map(|binned| (binned.within, binned.from)),
        Some((None, Some(home)))
    );
    let inner = folder(&tree, cards).and_then(|found| found.binned);
    assert_eq!(
        inner.map(|binned| (binned.within, binned.from)),
        Some((Some(banking), Some(home)))
    );

    assert_eq!(put_back(&mut vault, chase), home);
    assert_eq!(put_back_folder(&mut vault, cards), home);

    // The folder it would go back with goes in after it: there is nowhere left
    // to take it but the top.
    vault
        .delete_group(home, Deletion::Bin)
        .expect("it goes to the bin");
    let stayed = vault
        .entry(visa)
        .and_then(|entry| entry.binned)
        .expect("it is still in the bin");
    assert_eq!((stayed.within, stayed.from), (Some(banking), None));
    assert_eq!(put_back(&mut vault, visa), root);
}

/// A hundred folders deep is a hundred folders that all know they are in the
/// bin, and one of them put back on its own goes where the folder they all
/// went in with came from, which is the top of the vault.
#[test]
fn a_hundred_folders_deep_in_the_bin_all_know_it() {
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
    let bottom = made(&mut vault, here, "at the bottom");
    let top = *chain.first().expect("a hundred folders");
    let middle = *chain.get(50).expect("a hundred folders");

    vault
        .delete_group(top, Deletion::Bin)
        .expect("it goes to the bin");
    let tree = vault.tree();
    let since = folder(&tree, top)
        .and_then(|found| found.binned)
        .map(|binned| binned.since);
    for id in &chain {
        let found = folder(&tree, *id).expect("every folder is still there");
        assert_eq!(found.binned.map(|binned| binned.since), since);
        assert_eq!(found.deletion, Deletion::Forever);
    }

    assert_eq!(put_back_folder(&mut vault, middle), root);
    let tree = vault.tree();
    for (depth, id) in chain.iter().enumerate() {
        let found = folder(&tree, *id).expect("every folder is still there");
        assert_eq!(found.binned.is_some(), depth < 50, "level {depth}");
    }
    assert_eq!(
        vault.entry(bottom).map(|entry| entry.deletion),
        Some(Deletion::Bin)
    );
}

/// The folder an entry came from can be in the bin itself by the time the
/// entry is put back, or gone from the file altogether. Neither is somewhere to
/// go, and the entry goes to the top of the vault rather than staying behind.
#[test]
fn an_entry_whose_folder_is_in_the_bin_or_gone_goes_back_to_the_top() {
    let (_scratch, path) = cheap("orphans.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;

    let personal = vault
        .create_group(root, "Personal")
        .expect("the folder is made");
    let bank = made(&mut vault, personal, "Bank");
    vault
        .delete_entry(bank, Deletion::Bin)
        .expect("it goes to the bin");
    vault
        .delete_group(personal, Deletion::Bin)
        .expect("its folder follows it");
    assert_eq!(
        vault
            .entry(bank)
            .and_then(|entry| entry.binned)
            .map(|binned| binned.from),
        Some(None),
        "an entry offered to go back into a folder in the bin"
    );
    assert_eq!(put_back(&mut vault, bank), root);

    let old = vault.create_group(root, "Old").expect("the folder is made");
    let mail = made(&mut vault, old, "Mail");
    vault
        .delete_entry(mail, Deletion::Bin)
        .expect("it goes to the bin");
    vault
        .delete_group(old, Deletion::Bin)
        .expect("its folder goes to the bin");
    vault
        .delete_group(old, Deletion::Forever)
        .expect("and out of the file");
    assert!(
        vault.entry(mail).is_some(),
        "erasing the folder took an entry that had already left it"
    );
    assert_eq!(
        vault
            .entry(mail)
            .and_then(|entry| entry.binned)
            .map(|binned| binned.from),
        Some(None)
    );
    assert_eq!(put_back(&mut vault, mail), root);
    assert_eq!(vault.entry(mail).map(|entry| entry.group), Some(root));
}

/// Deleted from the top of the vault, it goes back to the top: the root is a
/// folder like any other to come from.
#[test]
fn an_entry_deleted_from_the_top_of_the_vault_goes_back_there() {
    let (_scratch, path) = cheap("top.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let note = made(&mut vault, root, "Note");

    vault
        .delete_entry(note, Deletion::Bin)
        .expect("it goes to the bin");
    assert_eq!(
        vault
            .entry(note)
            .and_then(|entry| entry.binned)
            .and_then(|binned| binned.from),
        Some(root)
    );
    assert_eq!(put_back(&mut vault, note), root);
}

/// Another client can put an entry or a folder in the bin without writing
/// down where it came from. Nothing is made up: the bin says it does not know,
/// and putting it back takes it to the top.
#[test]
fn what_another_client_binned_without_saying_where_from_goes_to_the_top() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "foreign.kdbx", |database| {
        let bin = {
            let mut root = database.root_mut();
            let mut bin = root.add_group();
            bin.name = "Recycle Bin".to_owned();
            bin.id()
        };
        database.meta.recyclebin_enabled = Some(true);
        database.meta.recyclebin_uuid = Some(bin.uuid());
        let mut bin = database.group_mut(bin).expect("the bin is there");
        bin.add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "stray"));
        bin.add_group().name = "stray folder".to_owned();
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let stray = entry_titled(&vault, "stray");
    assert_eq!(stray.binned.map(|binned| binned.from), Some(None));
    let loose = only_group(&vault, "stray folder");

    assert_eq!(put_back(&mut vault, stray.id), root);
    assert_eq!(put_back_folder(&mut vault, loose), root);
    assert!(bin_of(&vault.tree()).sections.is_empty());
}

/// Only what is in the bin comes out of it. The bin itself is not in the bin,
/// the top of the vault is in nothing, an entry already put back has nothing
/// to come out of, and one deleted forever is not there to move. None of these
/// may move anything, and a refusal leaves the vault with nothing to write.
#[test]
fn nothing_outside_the_bin_can_be_put_back() {
    let (_scratch, path) = cheap("refusals.kdbx");
    let (root, bank, bin, erased, emptied) = {
        let mut vault = open(&path, BUILT_PASSWORD);
        let root = vault.tree().id;
        let bank = made(&mut vault, root, "Bank");
        let gone = made(&mut vault, root, "Gone");
        vault
            .delete_entry(gone, Deletion::Bin)
            .expect("the bin is made");
        let bin = bin_of(&vault.tree()).id;

        let erased = made(&mut vault, root, "Erased");
        vault
            .delete_entry(erased, Deletion::Bin)
            .expect("it goes to the bin");
        vault
            .delete_entry(erased, Deletion::Forever)
            .expect("and out of the file");
        let emptied = vault
            .create_group(root, "Emptied")
            .expect("the folder is made");
        vault
            .delete_group(emptied, Deletion::Bin)
            .expect("it goes to the bin");
        vault
            .delete_group(emptied, Deletion::Forever)
            .expect("and out of the file");

        vault.save().expect("the database saves");
        (root, bank, bin, erased, emptied)
    };

    let mut vault = open(&path, BUILT_PASSWORD);
    assert!(matches!(
        vault.put_back_group(bin),
        Err(VaultError::NotInRecycleBin)
    ));
    assert!(matches!(
        vault.put_back_group(root),
        Err(VaultError::NotInRecycleBin)
    ));
    assert!(matches!(
        vault.put_back_entry(bank),
        Err(VaultError::NotInRecycleBin)
    ));
    assert!(matches!(
        vault.put_back_entry(erased),
        Err(VaultError::NoSuchEntry)
    ));
    assert!(matches!(
        vault.put_back_group(emptied),
        Err(VaultError::NoSuchGroup)
    ));
    assert_eq!(
        vault.rescue(),
        Rescue::Nothing,
        "a refusal changed the vault"
    );

    // The same entry put back twice: the second press finds nothing to do and
    // leaves it where the first one put it.
    let gone = entry_titled(&vault, "Gone").id;
    assert_eq!(put_back(&mut vault, gone), root);
    assert!(matches!(
        vault.put_back_entry(gone),
        Err(VaultError::NotInRecycleBin)
    ));
    assert_eq!(vault.entry(gone).map(|entry| entry.group), Some(root));
}

/// A database Coffer will not write back still says what is in its bin, and
/// puts nothing back.
#[test]
fn a_read_only_database_says_what_is_in_the_bin_and_puts_nothing_back() {
    let (_scratch, database) = support::scratch(RICH);
    {
        let mut vault = open(&database, SECRET);
        vault.save().expect("the database saves");
    }

    let snapshot = vault_core::storage::snapshot::slot(&database, 1).expect("a slot has a name");
    let mut vault = open(&snapshot, SECRET);
    let deleted = entry_titled(&vault, "deleted entry");
    assert!(deleted.binned.is_some());
    let work = only_group(&vault, "Work");

    assert!(matches!(
        vault.put_back_entry(deleted.id),
        Err(VaultError::ReadOnlySnapshot)
    ));
    assert!(matches!(
        vault.put_back_group(work),
        Err(VaultError::ReadOnlySnapshot)
    ));
    assert!(
        bin_of(&vault.tree())
            .entries
            .iter()
            .any(|row| row.id == deleted.id)
    );
}

/// A vault shaped to catch every way the answer could be wrong: the bin inside
/// a folder, a folder inside the bin, and an entry in each place.
pub fn awkward(directory: &Path, name: &str, keeps: bool) -> PathBuf {
    built(directory, name, |database| {
        let root = database.root().id();
        let archive = {
            let mut root = database.root_mut();
            let mut archive = root.add_group();
            archive.name = "Archive".to_owned();
            archive
                .add_entry()
                .edit(|entry| entry.set_unprotected(fields::TITLE, "archived"));
            archive.id()
        };
        let bin = {
            let mut archive = database.group_mut(archive).expect("the folder is there");
            let mut bin = archive.add_group();
            bin.name = "Recycle Bin".to_owned();
            bin.add_entry()
                .edit(|entry| entry.set_unprotected(fields::TITLE, "binned"));
            let mut inside = bin.add_group();
            inside.name = "Binned folder".to_owned();
            inside
                .add_entry()
                .edit(|entry| entry.set_unprotected(fields::TITLE, "inside"));
            bin.id()
        };
        database.meta.recyclebin_enabled = Some(keeps);
        database.meta.recyclebin_uuid = Some(bin.uuid());

        let mut root = database.group_mut(root).expect("the top is there");
        root.add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "top"));
        let mut personal = root.add_group();
        personal.name = "Personal".to_owned();
        personal
            .add_entry()
            .edit(|entry| entry.set_unprotected(fields::TITLE, "bank"));
    })
}

/// What a deletion says it will do before it happens is what it does, for
/// every entry and every folder, in a vault that keeps a bin and in one that
/// does not. A window that promised the bin and got an erasure would be
/// offering to undo something nobody can undo.
///
/// Each deletion happens in a fresh copy of the file, so the answer is always
/// about the vault as the reader last saw it.
#[test]
fn what_a_deletion_says_it_will_do_is_what_it_does() {
    let scratch = tempfile::tempdir().expect("a scratch directory");

    for keeps in [true, false] {
        let path = awkward(scratch.path(), &format!("awkward-{keeps}.kdbx"), keeps);

        let expected_entries = [
            ("top", keeps),
            ("bank", keeps),
            ("archived", keeps),
            ("binned", false),
            ("inside", false),
        ];
        for (title, kept) in expected_entries {
            let copy = scratch.path().join(format!("entry-{title}-{keeps}.kdbx"));
            std::fs::copy(&path, &copy).expect("the file copies");
            let mut vault = open(&copy, BUILT_PASSWORD);

            let entry = entry_titled(&vault, title);
            let said = entry.deletion;
            assert_eq!(said == Deletion::Bin, kept, "{title} said {said:?}");

            vault.delete_entry(entry.id, said).expect("it is deleted");
            let after = vault.entry(entry.id);
            match said {
                Deletion::Bin => assert!(
                    after.is_some_and(|found| found.binned.is_some()),
                    "{title} was promised the bin and did not land in it"
                ),
                Deletion::Forever => assert!(
                    after.is_none(),
                    "{title} was said to go for good and is still here"
                ),
            }
        }

        let expected_folders = [
            ("Personal", keeps),
            // The bin is inside it, and a folder cannot go inside itself.
            ("Archive", false),
            ("Recycle Bin", false),
            ("Binned folder", false),
        ];
        for (name, kept) in expected_folders {
            let copy = scratch.path().join(format!("folder-{name}-{keeps}.kdbx"));
            std::fs::copy(&path, &copy).expect("the file copies");
            let mut vault = open(&copy, BUILT_PASSWORD);

            let id = only_group(&vault, name);
            let said = folder(&vault.tree(), id)
                .map(|found| found.deletion)
                .expect("the folder is in the tree");
            assert_eq!(said == Deletion::Bin, kept, "{name} said {said:?}");

            vault.delete_group(id, said).expect("it is deleted");
            let tree = vault.tree();
            match said {
                Deletion::Bin => assert!(
                    folder(&tree, id).is_some_and(|found| found.binned.is_some()),
                    "{name} was promised the bin and did not land in it"
                ),
                Deletion::Forever => assert!(
                    folder(&tree, id).is_none(),
                    "{name} was said to go for good and is still here"
                ),
            }
        }
    }
}

/// The answer is read off the file the vault holds now. Another client that
/// turned the bin off while Coffer had the vault open is a reload away from
/// every deletion being final, and the next answer says so.
#[test]
fn what_a_deletion_will_do_follows_a_file_another_client_changed() {
    let (_scratch, path) = cheap("changed.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let bank = made(&mut vault, root, "Bank");
    vault.save().expect("the database saves");
    assert_eq!(
        vault.entry(bank).map(|entry| entry.deletion),
        Some(Deletion::Bin)
    );

    {
        let key = || DatabaseKey::new().with_password(BUILT_PASSWORD);
        let mut other = library(&path, BUILT_PASSWORD);
        other.meta.recyclebin_enabled = Some(false);
        let mut file = std::fs::File::create(&path).expect("the file is rewritten");
        other
            .save(&mut file, key())
            .expect("the other client saves");
    }

    vault.reload().expect("the vault reads the file again");
    assert_eq!(
        vault.entry(bank).map(|entry| entry.deletion),
        Some(Deletion::Forever)
    );
    assert_eq!(vault.tree().deletion, Deletion::Forever);
    vault
        .delete_entry(bank, Deletion::Forever)
        .expect("it is deleted");
    assert!(vault.entry(bank).is_none());
}

/// Two deletions the reader asked for one after the other can reach the vault
/// in either order: they wait on one lock behind a save, and the thread that
/// takes it first is not the one that asked first. A folder that goes into the
/// bin ahead of an entry in it makes that entry's move to the bin an erasure,
/// and the reader agreed to a move. So it is refused, and nothing changes: the
/// entry stays in the file, inside the folder it was in, in the bin.
#[test]
fn an_entry_whose_folder_went_to_the_bin_first_is_not_erased_on_the_way() {
    let (_scratch, path) = cheap("order.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let banking = vault
        .create_group(root, "Banking")
        .expect("the folder is made");
    let chase = made(&mut vault, banking, "Chase");
    assert_eq!(
        vault.entry(chase).map(|entry| entry.deletion),
        Some(Deletion::Bin),
        "what the window shows before either deletion is pressed"
    );

    vault
        .delete_group(banking, Deletion::Bin)
        .expect("the folder goes to the bin");
    assert!(matches!(
        vault.delete_entry(chase, Deletion::Bin),
        Err(VaultError::DeletionChanged)
    ));

    let entry = vault.entry(chase).expect("the entry is still in the file");
    assert_eq!(entry.group, banking, "the entry left the folder it was in");
    assert_eq!(
        entry.binned.and_then(|binned| binned.within),
        Some(banking),
        "the entry is in the bin with its folder"
    );
    assert_eq!(entry.deletion, Deletion::Forever, "read again, it says so");

    // Asked for again as what it now is, it goes.
    vault
        .delete_entry(chase, Deletion::Forever)
        .expect("the reader agreed to this one");
    assert!(vault.entry(chase).is_none());
}

/// The other order, which needs no refusal: the entry goes to the bin on its
/// own, and its folder follows it there.
#[test]
fn an_entry_that_went_to_the_bin_before_its_folder_stays_there_on_its_own() {
    let (_scratch, path) = cheap("order.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let banking = vault
        .create_group(root, "Banking")
        .expect("the folder is made");
    let chase = made(&mut vault, banking, "Chase");

    vault
        .delete_entry(chase, Deletion::Bin)
        .expect("the entry goes to the bin");
    vault
        .delete_group(banking, Deletion::Bin)
        .expect("the folder goes to the bin");

    let tree = vault.tree();
    let bin = bin_of(&tree);
    assert!(bin.entries.iter().any(|entry| entry.id == chase));
    assert!(bin.sections.iter().any(|section| section.id == banking));
}

/// A folder inside one that went into the bin first is in the bin by then, and
/// the move the reader agreed to for it would erase it and everything in it.
/// Refused, the same way; and in the other order both go to the bin.
#[test]
fn a_folder_whose_parent_went_to_the_bin_first_is_not_erased_on_the_way() {
    let (_scratch, path) = cheap("order.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let parent = vault
        .create_group(root, "Work")
        .expect("the folder is made");
    let child = vault
        .create_group(parent, "Servers")
        .expect("the folder is made");
    let inside = made(&mut vault, child, "SSH");

    vault
        .delete_group(parent, Deletion::Bin)
        .expect("the parent goes to the bin");
    assert!(matches!(
        vault.delete_group(child, Deletion::Bin),
        Err(VaultError::DeletionChanged)
    ));
    let tree = vault.tree();
    let held = folder(&tree, parent).expect("the parent is in the bin");
    assert!(
        held.sections.iter().any(|section| section.id == child),
        "the folder left the parent it was in"
    );
    assert_eq!(vault.entry(inside).map(|entry| entry.group), Some(child));

    let (_scratch, path) = cheap("order.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let parent = vault
        .create_group(root, "Work")
        .expect("the folder is made");
    let child = vault
        .create_group(parent, "Servers")
        .expect("the folder is made");

    vault
        .delete_group(child, Deletion::Bin)
        .expect("the folder goes to the bin");
    vault
        .delete_group(parent, Deletion::Bin)
        .expect("its parent follows it");
    let tree = vault.tree();
    let bin = bin_of(&tree);
    assert!(bin.sections.iter().any(|section| section.id == child));
    assert!(bin.sections.iter().any(|section| section.id == parent));
}

/// The other way round: something the reader agreed to erase from the bin
/// left it before the deletion arrived. Erasing it now would take out of the
/// file an entry that is back among the living, so that is refused too.
#[test]
fn an_erasure_of_something_that_left_the_bin_is_refused() {
    let (_scratch, path) = cheap("order.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let note = made(&mut vault, root, "Note");
    vault
        .delete_entry(note, Deletion::Bin)
        .expect("it goes to the bin");
    let old = vault.create_group(root, "Old").expect("the folder is made");
    vault
        .delete_group(old, Deletion::Bin)
        .expect("it goes to the bin");

    vault.put_back_entry(note).expect("it is put back");
    vault.put_back_group(old).expect("it is put back");

    assert!(matches!(
        vault.delete_entry(note, Deletion::Forever),
        Err(VaultError::DeletionChanged)
    ));
    assert!(matches!(
        vault.delete_group(old, Deletion::Forever),
        Err(VaultError::DeletionChanged)
    ));
    assert_eq!(vault.entry(note).map(|entry| entry.group), Some(root));
    assert!(folder(&vault.tree(), old).is_some());
}
