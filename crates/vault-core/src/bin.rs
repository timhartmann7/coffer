//! The recycle bin: what deleting something does, and what is known about what
//! the bin holds.
//!
//! One place for both, because they are one question asked from two sides. The
//! window asks it before a deletion, to say whether the deletion can be taken
//! back, and the deletion asks it to decide where to go. Two answers worked out
//! apart would sooner or later be a window promising the bin to something that
//! went out of the file.

use chrono::NaiveDateTime;
use keepass::Database;
use keepass::db::{GroupId, Times};

use crate::model::{Binned, Deletion};

/// The database's recycle bin, as a deletion and a put back see it.
pub(crate) struct Bin {
    /// The group the database names as its bin, when that group is there.
    id: Option<GroupId>,
    /// Whether a deletion goes to the bin at all. Absent is KeePass's own
    /// default, which keeps one.
    keeps: bool,
    /// Every folder the bin is inside, from its parent up to the top.
    around: Vec<GroupId>,
}

/// Where a folder stands with respect to the recycle bin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Standing {
    /// Somewhere in the vault the reader works in.
    Outside,
    /// The bin itself. What it holds directly went in on its own.
    Bin,
    /// Inside a folder that went into the bin, and so in the bin since that
    /// folder went in. `folder` is the one the bin holds: the folder that was
    /// deleted, whatever depth this is below it.
    Within {
        since: Option<NaiveDateTime>,
        folder: GroupId,
    },
}

impl Standing {
    /// Whether this is the bin or somewhere inside it.
    pub(crate) fn binned(self) -> bool {
        self != Standing::Outside
    }
}

impl Bin {
    pub(crate) fn of(database: &Database) -> Bin {
        let id = database.recycle_bin().map(|group| group.id());
        Bin {
            id,
            keeps: database.meta.recyclebin_enabled != Some(false),
            around: id.map(|bin| above(database, bin)).unwrap_or_default(),
        }
    }

    pub(crate) fn id(&self) -> Option<GroupId> {
        self.id
    }

    /// Where a folder stands, given where the folder holding it stands.
    ///
    /// The one step every answer here is built from. A walk of the tree takes
    /// it once per folder on the way down, and [`Bin::standing`] takes the
    /// same steps for a single folder, so the two cannot disagree.
    pub(crate) fn enter(&self, holder: Standing, group: GroupId, times: &Times) -> Standing {
        match holder {
            Standing::Outside if Some(group) == self.id => Standing::Bin,
            Standing::Outside => Standing::Outside,
            Standing::Bin => Standing::Within {
                since: times.location_changed,
                folder: group,
            },
            within @ Standing::Within { .. } => within,
        }
    }

    /// Where one folder stands, stepped down to from the top of the vault.
    pub(crate) fn standing(&self, database: &Database, group: GroupId) -> Standing {
        let mut chain = above(database, group);
        chain.reverse();
        chain.push(group);

        chain.iter().fold(Standing::Outside, |holder, id| {
            database
                .group(*id)
                .map_or(holder, |found| self.enter(holder, *id, &found.times))
        })
    }

    /// What deleting something that stands here does. An entry stands where
    /// its folder does.
    pub(crate) fn deletion(&self, standing: Standing) -> Deletion {
        if !self.keeps || standing.binned() {
            Deletion::Forever
        } else {
            Deletion::Bin
        }
    }

    /// What deleting a folder does, given where it stands itself.
    ///
    /// A folder the bin is inside cannot go into it: a folder cannot contain
    /// itself, and moving it there would take the bin along. This is asked of
    /// a folder and never of an entry. The top of the vault is where every
    /// entry Coffer makes lands and where the bin usually sits, so asking it of
    /// the folder an entry comes out of quietly erased entries at the top
    /// instead of binning them.
    pub(crate) fn group_deletion(&self, group: GroupId, standing: Standing) -> Deletion {
        if self.around.contains(&group) {
            Deletion::Forever
        } else {
            self.deletion(standing)
        }
    }

    /// What is known about something held by a folder standing at `holder`:
    /// nothing when that folder is outside the bin.
    ///
    /// `previous` is the folder the thing was in before its last move, which
    /// is what the format keeps as `PreviousParentGroup`. For something that
    /// went into the bin on its own it is where putting it back takes it, as
    /// long as it is still somewhere to go: a folder in the bin is not, and
    /// neither is one that has gone.
    ///
    /// Something that went in with a deleted folder was not moved by the
    /// deletion, so its own `PreviousParentGroup` is about some older move -
    /// KeePass and KeePassXC write one on every drag between folders - and it
    /// was never in that folder when it was deleted. It goes back where the
    /// folder that took it in came from, which is where it would be had the
    /// folder been put back whole.
    pub(crate) fn binned(
        &self,
        database: &Database,
        holder: Standing,
        times: &Times,
        previous: Option<GroupId>,
    ) -> Option<Binned> {
        let (since, within, previous) = match holder {
            Standing::Outside => return None,
            Standing::Bin => (times.location_changed, None, previous),
            Standing::Within { since, folder } => (
                since,
                Some(folder),
                database
                    .group(folder)
                    .and_then(|deleted| deleted.previous_parent().map(|parent| parent.id())),
            ),
        };
        let from = previous.filter(|group| {
            database.group(*group).is_some() && !self.standing(database, *group).binned()
        });
        Some(Binned {
            since,
            within,
            from,
        })
    }
}

/// The folders `group` is inside, from its own parent up to the top.
///
/// The library builds every parent out of the nesting of the file and refuses
/// a move that would make a cycle, so this walk ends at the top.
fn above(database: &Database, group: GroupId) -> Vec<GroupId> {
    let parent = |id: GroupId| {
        database
            .group(id)
            .and_then(|found| found.parent().map(|parent| parent.id()))
    };

    let mut found = Vec::new();
    let mut here = parent(group);
    while let Some(id) = here {
        found.push(id);
        here = parent(id);
    }
    found
}
