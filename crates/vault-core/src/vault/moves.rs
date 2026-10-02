//! Moving entries and folders between folders, and taking a move back.
//!
//! A move between folders is the deletion's own move with checks in front of
//! it. The recycle bin is closed at both ends: nothing in it moves, because
//! putting back is the way out and says where to, and nothing moves into it,
//! because deleting is the way in and says first whether it can be undone.
//! Everything a move names is checked before anything moves, so a batch the
//! window chose moves whole or not at all, and the one undo it is offered is
//! about all of it.

use std::collections::HashSet;

use keepass::db::{EntryId, GroupId};

use super::Vault;
use crate::bin::{Bin, Standings};
use crate::error::VaultError;
use crate::model::Move;

impl Vault {
    /// Moves entries into a folder, every one of them or none, and answers
    /// with each that changed folder and the folder it left, in the order they
    /// were given.
    ///
    /// A move is not an edit. No version is written, the modification time
    /// stays, and nothing goes into `DeletedObjects`; `LocationChanged` is the
    /// moment of the move and `PreviousParentGroup` the folder left, which is
    /// what KeePass and KeePassXC write when an entry is dragged between
    /// folders.
    ///
    /// Every entry is checked before any of them moves, so a batch holding one
    /// this refuses moves nothing: an entry that is not there, one in the
    /// recycle bin ([`VaultError::InRecycleBin`]: putting back is the way out),
    /// a folder that is not there, or one that is the bin or inside it
    /// ([`VaultError::IntoRecycleBin`]: deleting is the way in).
    ///
    /// An entry already in `into` stays where it is, in its place in the
    /// folder, and is not among those answered: the library would take it to
    /// the end of the folder and write the folder as the one it came from. A
    /// batch with nothing to move changes nothing and leaves nothing to save.
    /// One named twice moves once.
    ///
    /// The rest go in the order given, each to the end of the folder.
    pub fn move_entries(
        &mut self,
        ids: &[EntryId],
        into: GroupId,
    ) -> Result<Vec<Move>, VaultError> {
        self.writable()?;
        let bin = Bin::of(&self.database);
        self.destination(&bin, into)?;

        let mut standings = Standings::new(&bin, &self.database);
        let mut seen = HashSet::new();
        let mut moving = Vec::new();
        for &id in ids {
            let from = self
                .database
                .entry(id)
                .ok_or(VaultError::NoSuchEntry)?
                .parent()
                .id();
            if standings.of(from).binned() {
                return Err(VaultError::InRecycleBin);
            }
            if from != into && seen.insert(id) {
                moving.push(Move { entry: id, from });
            }
        }

        // In the order given, which costs the length of the folder left for
        // each entry taken out of it: see `docs/vault-core.md`. Nothing here
        // can be refused any more, because every entry and the folder were
        // found above and nothing has changed since.
        for moved in &moving {
            self.relocate_entry(moved.entry, into)?;
        }
        Ok(moving)
    }

    /// Moves a folder, with everything in it, into another folder.
    ///
    /// Not an edit, on the same terms as [`Vault::move_entries`]. Refused: the
    /// top of the vault, a folder that is not there, the recycle bin or
    /// anything in it, a destination that is the bin or inside it, and the
    /// folder itself or any folder under it
    /// ([`VaultError::CannotMoveIntoItself`]). A folder the bin is inside may
    /// move, and takes the bin along: it is still the bin, wherever it is.
    ///
    /// A folder already in `into` stays where it is, and nothing changes.
    pub fn move_group(&mut self, id: GroupId, into: GroupId) -> Result<(), VaultError> {
        self.writable()?;
        if id == self.database.root().id() {
            return Err(VaultError::CannotMoveRoot);
        }
        let parent = self
            .database
            .group(id)
            .ok_or(VaultError::NoSuchGroup)?
            .parent()
            .map(|parent| parent.id());

        let bin = Bin::of(&self.database);
        if bin.standing(&self.database, id).binned() {
            return Err(VaultError::InRecycleBin);
        }
        self.destination(&bin, into)?;
        if parent == Some(into) {
            return Ok(());
        }
        self.relocate_group(id, into)
    }

    /// Takes a move back: every entry [`Vault::move_entries`] answered goes
    /// back out of `into`, the folder it was moved into, to the folder it was
    /// moved from - every one of them or none.
    ///
    /// Only while the file still says so of each: it is in `into`, and its
    /// `PreviousParentGroup`, which every move writes, is the folder the move
    /// took it from. An entry moved on since, or moved away and back again by
    /// way of another folder, says something else, and taking the move back
    /// then would undo what was done after it. Refused with
    /// [`VaultError::MoveSuperseded`], moving nothing, for that, and when the
    /// folder it came from or `into` itself has gone or is in the recycle bin,
    /// which a move is not let into or out of either, and when an entry has
    /// gone out of the file since: that too was done after the move.
    ///
    /// Each goes to the end of the folder it came from, because the library
    /// has no way to put one back where it stood. One named twice goes once.
    pub fn move_entries_back(&mut self, moved: &[Move], into: GroupId) -> Result<(), VaultError> {
        self.writable()?;
        let bin = Bin::of(&self.database);
        let mut standings = Standings::new(&bin, &self.database);
        if self.database.group(into).is_none() || standings.of(into).binned() {
            return Err(VaultError::MoveSuperseded);
        }

        let mut seen = HashSet::new();
        let mut going = Vec::new();
        for &Move { entry: id, from } in moved {
            let entry = self.database.entry(id).ok_or(VaultError::MoveSuperseded)?;
            // The library answers only for a folder that is still there, so a
            // folder erased since is the same answer as one never written. A
            // folder written as the one the entry left for itself is another
            // client's move to where it already was, and there is nowhere to
            // take that back to.
            let last = entry.previous_parent().map(|previous| previous.id());
            if entry.parent().id() != into
                || last != Some(from)
                || from == into
                || standings.of(from).binned()
            {
                return Err(VaultError::MoveSuperseded);
            }
            if seen.insert(id) {
                going.push((id, from));
            }
        }

        // Nothing can be refused from here: every entry and every folder were
        // found above, and nothing has changed since.
        for (id, from) in going {
            self.relocate_entry(id, from)?;
        }
        Ok(())
    }

    /// Takes a folder's move back: the folder goes back out of `into`, where
    /// [`Vault::move_group`] put it, to `from`, the folder it left.
    ///
    /// On the terms [`Vault::move_entries_back`] takes entries back: only while
    /// the folder is still in `into` and its `PreviousParentGroup` is still
    /// `from`, and neither of the two is in the recycle bin. A folder moved on
    /// since, gone since, or one `from` has been moved inside since - the move
    /// back would put it inside itself - is something done after the move, and
    /// is refused with [`VaultError::MoveSuperseded`], moving nothing. It goes
    /// to the end of `from`.
    pub fn move_group_back(
        &mut self,
        id: GroupId,
        from: GroupId,
        into: GroupId,
    ) -> Result<(), VaultError> {
        self.writable()?;
        let group = self.database.group(id).ok_or(VaultError::MoveSuperseded)?;
        let parent = group.parent().map(|parent| parent.id());
        // As for an entry, the library answers only for a folder that is still
        // there.
        let last = group.previous_parent().map(|previous| previous.id());
        let bin = Bin::of(&self.database);
        // In `into`, the folder is binned exactly when `into` is, or when it is
        // the bin itself, which no move of Coffer's ever took anywhere.
        if parent != Some(into)
            || last != Some(from)
            || from == into
            || bin.standing(&self.database, id).binned()
            || bin.standing(&self.database, from).binned()
        {
            return Err(VaultError::MoveSuperseded);
        }

        // The library walks up from `from` before it moves anything, so a
        // folder that would land inside itself is refused with nothing moved.
        match self.relocate_group(id, from) {
            Err(VaultError::CannotMoveIntoItself) => Err(VaultError::MoveSuperseded),
            done => done,
        }
    }
}
