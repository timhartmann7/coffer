//! When a previous version of an entry is kept, and which ones survive a save.
//!
//! The format keeps previous versions inside each entry and states its limits in
//! `Meta`. The library Coffer is built on writes versions but never prunes them,
//! and writes one on every edit whether or not the edit changed anything. Both
//! rules live here.

use keepass::Database;
use keepass::db::{Entry, EntryId, History, Times};

/// What KeePassXC uses when a database states no limit of its own.
const DEFAULT_MAX_ITEMS: usize = 10;
const DEFAULT_MAX_SIZE: u64 = 6 * 1024 * 1024;

/// The limits a database puts on entry history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Limits {
    /// How many versions an entry may keep, or `None` for no limit.
    items: Option<usize>,
    /// How many bytes of versions an entry may keep, or `None` for no limit.
    size: Option<u64>,
}

impl Limits {
    /// Reads the limits out of a database. A negative limit means no limit, the
    /// way KeePass writes "never"; an absent one falls back to KeePassXC's
    /// default rather than to no limit at all, because a database with no
    /// opinion should not grow without bound.
    pub(crate) fn of(database: &Database) -> Limits {
        Limits {
            items: bound(database.meta.history_max_items, DEFAULT_MAX_ITEMS as isize)
                .map(|value| value as usize),
            size: bound(database.meta.history_max_size, DEFAULT_MAX_SIZE as isize)
                .map(|value| value as u64),
        }
    }
}

fn bound(stated: Option<isize>, default: isize) -> Option<isize> {
    match stated.unwrap_or(default) {
        negative if negative < 0 => None,
        limit => Some(limit),
    }
}

/// Edits an entry, keeping its previous state as a version.
///
/// The version is dropped again when the edit turned out to change nothing,
/// along with the modification time the library stamps on the way through: an
/// edit that changed nothing did not happen.
pub(crate) fn edit(
    database: &mut Database,
    id: EntryId,
    change: impl FnOnce(&mut keepass::db::EntryTrack<'_>),
) {
    let Some(mut entry) = database.entry_mut(id) else {
        return;
    };

    entry.edit_tracking(change);

    discard_unchanged(database, id);
}

/// Removes the version the library just wrote if it is the same entry again.
fn discard_unchanged(database: &mut Database, id: EntryId) {
    let Some(entry) = database.entry(id) else {
        return;
    };
    let Some(history) = entry.history.as_ref() else {
        return;
    };
    // `History::add_entry` inserts at the front, so the version just written is
    // the first one.
    let Some(previous) = history.get_entries().first() else {
        return;
    };

    if !same_content(&entry, previous) {
        return;
    }

    let restored = previous.times.clone();
    let kept: Vec<Entry> = history.get_entries().iter().skip(1).cloned().collect();

    let Some(mut entry) = database.entry_mut(id) else {
        return;
    };
    entry.times = restored;
    entry.history = Some(rebuild(kept));
}

/// Whether two versions of an entry hold the same thing.
///
/// The comparison is the library's own derived equality with the timestamps and
/// the nested history taken out, so a field the library gains in a future
/// release is compared without anybody having to remember to add it here. That
/// matters more than the cost of the two clones: a field left out of this
/// comparison is a version silently not kept.
fn same_content(current: &Entry, previous: &Entry) -> bool {
    let mut current = current.clone();
    let mut previous = previous.clone();

    current.times = Times::default();
    previous.times = Times::default();
    current.history = None;
    previous.history = None;

    current == previous
}

/// Brings every entry's history inside the database's limits, oldest versions
/// going first.
pub(crate) fn prune_all(database: &mut Database, limits: Limits) {
    let ids: Vec<EntryId> = database
        .iter_all_entries()
        .map(|entry| entry.id())
        .collect();

    for id in ids {
        prune(database, id, limits);
    }
}

fn prune(database: &mut Database, id: EntryId, limits: Limits) {
    // Each version is weighed while the database is still borrowed, because the
    // bytes of an attachment live in the database rather than in the version
    // that refers to them.
    let mut versions: Vec<(Entry, u64)> = {
        let Some(entry) = database.entry(id) else {
            return;
        };
        let Some(history) = entry.history.as_ref() else {
            return;
        };

        (0..history.get_entries().len())
            .filter_map(|index| {
                let version = entry.historical(index)?;
                let weight = weigh(&version);
                Some((history.get_entries().get(index)?.clone(), weight))
            })
            .collect()
    };

    // Position in the vector is not a reliable order: a file written by
    // KeePassXC lists versions oldest first, while the library adds new ones at
    // the front. Time decides, and equal times keep the order they were in.
    versions.sort_by_key(|(version, _)| version.times.last_modification);

    let mut drop_count = match limits.items {
        Some(max) => versions.len().saturating_sub(max),
        None => 0,
    };

    if let Some(max) = limits.size {
        let mut total: u64 = versions
            .iter()
            .skip(drop_count)
            .map(|(_, weight)| weight)
            .sum();
        while total > max {
            let Some((_, weight)) = versions.get(drop_count) else {
                break;
            };
            total -= weight;
            drop_count += 1;
        }
    }

    versions.drain(..drop_count);

    let kept: Vec<Entry> = versions.into_iter().map(|(version, _)| version).collect();

    let Some(mut entry) = database.entry_mut(id) else {
        return;
    };
    entry.history = Some(rebuild(kept));
}

/// What one version costs, counted the way KeePassXC counts it: the length of
/// the field values plus the size of the attachments the version refers to.
/// Attachments are stored once and shared between versions, so this overstates
/// the bytes on disk; it decides how many versions are kept, not whether the
/// file is valid.
fn weigh(version: &keepass::db::EntryRef<'_>) -> u64 {
    let fields: usize = version
        .fields
        .iter()
        .map(|(name, value)| name.len() + value.get().len())
        .sum();

    let attachments: usize = version
        .attachments()
        .map(|attachment| attachment.data.get().len())
        .sum();

    (fields + attachments) as u64
}

/// Puts a list of versions back, oldest first.
///
/// `History::add_entry` is the only way in from outside the library and it
/// inserts at the front, so the list goes in backwards to come out forwards.
/// Oldest first is the order KeePassXC writes, which keeps a database Coffer
/// saved looking like one KeePassXC saved.
fn rebuild(versions: Vec<Entry>) -> History {
    let mut history = History::default();
    for version in versions.into_iter().rev() {
        history.add_entry(version);
    }
    history
}
