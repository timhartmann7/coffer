//! Changes to many entries at once, every one of them or none.
//!
//! The window acts on a set of chosen entries with one command, and the reader
//! is offered one undo for all of it. That only means something if the change
//! is whole: a batch that went through nine entries and stopped at the tenth
//! would leave a vault in a state nobody chose, and an undo about ten entries.
//! So every entry a batch names is checked before any of them is changed, and
//! the one step that can still say no once the checks are passed - taking the
//! files of the entries being erased out of the pool - is worked out for all of
//! them at once and runs first.

use std::collections::HashSet;

use keepass::db::{EntryId, Times};

use super::Vault;
use crate::bin::{Bin, Standings};
use crate::error::VaultError;
use crate::history;
use crate::model::Deletion;
use crate::text;

impl Vault {
    /// Deletes entries, every one of them or none.
    ///
    /// As with a folder: to the recycle bin when the database keeps one, and
    /// out of the file and into `DeletedObjects` for an entry that is in the
    /// bin already, or in a database that keeps none. Which of the two happens
    /// to each is what its [`Entry::deletion`][crate::model::Entry::deletion]
    /// said it would.
    ///
    /// A move to the bin is not an edit. It writes no version and records no
    /// deletion, and the entry keeps the folder it came out of as its
    /// `PreviousParentGroup`, which is where [`Vault::put_back_entries`] takes
    /// it.
    ///
    /// Each entry comes with the deletion the reader was shown for it. One
    /// whose deletion is now the other - its folder went into the bin first,
    /// or a reload brought in a vault that keeps no bin - refuses the whole
    /// batch with [`VaultError::DeletionChanged`], for the reasons
    /// [`Vault::delete_group`] gives, and so does an entry that is not there:
    /// nothing is deleted. One named twice goes once; named twice with two
    /// answers, one of them is not what happens, and the batch is refused.
    ///
    /// The erasures run before any move into the bin, because taking their
    /// files out of the pool is the one step that can still refuse - a version
    /// of some entry holding a number in place - and it is worked out for every
    /// entry being erased together, so it refuses before anything has changed.
    pub fn delete_entries(&mut self, shown: &[(EntryId, Deletion)]) -> Result<(), VaultError> {
        self.writable()?;
        let bin = Bin::of(&self.database);

        let mut seen = HashSet::new();
        let (mut erasing, mut binning) = (Vec::new(), Vec::new());
        {
            let mut standings = Standings::new(&bin, &self.database);
            for &(id, shown) in shown {
                let group = self
                    .database
                    .entry(id)
                    .ok_or(VaultError::NoSuchEntry)?
                    .parent()
                    .id();
                let deletion = bin.deletion(standings.of(group));
                if deletion != shown {
                    return Err(VaultError::DeletionChanged);
                }
                if !seen.insert(id) {
                    continue;
                }
                match deletion {
                    Deletion::Forever => erasing.push(id),
                    Deletion::Bin => binning.push(id),
                }
            }
        }

        if !erasing.is_empty() {
            self.erase_entries(&erasing)?;
        }
        if !binning.is_empty() {
            let into = self.bin(bin.id());
            // Nothing here can be refused any more: every entry was found
            // above, the bin is there, and nothing has moved since.
            for id in binning {
                self.relocate_entry(id, into)?;
            }
        }
        Ok(())
    }

    /// Takes entries out of the recycle bin, every one of them or none, and
    /// puts each back where it was: the pane's Put back is a batch of one.
    ///
    /// Back into the folder it was deleted from, when that folder is still
    /// somewhere to go. One that has gone, one that is in the bin itself, and
    /// an entry another client put in the bin without saying where from all
    /// send it to the top of the vault instead: put back somewhere is better
    /// than left behind. An entry that went in with a deleted folder goes
    /// where that folder came from ([`Binned::from`][crate::model::Binned::from]).
    ///
    /// Moving is not an edit, so no version is written, and the folder it
    /// leaves becomes its `PreviousParentGroup` the way every move makes it.
    ///
    /// An entry that is not there, or not in the bin, refuses the whole batch,
    /// and nothing moves. One named twice goes once.
    pub fn put_back_entries(&mut self, ids: &[EntryId]) -> Result<(), VaultError> {
        self.writable()?;
        let bin = Bin::of(&self.database);

        let mut seen = HashSet::new();
        let mut going = Vec::new();
        {
            let mut standings = Standings::new(&bin, &self.database);
            for &id in ids {
                let entry = self.database.entry(id).ok_or(VaultError::NoSuchEntry)?;
                let binned = bin.binned(
                    &self.database,
                    standings.of(entry.parent().id()),
                    &entry.times,
                    entry.previous_parent().map(|previous| previous.id()),
                );
                let into = self.back(binned)?;
                if seen.insert(id) {
                    going.push((id, into));
                }
            }
        }

        // Every entry and every folder was found above, and nothing has
        // changed since.
        for (id, into) in going {
            self.relocate_entry(id, into)?;
        }
        Ok(())
    }

    /// Puts a tag on every entry named that does not have it yet, and answers
    /// with those, in the order they were named.
    ///
    /// A tag is an edit, unlike a move: each entry it goes on keeps its
    /// previous state as a version, and its modification time moves. An entry
    /// that already has the tag gets neither, and is not among those answered,
    /// so the undo - [`Vault::untag_entries`] with exactly those - leaves it
    /// with the tag it had. A tag the format would not give back as it was
    /// written is refused for every entry (see `text::tag`), and so is the
    /// whole batch when one entry is not there.
    pub fn tag_entries(&mut self, ids: &[EntryId], tag: &str) -> Result<Vec<EntryId>, VaultError> {
        self.writable()?;
        text::tag(tag)?;

        let mut seen = HashSet::new();
        let mut missing = Vec::new();
        for &id in ids {
            let entry = self.database.entry(id).ok_or(VaultError::NoSuchEntry)?;
            if seen.insert(id) && !entry.tags.iter().any(|held| held == tag) {
                missing.push(id);
            }
        }

        for &id in &missing {
            history::edit(&mut self.database, id, |entry| {
                entry.tags.push(tag.to_owned());
                entry.times.last_modification = Some(Times::now());
            });
        }
        if !missing.is_empty() {
            self.touched();
        }
        Ok(missing)
    }

    /// Takes a tag off every entry named that has it, every copy of it, with a
    /// version on each it came off and on no other. The undo of
    /// [`Vault::tag_entries`].
    ///
    /// No rule of spelling but one: it is a tag the file holds, however it was
    /// written there, and only an empty one is refused, which is no tag the
    /// window ever put on. An entry that is not there refuses the whole batch.
    pub fn untag_entries(&mut self, ids: &[EntryId], tag: &str) -> Result<(), VaultError> {
        self.writable()?;
        if tag.is_empty() {
            return Err(VaultError::UnwritableTag);
        }

        let mut seen = HashSet::new();
        let mut holding = Vec::new();
        for &id in ids {
            let entry = self.database.entry(id).ok_or(VaultError::NoSuchEntry)?;
            if seen.insert(id) && entry.tags.iter().any(|held| held == tag) {
                holding.push(id);
            }
        }

        for &id in &holding {
            history::edit(&mut self.database, id, |entry| {
                entry.tags.retain(|held| held != tag);
                entry.times.last_modification = Some(Times::now());
            });
        }
        if !holding.is_empty() {
            self.touched();
        }
        Ok(())
    }
}
