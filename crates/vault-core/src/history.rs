//! When a previous version of an entry is kept, and which ones survive a save.
//!
//! The format keeps previous versions inside each entry and states its limits in
//! `Meta`. The library Coffer is built on writes versions but never prunes them,
//! and writes one on every edit whether or not the edit changed anything. Both
//! rules live here.

use chrono::NaiveDateTime;
use keepass::Database;
use keepass::db::{Entry, EntryId, EntryRef, History, Times};

use crate::content::Content;
use crate::error::VaultError;

/// What KeePassXC uses when a database states no limit of its own, and what a
/// database Coffer creates writes into its own header.
pub(crate) const DEFAULT_MAX_ITEMS: isize = 10;
pub(crate) const DEFAULT_MAX_SIZE: isize = 6 * 1024 * 1024;

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
            items: bound(database.meta.history_max_items, DEFAULT_MAX_ITEMS)
                .map(|value| value as usize),
            size: bound(database.meta.history_max_size, DEFAULT_MAX_SIZE).map(|value| value as u64),
        }
    }
}

fn bound(stated: Option<isize>, default: isize) -> Option<isize> {
    match stated.unwrap_or(default) {
        negative if negative < 0 => None,
        limit => Some(limit),
    }
}

/// Where a version falls in time: by when it was last changed, and a version
/// nobody dated after every version somebody did.
///
/// One rule for the three places that put versions in order - the list the
/// window reads, the pruning on save, and the question of which version is the
/// newest - because two of them disagreeing is a list that shows one version as
/// the newest while the engine acts on another.
pub(crate) fn age(modified: Option<NaiveDateTime>) -> (bool, Option<NaiveDateTime>) {
    (modified.is_none(), modified)
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

/// Makes a previous version the current state of its entry, keeping the state
/// it replaces as a version of its own.
///
/// What a restore brings back is what [`Content`] lists: the fields, the tags,
/// the notes, the colours, the icon and the expiry date, and the rest of what
/// an entry holds rather than where it sits. The files on the entry are not
/// touched. A file belongs to the pool the whole database shares, a version
/// names it by a number, and the library gives no way to point an entry at a
/// number of its own choosing, so a restore that moved files would have to
/// copy their bytes.
pub(crate) fn restore(
    database: &mut Database,
    id: EntryId,
    index: usize,
) -> Result<(), VaultError> {
    let wanted = {
        let entry = database.entry(id).ok_or(VaultError::NoSuchVersion)?;
        let version = entry.historical(index).ok_or(VaultError::NoSuchVersion)?;
        Content::of(&version)
    };

    let restored = edit(database, id, move |entry| {
        wanted.put(&mut entry.as_mut());
        entry.times.last_modification = Some(Times::now());
    });

    if restored {
        Ok(())
    } else {
        Err(VaultError::NoSuchEntry)
    }
}

/// Which version holds the entry as it was before `field` came off it, when
/// taking it off is the last thing that happened to the entry.
///
/// Only the newest version can be that one, in the order a version list is read
/// in, and it is that one only when it is the entry as it stands now with that
/// one field put back. Any other version is one whose restore takes back more
/// than the field, which is not an undo of the removal:
///
/// - The save after a removal may prune the version the removal wrote. A limit
///   of no versions at all does, and so does a size limit that one version does
///   not fit. What is newest after that is something older, something undated,
///   or nothing at all.
/// - Any change after the removal writes a newer version of its own.
///
/// Both answer `None`, and never the position of some other version that
/// happens to hold a field of the same name.
pub(crate) fn before_removal(database: &Database, id: EntryId, field: &str) -> Option<usize> {
    let entry = database.entry(id)?;
    if entry.fields.contains_key(field) {
        return None;
    }

    let (index, newest) = entry
        .history
        .as_ref()?
        .get_entries()
        .iter()
        .enumerate()
        .max_by_key(|(index, version)| (age(version.times.last_modification), *index))?;

    let mut put_back = Entry::clone(&entry);
    put_back.history = None;
    put_back
        .fields
        .insert(field.to_owned(), newest.fields.get(field)?.clone());

    same_content(&put_back, newest).then_some(index)
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
    // names a version puts on files are reachable only through a reference into
    // the database the version belongs to.
    let mut versions: Vec<(Entry, u64)> = {
        let Some(entry) = database.entry(id) else {
            return;
        };
        if entry.history.is_none() {
            return;
        }
        // A version that cannot be weighed is a version this function does not
        // understand, and dropping what you do not understand is how history
        // goes missing. Leave the entry exactly as it is.
        let Some(weighed) = weighed(&entry, Entry::clone) else {
            return;
        };
        weighed
    };

    keep(
        &mut versions,
        |version| version.times.last_modification,
        limits,
    );

    let kept: Vec<Entry> = versions.into_iter().map(|(version, _)| version).collect();

    let Some(mut entry) = database.entry_mut(id) else {
        return;
    };
    entry.history = Some(rebuild(kept));
}

/// Whether the version a change to an entry would write now is still there
/// once the save after it has pruned.
///
/// That version is the entry as it stands, dated when the entry was last
/// changed, so it is weighed and dated here and put through [`keep`] behind the
/// versions the entry already has, which is where a change puts it. A removal
/// asks before it happens: the version it writes is the only way back, and a
/// database that keeps no versions, or a size limit that version does not fit,
/// throws it away at the very next save.
pub(crate) fn outlasts_save(database: &Database, id: EntryId) -> bool {
    let Some(entry) = database.entry(id) else {
        return false;
    };
    // A history the save cannot weigh is one it leaves exactly as it is.
    let Some(mut versions) = weighed(&entry, |version| (version.times.last_modification, false))
    else {
        return true;
    };
    versions.push(((entry.times.last_modification, true), weigh(&entry)));

    keep(
        &mut versions,
        |(modified, _)| *modified,
        Limits::of(database),
    );
    versions.iter().any(|((_, written), _)| *written)
}

/// Every version an entry keeps, as `read` makes of it, with what it weighs;
/// or `None` when one of them cannot be weighed.
fn weighed<T>(entry: &EntryRef<'_>, read: impl Fn(&Entry) -> T) -> Option<Vec<(T, u64)>> {
    let Some(history) = entry.history.as_ref() else {
        return Some(Vec::new());
    };

    let mut weighed = Vec::with_capacity(history.get_entries().len());
    for (index, version) in history.get_entries().iter().enumerate() {
        weighed.push((read(version), weigh(&entry.historical(index)?)));
    }
    Some(weighed)
}

/// Which of an entry's versions a save keeps: `versions` put oldest first, and
/// as many taken off the front as the limits ask.
///
/// The one rule for the save that prunes and for a removal asking beforehand
/// whether the version it writes will survive that save. Two copies of it would
/// be a removal that asked nobody, and a save that counted differently and
/// threw its version away.
fn keep<T>(
    versions: &mut Vec<(T, u64)>,
    modified: impl Fn(&T) -> Option<NaiveDateTime>,
    limits: Limits,
) {
    // The vector is oldest first: that is the order KeePassXC writes and the
    // order `settle` restores after every edit. Sorting by time only corrects a
    // file that arrived out of order, and being stable it leaves versions that
    // share a timestamp exactly where they were, so the newest of a tied pair is
    // still the last one and is never the first to go.
    //
    // A version with no modification time sorts last rather than first. Nothing
    // is known about when it was written, and a version nobody can date is the
    // wrong thing to throw away first.
    versions.sort_by_key(|(version, _)| age(modified(version)));

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
}

/// What one version costs, counted as what dropping it would give back: its
/// field names and values, and the names it puts on files.
///
/// Not the bytes of those files, which KeePassXC does count. They are held once
/// in the database's pool, every version that names one names the same bytes,
/// and the writer puts the whole pool in the file whether anything refers to it
/// or not - so dropping a version returns none of them, and charging a version
/// for bytes it cannot give back is a limit that cannot be met. An entry
/// carrying a file larger than `HistoryMaxSize` weighed more than the limit in
/// every version it had, so every version went, and no new one could ever be
/// kept: attaching a scan to an entry silently threw away the record of every
/// password it had ever held.
///
/// `SPEC.md` grants this. The arithmetic decides how many versions are kept and
/// not whether the file is valid, and it does not have to be KeePassXC's.
fn weigh(version: &EntryRef<'_>) -> u64 {
    let fields: usize = version
        .fields
        .iter()
        .map(|(name, value)| name.len() + value.get().len())
        .sum();

    let names: usize = version
        .attachments_named()
        .map(|(name, _)| name.len())
        .sum();

    (fields + names) as u64
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
