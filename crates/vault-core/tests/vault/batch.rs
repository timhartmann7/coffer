//! Changing many entries at once: deleting, putting back, and putting a tag on
//! and taking it off again, every entry a batch names or none of them.

use keepass::db::fields;
use vault_core::model::{Deletion, EntryId, GroupId};
use vault_core::{NewValue, Rescue, Vault, VaultError};

use crate::bin::{awkward, bin_of};
use crate::support::{
    self, BUILT_PASSWORD, RICH, SECRET, all_entries, built, cheap, entry_titled, files, library,
    made, open,
};

/// Every entry named, each with the deletion its row says.
fn as_shown(vault: &Vault, ids: &[EntryId]) -> Vec<(EntryId, Deletion)> {
    ids.iter()
        .map(|&id| (id, vault.entry(id).expect("the entry is there").deletion))
        .collect()
}

/// Where each entry is, by id, so a refused batch can be seen to have moved
/// nothing.
fn places(vault: &Vault) -> Vec<(EntryId, GroupId)> {
    let mut found: Vec<_> = all_entries(vault)
        .into_iter()
        .map(|row| (row.id, row.group))
        .collect();
    found.sort_by_key(|(id, _)| id.to_string());
    found
}

/// The tags an entry has now.
fn tags(vault: &Vault, id: EntryId) -> Vec<String> {
    vault.entry(id).expect("the entry is there").tags
}

/// A vault with no recycle bin yet, three folders and an entry in each.
fn three_folders(path: &std::path::Path) -> (Vault, [GroupId; 3], [EntryId; 3]) {
    let mut vault = open(path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let folders = ["Personal", "Work", "Banking"]
        .map(|name| vault.create_group(root, name).expect("the folder is made"));
    let entries = [0, 1, 2].map(|at| made(&mut vault, folders[at], &format!("entry {at}")));
    (vault, folders, entries)
}

/// Entries from two folders go to the bin in one call, and that is a move for
/// each of them and nothing else: no version, the modification time where it
/// was, nothing in `DeletedObjects`, and each one's folder written down as the
/// one it left. Read back from the file, putting them all back in one call
/// takes each to its own folder.
#[test]
fn a_batch_moved_to_the_bin_is_neither_an_edit_nor_a_deletion_and_comes_back_whole() {
    let (_scratch, database) = support::scratch(RICH);
    {
        // Settled once, so the history the test counts is the one a save keeps.
        let mut vault = open(&database, SECRET);
        vault.save().expect("the database saves");
    }

    let mut vault = open(&database, SECRET);
    let basic = entry_titled(&vault, "basic");
    let versioned = entry_titled(&vault, "versioned");
    assert_ne!(basic.group, versioned.group, "the two are in one folder");
    assert!(versioned.versions > 0, "the fixture entry has history");
    let held = files(&vault);

    vault
        .delete_entries(&[(basic.id, Deletion::Bin), (versioned.id, Deletion::Bin)])
        .expect("both go to the bin");
    vault.save().expect("the database saves");
    drop(vault);

    let written = library(&database, SECRET);
    assert_eq!(
        written.deleted_objects.len(),
        2,
        "the fixture's own two records, and no more"
    );
    for before in [&basic, &versioned] {
        let entry = written.entry(before.id).expect("the library finds it");
        assert_eq!(
            entry.previous_parent().map(|left| left.id()),
            Some(before.group),
            "the folder it left was not written down"
        );
    }

    let mut vault = open(&database, SECRET);
    for before in [&basic, &versioned] {
        let binned = vault.entry(before.id).expect("it is in the file");
        assert!(binned.binned.is_some(), "it is not in the bin");
        assert_eq!(binned.versions, before.versions, "a move wrote a version");
        assert_eq!(binned.times.modified, before.times.modified);
        assert_eq!(binned.deletion, Deletion::Forever);
    }

    vault
        .put_back_entries(&[basic.id, versioned.id])
        .expect("both come back");
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, SECRET);
    for before in [basic, versioned] {
        let back = vault.entry(before.id).expect("it is there");
        assert_eq!(back, before, "it came back other than it went");
    }
    assert_eq!(files(&vault), held, "a file changed on the way");
}

/// Every row says what deleting it would do, and it is what the entry itself
/// says. One batch of every entry in the vault, each with the answer its row
/// gave, does exactly that to each: erasures and moves into the bin side by
/// side, in a vault that keeps a bin and in one that does not, with the bin
/// inside a folder and a folder inside the bin.
#[test]
fn every_row_says_what_deleting_it_would_do_and_one_batch_of_all_of_them_does_it() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    for keeps in [true, false] {
        let path = awkward(scratch.path(), &format!("awkward-{keeps}.kdbx"), keeps);
        let mut vault = open(&path, BUILT_PASSWORD);

        let rows = all_entries(&vault);
        assert_eq!(rows.len(), 5);
        let shown: Vec<_> = rows.iter().map(|row| (row.id, row.deletion)).collect();
        for row in &rows {
            assert_eq!(
                Some(row.deletion),
                vault.entry(row.id).map(|entry| entry.deletion),
                "{:?}: the row and the entry disagree",
                row.title.open()
            );
        }
        assert!(shown.iter().any(|(_, said)| *said == Deletion::Forever));
        assert_eq!(
            shown.iter().any(|(_, said)| *said == Deletion::Bin),
            keeps,
            "a vault that keeps no bin promised one"
        );

        vault.delete_entries(&shown).expect("every one goes");
        for (id, said) in shown {
            let after = vault.entry(id);
            match said {
                Deletion::Bin => assert!(after.is_some_and(|found| found.binned.is_some())),
                Deletion::Forever => assert!(after.is_none()),
            }
        }
        vault.save().expect("the database saves");
    }
}

/// Two deletions can reach the vault in either order. An entry whose folder
/// went into the bin first would now be erased where the reader agreed to a
/// move, and one entry like that refuses the whole batch: every entry stays
/// where it was, and the vault has nothing new to save.
#[test]
fn one_entry_whose_deletion_changed_refuses_the_whole_batch() {
    let (_scratch, path) = cheap("changed.kdbx");
    let (mut vault, folders, entries) = three_folders(&path);
    let [personal, ..] = folders;
    // The bin is made by the first deletion, so the second has one to go into.
    let root = vault.tree().id;
    let first = made(&mut vault, root, "first");
    vault
        .delete_entries(&[(first, Deletion::Bin)])
        .expect("the bin is made");
    vault
        .delete_group(personal, Deletion::Bin)
        .expect("the folder goes first");

    let before = places(&vault);
    let edits = vault.edits();
    let shown: Vec<_> = entries.iter().map(|&id| (id, Deletion::Bin)).collect();
    assert!(matches!(
        vault.delete_entries(&shown),
        Err(VaultError::DeletionChanged)
    ));
    assert_eq!(places(&vault), before, "a refused batch moved something");
    assert_eq!(vault.edits(), edits, "a refused batch counted as a change");
    assert!(vault.entry(entries[0]).is_some(), "it was erased anyway");
}

/// An entry that has gone out of the file since the window drew it refuses
/// the batch it is in, put back and tagged alike, and nothing else moves.
#[test]
fn an_entry_that_is_gone_refuses_the_whole_batch() {
    let (_scratch, path) = cheap("gone.kdbx");
    let (mut vault, _, entries) = three_folders(&path);
    let root = vault.tree().id;
    let gone = made(&mut vault, root, "gone");
    vault
        .delete_entries(&[(gone, Deletion::Bin)])
        .expect("it goes to the bin");
    vault
        .delete_entries(&[(gone, Deletion::Forever)])
        .expect("and out of the file");

    let before = places(&vault);
    let edits = vault.edits();
    let mut shown: Vec<_> = entries.iter().map(|&id| (id, Deletion::Bin)).collect();
    shown.push((gone, Deletion::Bin));
    assert!(matches!(
        vault.delete_entries(&shown),
        Err(VaultError::NoSuchEntry)
    ));
    let mut named = entries.to_vec();
    named.insert(1, gone);
    assert!(matches!(
        vault.tag_entries(&named, "work"),
        Err(VaultError::NoSuchEntry)
    ));
    assert!(matches!(
        vault.untag_entries(&named, "work"),
        Err(VaultError::NoSuchEntry)
    ));
    assert!(matches!(
        vault.put_back_entries(&[gone]),
        Err(VaultError::NoSuchEntry)
    ));

    assert_eq!(places(&vault), before);
    assert_eq!(vault.edits(), edits);
    for id in entries {
        assert!(tags(&vault, id).is_empty(), "a refused batch put a tag on");
    }
}

/// A file a previous version holds in place can make an erasure impossible:
/// the pool has to stay an unbroken run from zero, and a version cannot be
/// rewritten. Two entries in the bin, the second's file pinned behind the
/// first's: the batch erases neither, moves nothing it named into the bin -
/// the erasures run first - and every file in the vault is as it was.
#[test]
fn a_batch_erasure_a_version_stands_in_the_way_of_erases_nothing() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "pinned.kdbx", |_| {});

    let (ids, outside) = {
        let mut vault = open(&path, BUILT_PASSWORD);
        let root = vault.tree().id;
        let mut ids = Vec::new();
        for round in 0..3u8 {
            let id = made(&mut vault, root, &format!("entry {round}"));
            support::attach(&mut vault, id, &format!("file-{round}.bin"), &[round; 64]);
            // The version this writes holds the file the entry has.
            vault
                .set_field(id, fields::NOTES, NewValue::Open(format!("note {round}")))
                .expect("the note is written");
            ids.push(id);
        }
        let outside = made(&mut vault, root, "outside");
        vault.save().expect("the database saves");
        (ids, outside)
    };

    let mut vault = open(&path, BUILT_PASSWORD);
    let [first, second, ..] = ids[..] else {
        panic!("three entries were made");
    };
    vault
        .delete_entries(&[(first, Deletion::Bin), (second, Deletion::Bin)])
        .expect("both go to the bin");
    let held = files(&vault);
    let before = places(&vault);
    let edits = vault.edits();

    assert!(matches!(
        vault.delete_entries(&[
            (outside, Deletion::Bin),
            (first, Deletion::Forever),
            (second, Deletion::Forever),
        ]),
        Err(VaultError::AttachmentInHistory)
    ));
    assert!(vault.entry(first).is_some() && vault.entry(second).is_some());
    assert_eq!(places(&vault), before, "the entry outside went to the bin");
    assert_eq!(files(&vault), held, "the pool changed under a refusal");
    assert_eq!(vault.edits(), edits);

    vault.save().expect("the database saves");
    drop(vault);
    assert_eq!(files(&open(&path, BUILT_PASSWORD)), held);
}

/// Three entries erased in one call are three records in `DeletedObjects`, so
/// no other client brings any of them back, and every file of every other
/// entry is where it was, byte for byte, although the pool closed up behind
/// three files at once.
#[test]
fn a_batch_erasure_records_every_entry_and_keeps_every_other_file() {
    let (_scratch, database) = support::scratch(RICH);
    let mut vault = open(&database, SECRET);
    let root = vault.tree().id;
    let mut doomed = Vec::new();
    for round in 0..3u8 {
        let id = made(&mut vault, root, &format!("doomed {round}"));
        support::attach(&mut vault, id, "scan.pdf", &[round; 300]);
        doomed.push(id);
    }
    let shown: Vec<_> = doomed.iter().map(|&id| (id, Deletion::Bin)).collect();
    vault.delete_entries(&shown).expect("they go to the bin");
    let kept: Vec<_> = files(&vault)
        .into_iter()
        .filter(|(title, ..)| !title.starts_with("doomed"))
        .collect();

    let shown: Vec<_> = doomed.iter().map(|&id| (id, Deletion::Forever)).collect();
    vault.delete_entries(&shown).expect("they go for good");
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&database, SECRET);
    for id in &doomed {
        assert!(vault.entry(*id).is_none(), "an erased entry came back");
    }
    assert_eq!(files(&vault), kept, "a file went to the wrong entry");
    let written = library(&database, SECRET);
    for id in doomed {
        assert!(
            written.deleted_objects.contains_key(&id.uuid()),
            "an erasure was not recorded"
        );
    }
}

/// One file two entries name - which KeePass 2 writes for identical files,
/// and nothing in Coffer makes - goes when both names go in one batch, and the
/// file nobody shared is still the one its entry names.
#[test]
fn a_batch_erasure_of_two_entries_sharing_a_file_erases_both_and_keeps_every_other() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<KeePassFile>
  <Meta>
    <Generator>coffer-test</Generator>
    <Binaries>
      <Binary ID="0" Compressed="False">c2hhcmVkIGJ5dGVz</Binary>
      <Binary ID="1" Compressed="False">bWluZSBhbG9uZQ==</Binary>
    </Binaries>
  </Meta>
  <Root>
    <Group>
      <UUID>YmF0Y2gtcm9vdC0tLS0tLQ==</UUID>
      <Name>batch</Name>
      <Times><CreationTime>2020-01-01T00:00:00Z</CreationTime><Expires>False</Expires></Times>
      <Entry>
        <UUID>YmF0Y2gtYS0tLS0tLS0tLQ==</UUID>
        <Times><CreationTime>2020-01-01T00:00:00Z</CreationTime><Expires>False</Expires></Times>
        <String><Key>Title</Key><Value>a</Value></String>
        <QualityCheck>False</QualityCheck>
        <Binary><Key>together.bin</Key><Value Ref="0"/></Binary>
      </Entry>
      <Entry>
        <UUID>YmF0Y2gtYi0tLS0tLS0tLQ==</UUID>
        <Times><CreationTime>2020-01-01T00:00:00Z</CreationTime><Expires>False</Expires></Times>
        <String><Key>Title</Key><Value>b</Value></String>
        <Binary><Key>together.bin</Key><Value Ref="0"/></Binary>
      </Entry>
      <Entry>
        <UUID>YmF0Y2gtYy0tLS0tLS0tLQ==</UUID>
        <Times><CreationTime>2020-01-01T00:00:00Z</CreationTime><Expires>False</Expires></Times>
        <String><Key>Title</Key><Value>c</Value></String>
        <Binary><Key>alone.bin</Key><Value Ref="1"/></Binary>
      </Entry>
    </Group>
  </Root>
</KeePassFile>
"#;
    let path = support::imported(&tool, scratch.path(), "batch", xml, SECRET);

    {
        let mut vault = open(&path, SECRET);
        assert!(
            vault.read_only().is_none(),
            "the import is not a file Coffer writes"
        );
        let (a, b) = (entry_titled(&vault, "a").id, entry_titled(&vault, "b").id);
        vault
            .delete_entries(&[(a, Deletion::Bin), (b, Deletion::Bin)])
            .expect("both go to the bin");
        vault
            .delete_entries(&[(b, Deletion::Forever), (a, Deletion::Forever)])
            .expect("both go for good together");
        vault.save().expect("the database saves");
    }

    let vault = open(&path, SECRET);
    assert_eq!(
        files(&vault),
        [(
            "c".to_owned(),
            "alone.bin".to_owned(),
            b"mine alone".to_vec()
        )],
        "the shared file outlived both its names, or the other moved"
    );
    let written = library(&path, SECRET);
    assert_eq!(
        written.num_attachments(),
        1,
        "the pool kept a file nobody names"
    );
}

/// An entry named twice with the same answer goes once. Named twice with two
/// answers, one of them is not what deleting it does, and the batch is
/// refused; named twice to be put back, or tagged, it is put back or tagged
/// once.
#[test]
fn the_same_entry_twice_in_a_batch_goes_once_and_two_answers_for_it_refuse() {
    let (_scratch, path) = cheap("twice.kdbx");
    let (mut vault, _, [one, two, _]) = three_folders(&path);

    let before = places(&vault);
    assert!(matches!(
        vault.delete_entries(&[(one, Deletion::Bin), (one, Deletion::Forever)]),
        Err(VaultError::DeletionChanged)
    ));
    assert_eq!(places(&vault), before);

    vault
        .delete_entries(&[
            (one, Deletion::Bin),
            (two, Deletion::Bin),
            (one, Deletion::Bin),
        ])
        .expect("they go to the bin");
    let bin = bin_of(&vault.tree()).entries.clone();
    assert_eq!(
        bin.iter().map(|row| row.id).collect::<Vec<_>>(),
        [one, two],
        "an entry named twice went twice"
    );

    vault
        .put_back_entries(&[two, one, two])
        .expect("they come back");
    assert!(bin_of(&vault.tree()).entries.is_empty());

    assert_eq!(
        vault
            .tag_entries(&[one, one, two], "twice")
            .expect("the tag goes on"),
        [one, two]
    );
    assert_eq!(tags(&vault, one), ["twice"], "a tag went on twice");
    assert_eq!(
        vault.versions(one).len(),
        2,
        "the title, and one for the tag"
    );
}

/// Nothing named is nothing done: no change counted, nothing to save, and no
/// bin made in a vault that has none yet.
#[test]
fn an_empty_batch_changes_nothing() {
    let (_scratch, path) = cheap("empty.kdbx");
    let mut vault = open(&path, BUILT_PASSWORD);
    let tree = vault.tree();
    let edits = vault.edits();

    vault.delete_entries(&[]).expect("nothing is deleted");
    vault.put_back_entries(&[]).expect("nothing is put back");
    assert!(
        vault
            .tag_entries(&[], "work")
            .expect("nothing is tagged")
            .is_empty()
    );
    vault
        .untag_entries(&[], "work")
        .expect("nothing is untagged");

    assert_eq!(vault.tree(), tree, "an empty batch made a bin");
    assert_eq!(vault.edits(), edits);
    assert_eq!(
        vault.rescue(),
        Rescue::Nothing,
        "an empty batch left something to save"
    );
}

/// A vault that asks for a recycle bin and has none yet gets one from the
/// first batch, one bin for the whole batch, the way KeePassXC makes one on
/// the first deletion.
#[test]
fn a_vault_with_no_bin_yet_makes_one_for_a_batch() {
    let (_scratch, path) = cheap("first.kdbx");
    let (mut vault, _, entries) = three_folders(&path);
    let shown = as_shown(&vault, &entries);
    assert!(shown.iter().all(|(_, said)| *said == Deletion::Bin));

    vault.delete_entries(&shown).expect("they go to the bin");
    let tree = vault.tree();
    let bins = tree
        .sections
        .iter()
        .filter(|section| section.is_recycle_bin)
        .count();
    assert_eq!(bins, 1, "a batch made a bin per entry");
    assert_eq!(bin_of(&tree).entries.len(), 3);
}

/// A vault that keeps no bin erases what the window said goes for good, and
/// refuses a batch that was shown the bin rather than erase what the reader
/// thought they could put back.
#[test]
fn a_vault_that_keeps_no_bin_erases_a_batch_shown_forever_and_refuses_one_shown_bin() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "nobin.kdbx", |database| {
        database.meta.recyclebin_enabled = Some(false);
        for title in ["one", "two"] {
            database
                .root_mut()
                .add_entry()
                .edit(|entry| entry.set_unprotected(fields::TITLE, title));
        }
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let ids = [
        entry_titled(&vault, "one").id,
        entry_titled(&vault, "two").id,
    ];
    assert!(
        as_shown(&vault, &ids)
            .iter()
            .all(|(_, said)| *said == Deletion::Forever)
    );

    assert!(matches!(
        vault.delete_entries(&[(ids[0], Deletion::Forever), (ids[1], Deletion::Bin)]),
        Err(VaultError::DeletionChanged)
    ));
    assert_eq!(all_entries(&vault).len(), 2, "a refused batch erased one");

    vault
        .delete_entries(&[(ids[0], Deletion::Forever), (ids[1], Deletion::Forever)])
        .expect("both go for good");
    assert!(all_entries(&vault).is_empty());
    assert!(
        !vault
            .tree()
            .sections
            .iter()
            .any(|section| section.is_recycle_bin),
        "a vault that keeps no bin was given one"
    );
}

/// A database Coffer will not write back takes no batch of any kind, and says
/// why with the reason the file is read only.
#[test]
fn a_read_only_vault_takes_no_batch() {
    let (_scratch, database) = support::scratch(RICH);
    {
        let mut vault = open(&database, SECRET);
        vault.save().expect("the database saves");
    }
    let snapshot = vault_core::storage::snapshot::slot(&database, 1).expect("a slot has a name");
    let (_older, third) = support::scratch("rich-kdbx31.kdbx");

    type Why = fn(&VaultError) -> bool;
    let cases: [(std::path::PathBuf, Why); 2] = [
        (snapshot, |error| {
            matches!(error, VaultError::ReadOnlySnapshot)
        }),
        (third, |error| {
            matches!(error, VaultError::ReadOnlyKdbx3Attachments)
        }),
    ];
    for (path, read_only) in cases {
        let mut vault = open(&path, SECRET);
        let ids: Vec<_> = all_entries(&vault).iter().map(|row| row.id).collect();
        let tree = vault.tree();

        let shown = as_shown(&vault, &ids);
        let refused = [
            vault.delete_entries(&shown),
            vault.put_back_entries(&ids),
            vault.tag_entries(&ids, "work").map(drop),
            vault.untag_entries(&ids, "work"),
        ];
        for answer in refused {
            let error = answer.expect_err("a read-only vault took a batch");
            assert!(read_only(&error), "{error:?}");
        }
        assert_eq!(vault.tree(), tree, "{}", path.display());
    }
}

/// Fifty thousand entries - a select-all of a large vault - go to the bin in
/// one batch and come back in another, in their order both times, and the
/// file still saves. The cost is in `docs/vault-core.md`; this is that it
/// ends.
#[test]
fn fifty_thousand_entries_go_to_the_bin_and_come_back_in_one_batch() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "many.kdbx", |database| {
        for index in 0..50_000 {
            database
                .root_mut()
                .add_entry()
                .edit(|entry| entry.set_unprotected(fields::TITLE, format!("entry {index}")));
        }
    });

    let mut vault = open(&path, BUILT_PASSWORD);
    let root = vault.tree().id;
    let ids: Vec<_> = vault.tree().entries.iter().map(|row| row.id).collect();
    assert_eq!(ids.len(), 50_000);

    let shown: Vec<_> = ids.iter().map(|&id| (id, Deletion::Bin)).collect();
    vault.delete_entries(&shown).expect("they go to the bin");
    let tree = vault.tree();
    assert!(tree.entries.is_empty());
    let binned: Vec<_> = bin_of(&tree).entries.iter().map(|row| row.id).collect();
    assert_eq!(binned, ids, "they did not arrive in order");

    vault.put_back_entries(&ids).expect("they come back");
    let tree = vault.tree();
    assert!(bin_of(&tree).entries.is_empty());
    let back: Vec<_> = tree.entries.iter().map(|row| row.id).collect();
    assert_eq!(back, ids, "they did not come back in order");
    assert!(tree.entries.iter().all(|row| row.group == root));
    vault.save().expect("the database saves");
    drop(vault);

    assert_eq!(open(&path, BUILT_PASSWORD).tree().entries.len(), 50_000);
}

/// Fifty thousand entries - a select-all of a large vault, then Add tag - take
/// a tag in one batch, one version each, and give it back in another. Each
/// time the file saves and opens again with the tag on every one of them, and
/// then on none, with two versions on each and the file a few times the size it
/// started at rather than growing with every entry it touched.
#[test]
fn fifty_thousand_entries_take_a_tag_and_give_it_back_in_one_batch() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "many.kdbx", |database| {
        for index in 0..50_000 {
            database
                .root_mut()
                .add_entry()
                .edit(|entry| entry.set_unprotected(fields::TITLE, format!("entry {index}")));
        }
    });
    let size = || std::fs::metadata(&path).expect("the file is there").len();
    let first = size();

    let mut vault = open(&path, BUILT_PASSWORD);
    let ids: Vec<_> = vault.tree().entries.iter().map(|row| row.id).collect();
    assert_eq!(ids.len(), 50_000);
    let changed = vault
        .tag_entries(&ids, "everything")
        .expect("the tag goes on");
    assert_eq!(changed, ids, "the answer is not every entry, in order");
    vault.save().expect("the database saves");
    drop(vault);

    let mut vault = open(&path, BUILT_PASSWORD);
    assert!(
        vault
            .tree()
            .entries
            .iter()
            .all(|row| row.tags == ["everything"])
    );
    assert!(ids.iter().all(|&id| vault.versions(id).len() == 1));
    vault
        .untag_entries(&changed, "everything")
        .expect("the tag comes off");
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&path, BUILT_PASSWORD);
    assert!(vault.tree().entries.iter().all(|row| row.tags.is_empty()));
    assert!(ids.iter().all(|&id| vault.versions(id).len() == 2));
    assert!(
        size() < first * 4,
        "{} bytes after starting at {first}",
        size()
    );
}

/// What went into the bin inside a deleted folder goes back, in a batch as on
/// its own, where that folder came from - not where its own last move says.
/// An entry deleted on its own goes back to its own folder in the same batch.
#[test]
fn what_went_in_with_a_folder_is_put_back_where_the_folder_came_from_in_a_batch() {
    let (_scratch, path) = cheap("with.kdbx");
    let (mut vault, [personal, work, banking], [alone, ..]) = three_folders(&path);
    let cards = vault
        .create_group(banking, "Cards")
        .expect("the folder is made");
    let visa = made(&mut vault, personal, "Visa");
    vault.move_entries(&[visa], cards).expect("it is filed");
    vault
        .delete_group(cards, Deletion::Bin)
        .expect("the folder goes to the bin");
    vault
        .delete_entries(&[(alone, Deletion::Bin)])
        .expect("the entry goes on its own");

    vault
        .put_back_entries(&[visa, alone])
        .expect("both come out");
    assert_eq!(vault.entry(visa).map(|entry| entry.group), Some(banking));
    assert_eq!(vault.entry(alone).map(|entry| entry.group), Some(personal));
    assert_ne!(Some(work), vault.entry(visa).map(|entry| entry.group));
}

/// Only what is in the bin comes out of it. One entry of a batch that is not
/// in the bin refuses the batch, and nothing in the bin moves.
#[test]
fn put_back_refuses_the_whole_batch_when_one_is_not_in_the_bin() {
    let (_scratch, path) = cheap("outside.kdbx");
    let (mut vault, _, [one, two, three]) = three_folders(&path);
    vault
        .delete_entries(&[(one, Deletion::Bin), (two, Deletion::Bin)])
        .expect("they go to the bin");

    let before = places(&vault);
    let edits = vault.edits();
    assert!(matches!(
        vault.put_back_entries(&[one, three, two]),
        Err(VaultError::NotInRecycleBin)
    ));
    assert_eq!(places(&vault), before);
    assert_eq!(vault.edits(), edits);
}

/// A tag goes on where it is missing, one version on each of those, and on
/// no other: the answer is those entries, in the order they were named, which
/// is what the undo takes it off again. Taking it off leaves the entry that
/// had it before with its tag.
#[test]
fn a_tag_goes_on_only_where_it_is_missing_one_version_each() {
    let (_scratch, path) = cheap("tag.kdbx");
    let (mut vault, _, [one, two, three]) = three_folders(&path);
    vault
        .set_tags(two, vec!["work".to_owned(), "x".to_owned()])
        .expect("the tags are written");
    vault.save().expect("the database saves");
    let versions = [one, two, three].map(|id| vault.versions(id).len());
    let modified = vault.entry(two).expect("it is there").times.modified;

    let changed = vault
        .tag_entries(&[three, two, one], "work")
        .expect("the tag goes on");
    assert_eq!(changed, [three, one]);
    assert_eq!(
        [one, two, three].map(|id| vault.versions(id).len()),
        [versions[0] + 1, versions[1], versions[2] + 1]
    );
    assert_eq!(
        vault.entry(two).expect("it is there").times.modified,
        modified
    );
    assert_eq!(tags(&vault, one), ["work"]);
    assert_eq!(tags(&vault, two), ["work", "x"], "a tag it had was doubled");

    vault
        .untag_entries(&changed, "work")
        .expect("the tag comes off");
    assert!(tags(&vault, one).is_empty() && tags(&vault, three).is_empty());
    assert_eq!(
        tags(&vault, two),
        ["work", "x"],
        "the undo took off a tag it never put on"
    );
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&path, BUILT_PASSWORD);
    assert_eq!(tags(&vault, two), ["work", "x"]);
    assert!(tags(&vault, one).is_empty());
}

/// A tag the format would give back as something else - split in two at a
/// semicolon, a comma or a tab, trimmed, dropped for being empty - is refused
/// for every entry with a sentence about tags, and one holding a character no
/// KeePass file can hold with the sentence about that. Nothing changes. The
/// rule is the one an entry's own tags are written by.
#[test]
fn a_tag_the_format_would_split_or_trim_is_refused_for_every_entry() {
    let (_scratch, path) = cheap("split.kdbx");
    let (mut vault, _, entries) = three_folders(&path);
    let edits = vault.edits();

    let split = ["a;b", "a,b", "a\tb", " a", "a ", "", "\n"];
    let unwritable = ["\u{0}", "\u{fffe}", "a\u{b}b"];
    for (tag, tagged) in split
        .iter()
        .map(|tag| (tag, true))
        .chain(unwritable.iter().map(|tag| (tag, false)))
    {
        let refused = |result: Result<(), VaultError>| match result {
            Err(VaultError::UnwritableTag) => tagged,
            Err(VaultError::UnwritableText) => !tagged,
            _ => false,
        };
        assert!(
            refused(vault.tag_entries(&entries, tag).map(drop)),
            "{tag:?} was put on, or refused for another reason"
        );
        assert!(
            refused(vault.set_tags(entries[0], vec![(*tag).to_owned()])),
            "{tag:?} was written on one entry, or refused for another reason"
        );
    }
    assert_eq!(
        VaultError::UnwritableTag.to_string(),
        "a tag cannot be empty, hold a semicolon, a comma or a tab, or begin or end with a space"
    );
    assert_eq!(vault.edits(), edits);
    for id in entries {
        assert!(tags(&vault, id).is_empty());
    }
}

/// A tag is the reader's text: markup, an ampersand the writer has to escape,
/// a right-to-left override and a megabyte of it go into the file and come
/// back exactly as they were. (An entity spelled out whole has a semicolon in
/// it, which the format reads as the end of a tag, and is refused.)
#[test]
fn a_tag_of_markup_entities_and_a_megabyte_survives_the_file() {
    let (_scratch, path) = cheap("odd.kdbx");
    let (mut vault, _, entries) = three_folders(&path);
    let long = "x".repeat(1 << 20);
    let odd = [
        "<script>alert(1)</script> & &lt",
        "\u{202e}gnihton",
        "]]>",
        long.as_str(),
    ];
    for tag in odd {
        vault.tag_entries(&entries, tag).expect("the tag goes on");
    }
    vault.save().expect("the database saves");
    drop(vault);

    let vault = open(&path, BUILT_PASSWORD);
    for id in entries {
        assert_eq!(tags(&vault, id), odd, "a tag came back changed");
    }
}

/// Taking a tag off takes every copy an entry has of it - a file can hold the
/// same tag twice - and writes a version only where it was. An empty tag is
/// no tag the window put on, and is refused.
#[test]
fn taking_a_tag_off_takes_every_copy_and_touches_nothing_else() {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let path = built(scratch.path(), "copies.kdbx", |database| {
        database.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "doubled");
            entry.tags = vec!["work".to_owned(), "x".to_owned(), "work".to_owned()];
        });
        database.root_mut().add_entry().edit(|entry| {
            entry.set_unprotected(fields::TITLE, "without");
            entry.tags = vec!["Work".to_owned()];
        });
    });
    let mut vault = open(&path, BUILT_PASSWORD);
    let doubled = entry_titled(&vault, "doubled").id;
    let without = entry_titled(&vault, "without").id;
    let versions = [doubled, without].map(|id| vault.versions(id).len());

    assert!(matches!(
        vault.untag_entries(&[doubled, without], ""),
        Err(VaultError::UnwritableTag)
    ));
    vault
        .untag_entries(&[doubled, without], "work")
        .expect("the tag comes off");
    assert_eq!(tags(&vault, doubled), ["x"]);
    assert_eq!(
        tags(&vault, without),
        ["Work"],
        "a tag of another case came off"
    );
    assert_eq!(
        [doubled, without].map(|id| vault.versions(id).len()),
        [versions[0] + 1, versions[1]]
    );
}

/// A tag put on and taken off a thousand times over three entries, saved every
/// hundred rounds: every entry's history stays inside the vault's limits, and
/// the file does not grow with the rounds.
#[test]
fn a_thousand_tags_on_and_off_keep_history_inside_its_limits() {
    let (_scratch, path) = cheap("rounds.kdbx");
    let (mut vault, _, entries) = three_folders(&path);
    vault.save().expect("the database saves");
    let mut first = None;

    for round in 0..1_000 {
        let changed = vault
            .tag_entries(&entries, "round")
            .expect("the tag goes on");
        assert_eq!(changed, entries);
        vault
            .untag_entries(&changed, "round")
            .expect("the tag comes off");
        if round % 100 == 99 {
            vault.save().expect("the database saves");
            let size = std::fs::metadata(&path).expect("the file is there").len();
            let first = *first.get_or_insert(size);
            assert!(
                size <= first + first / 4,
                "round {round}: {size} after {first}"
            );
            for id in entries {
                assert!(vault.versions(id).len() <= 10, "round {round}");
            }
        }
    }
    for id in entries {
        assert!(tags(&vault, id).is_empty());
    }
}

/// A tag put on here is a tag in KeePassXC, on every entry it went on, in the
/// format's one string.
#[test]
fn a_tag_added_in_coffer_is_a_tag_in_keepassxc() {
    let Some(tool) = support::keepassxc_cli() else {
        return;
    };
    let (_scratch, database) = support::scratch(RICH);
    let ids = {
        let mut vault = open(&database, SECRET);
        let ids = [
            entry_titled(&vault, "basic").id,
            entry_titled(&vault, "ssh key").id,
        ];
        vault.tag_entries(&ids, "batched").expect("the tag goes on");
        vault.save().expect("the database saves");
        ids
    };

    let written = library(&database, SECRET);
    for id in ids {
        let entry = written.entry(id).expect("the library finds it");
        assert!(entry.tags.iter().any(|tag| tag == "batched"));
    }
    let exported = support::export(&tool, &database, SECRET, None);
    let tagged = exported
        .split("<Entry>")
        .filter(|entry| {
            entry
                .split("<History>")
                .next()
                .is_some_and(|current| current.contains("batched"))
        })
        .count();
    assert_eq!(tagged, 2, "KeePassXC sees the tag on {tagged} entries");
}
