//! When a previous version of an entry is kept, and which ones survive a save.
//!
//! The format keeps previous versions inside each entry and states its limits in
//! `Meta`. The library Coffer is built on writes versions but never prunes them,
//! and writes one on every edit whether or not the edit changed anything. Both
//! rules live here.

use keepass::Database;
use keepass::db::{Entry, EntryId, History, Icon, Times};

use crate::error::VaultError;

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
/// Returns whether the entry was there to edit.
///
/// The version is dropped again when the edit turned out to change nothing,
/// along with the modification time the library stamps on the way through: an
/// edit that changed nothing did not happen.
pub(crate) fn edit(
    database: &mut Database,
    id: EntryId,
    change: impl FnOnce(&mut keepass::db::EntryTrack<'_>),
) -> bool {
    let Some(mut entry) = database.entry_mut(id) else {
        return false;
    };

    entry.edit_tracking(change);
    settle(database, id);
    true
}

/// Puts the version the library just wrote where it belongs, or takes it away
/// again.
///
/// `History::add_entry` inserts at the front, and the front is where the oldest
/// version sits in a file KeePassXC wrote. Left alone, the newest version would
/// be sitting in the oldest one's place, where pruning throws it away first and
/// a version list shows it in the wrong order.
fn settle(database: &mut Database, id: EntryId) {
    let Some(mut entry) = database.entry_mut(id) else {
        return;
    };

    // Moved out rather than borrowed, so that the comparison below copies the
    // entry without copying every version of it.
    let Some(history) = entry.history.take() else {
        return;
    };
    let mut versions: Vec<Entry> = history.get_entries().clone();

    if versions.is_empty() {
        entry.history = Some(history);
        return;
    }
    let written = versions.remove(0);

    if same_content(&entry, &written) {
        entry.times = written.times.clone();
    } else {
        versions.push(written);
    }

    entry.history = Some(rebuild(versions));
}

/// Whether two versions of an entry hold the same thing.
///
/// The comparison is the library's own derived equality with the bookkeeping
/// timestamps taken out, so a field the library gains in a future release is
/// compared without anybody having to remember to add it here. That matters
/// more than the cost of the clone: a field left out of this comparison is a
/// version silently not kept.
///
/// Whether an entry expires, and when, stays in the comparison. When it was
/// made, touched or looked at is not something anybody chose; an expiry date
/// is, and an edit that set one is an edit. Taking it out is how a restore
/// whose only change was an expiry date came back reporting success and doing
/// nothing.
///
/// Neither side carries history at this point - the live entry's was moved out
/// by the caller, and `History::add_entry` strips it from a version on the way
/// in - so nothing here copies a version list.
fn same_content(current: &Entry, previous: &Entry) -> bool {
    fn content_only(times: &mut Times) {
        let expires = times.expires;
        let expiry = times.expiry;
        *times = Times::default();
        times.expires = expires;
        times.expiry = expiry;
    }

    let mut current = current.clone();
    let mut previous = previous.clone();

    content_only(&mut current.times);
    content_only(&mut previous.times);

    current == previous
}

/// Reads one previous version of an entry.
///
/// The index is the position the version holds in the file, which is what a
/// version list hands back and what addresses the same version however the list
/// was ordered.
pub(crate) fn version(database: &Database, id: EntryId, index: usize) -> Option<Entry> {
    let entry = database.entry(id)?;
    let version = entry.historical(index)?;
    Some((*version).clone())
}

/// Makes a previous version the current state of its entry, keeping the state
/// it replaces as a version of its own.
///
/// The files on the entry are not touched. A file belongs to the pool the whole
/// database shares, a version names it by a number, and the library gives no way
/// to point an entry at a number of its own choosing, so a restore that moved
/// files would have to copy their bytes. What a restore brings back is the
/// fields, the tags, the notes, the colours, the icon and the expiry date.
pub(crate) fn restore(
    database: &mut Database,
    id: EntryId,
    index: usize,
) -> Result<(), VaultError> {
    let wanted = version(database, id, index).ok_or(VaultError::NoSuchVersion)?;

    // The icon is read out before the rest, because it is not a field anybody
    // can assign: an entry holds a back-reference in the custom icon it uses,
    // and only the three setters below keep that right.
    let icon = wanted.icon().cloned();

    let restored = edit(database, id, move |entry| {
        entry.fields = wanted.fields;
        entry.tags = wanted.tags;
        entry.custom_data = wanted.custom_data;
        entry.autotype = wanted.autotype;
        entry.foreground_color = wanted.foreground_color;
        entry.background_color = wanted.background_color;
        entry.override_url = wanted.override_url;
        entry.quality_check = wanted.quality_check;
        entry.times.expires = wanted.times.expires;
        entry.times.expiry = wanted.times.expiry;
        entry.times.last_modification = Some(Times::now());

        // A custom icon somebody deleted meanwhile leaves the entry with none
        // rather than with a reference to nothing.
        match icon {
            None => entry.set_icon_none(),
            Some(Icon::BuiltIn(icon)) => entry.set_icon_builtin(icon),
            Some(Icon::Custom(icon)) => {
                if entry.set_icon_custom(icon).is_err() {
                    entry.set_icon_none();
                }
            }
        }
    });

    if restored {
        Ok(())
    } else {
        Err(VaultError::NoSuchEntry)
    }
}

/// Drops one previous version.
pub(crate) fn forget(database: &mut Database, id: EntryId, index: usize) -> Result<(), VaultError> {
    let mut entry = database.entry_mut(id).ok_or(VaultError::NoSuchEntry)?;
    let history = entry.history.take().unwrap_or_default();

    let mut versions = history.get_entries().clone();
    if index >= versions.len() {
        entry.history = Some(history);
        return Err(VaultError::NoSuchVersion);
    }
    versions.remove(index);

    entry.history = Some(rebuild(versions));
    Ok(())
}

/// Drops every previous version, leaving the entry as it is now.
pub(crate) fn clear(database: &mut Database, id: EntryId) -> Result<(), VaultError> {
    let mut entry = database.entry_mut(id).ok_or(VaultError::NoSuchEntry)?;
    entry.history = Some(History::default());
    Ok(())
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

        let mut weighed = Vec::with_capacity(history.get_entries().len());
        for index in 0..history.get_entries().len() {
            // A version that cannot be weighed is a version this function does
            // not understand, and dropping what you do not understand is how
            // history goes missing. Leave the entry exactly as it is.
            let (Some(version), Some(entry)) =
                (entry.historical(index), history.get_entries().get(index))
            else {
                return;
            };
            weighed.push((entry.clone(), weigh(&version)));
        }
        weighed
    };

    // The vector is oldest first: that is the order KeePassXC writes and the
    // order `settle` restores after every edit. Sorting by time only corrects a
    // file that arrived out of order, and being stable it leaves versions that
    // share a timestamp exactly where they were, so the newest of a tied pair is
    // still the last one and is never the first to go.
    //
    // A version with no modification time sorts last rather than first. Nothing
    // is known about when it was written, and a version nobody can date is the
    // wrong thing to throw away first.
    versions.sort_by_key(|(version, _)| {
        (
            version.times.last_modification.is_none(),
            version.times.last_modification,
        )
    });

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
