//! The pool of files a database carries, and the rule that keeps it readable.
//!
//! A KDBX 4 file stores every attachment once, in the inner header, and an
//! entry refers to one by a number. The reader Coffer is built on hands those
//! numbers out by position in the header, and the writer orders the header by
//! the number an attachment already carries. The two agree only while the
//! numbers are an unbroken run from zero: leave a hole and every file after it
//! answers to somebody else's number, so entries come back holding the wrong
//! file, or none at all.
//!
//! Two properties of the library shape everything below.
//!
//! **A file with no name on any entry cannot be in the pool.** The only way in
//! is [`EntryMut::add_attachment`][add], which always writes a name on the entry
//! that calls it, and every way out takes the bytes with the last name pointing
//! at them. A file therefore lives exactly as long as some entry, as it is now,
//! names it.
//!
//! **A previous version of an entry names files and cannot be rewritten.** The
//! library hands versions out behind a shared reference and offers no way to
//! change one. A file a version names can be neither dropped nor renumbered,
//! and a removal that would do either is refused rather than quietly taking
//! away what the version holds.
//!
//! [add]: keepass::db::EntryMut::add_attachment

use std::collections::{HashMap, HashSet};

use keepass::Database;
use keepass::db::{AttachmentId, EntryId, Value};

use crate::error::VaultError;

/// One name an entry gives a file.
#[derive(Clone, PartialEq, Eq)]
struct Named {
    entry: EntryId,
    name: String,
}

/// Everything that names one file.
#[derive(Default)]
struct Uses {
    /// The entries that name it as they are now.
    live: Vec<Named>,
    /// One entry for each previous version that names it.
    pinned: Vec<EntryId>,
}

impl Uses {
    /// The previous versions outside `doomed` that name this file. Entries in
    /// `doomed` are going away, versions and all, so what they hold is not
    /// something to preserve.
    fn pins(&self, doomed: &[EntryId]) -> usize {
        self.pinned
            .iter()
            .filter(|entry| !doomed.contains(entry))
            .count()
    }
}

/// What names every file in the database, entries as they are and previous
/// versions alike.
fn uses(database: &Database) -> HashMap<AttachmentId, Uses> {
    let mut found: HashMap<AttachmentId, Uses> = HashMap::new();

    for entry in database.iter_all_entries() {
        let id = entry.id();

        for (name, attachment) in entry.attachments_named() {
            found.entry(attachment.id()).or_default().live.push(Named {
                entry: id,
                name: name.to_owned(),
            });
        }

        let versions = entry
            .history
            .as_ref()
            .map_or(0, |history| history.get_entries().len());
        for index in 0..versions {
            let Some(version) = entry.historical(index) else {
                continue;
            };
            for (_, attachment) in version.attachments_named() {
                found.entry(attachment.id()).or_default().pinned.push(id);
            }
        }
    }

    found
}

/// The file `entry` calls `name`, if it has one.
fn named(database: &Database, entry: EntryId, name: &str) -> Option<AttachmentId> {
    Some(database.entry(entry)?.attachment_by_name(name)?.id())
}

/// Whether `entry` already has a file under this name.
pub(crate) fn held(database: &Database, entry: EntryId, name: &str) -> bool {
    named(database, entry, name).is_some()
}

/// Every name an entry gives a file.
fn names_on(database: &Database, entry: EntryId) -> Vec<Named> {
    let Some(found) = database.entry(entry) else {
        return Vec::new();
    };

    found
        .attachments_named()
        .map(|(name, _)| Named {
            entry: found.id(),
            name: name.to_owned(),
        })
        .collect()
}

/// Takes away the name `entry` gives a file. The bytes go with it when nothing
/// else names them, and stay when something does.
pub(crate) fn detach(
    database: &mut Database,
    entry: EntryId,
    name: &str,
) -> Result<(), VaultError> {
    if named(database, entry, name).is_none() {
        return Err(VaultError::NoSuchAttachment);
    }

    drop_names(
        database,
        &[Named {
            entry,
            name: name.to_owned(),
        }],
        &[],
    )
}

/// Takes away every name these entries give a file, so that the entries
/// themselves can be removed.
///
/// The library's own entry removal reaches into the pool, takes the bytes of
/// everything the entry named whether or not another entry names them too, and
/// leaves the holes behind. This does the same job by the rules of this module,
/// and it is what has to run before an entry goes.
pub(crate) fn detach_entries(
    database: &mut Database,
    entries: &[EntryId],
) -> Result<(), VaultError> {
    let names: Vec<Named> = entries
        .iter()
        .flat_map(|entry| names_on(database, *entry))
        .collect();

    drop_names(database, &names, entries)
}

/// A file on its way back into the pool: which one it is, which slot it belongs
/// in now, the name it comes back under, and its bytes.
struct Carried {
    id: AttachmentId,
    slot: usize,
    holder: Named,
    data: Value<Vec<u8>>,
}

/// Takes a set of names away and closes the pool up behind them.
///
/// Nothing is written until every part of the change is known to be possible,
/// so a refusal leaves the database exactly as it was.
fn drop_names(
    database: &mut Database,
    dropping: &[Named],
    doomed: &[EntryId],
) -> Result<(), VaultError> {
    let uses = uses(database);
    let unnamed = Uses::default();

    let mut pool: Vec<AttachmentId> = database
        .iter_all_attachments()
        .map(|attachment| attachment.id())
        .collect();
    pool.sort_unstable_by_key(AttachmentId::id);

    // What still names each file once these names are gone, which files a name
    // is being taken away from, and which files are left at all.
    let mut surviving: HashMap<AttachmentId, Vec<Named>> = HashMap::new();
    let mut touched: HashSet<AttachmentId> = HashSet::new();
    let mut kept: Vec<AttachmentId> = Vec::new();

    for id in &pool {
        let held = uses.get(id).unwrap_or(&unnamed);
        let (going, staying): (Vec<Named>, Vec<Named>) = held
            .live
            .iter()
            .cloned()
            .partition(|name| dropping.contains(name));
        if !going.is_empty() {
            touched.insert(*id);
        }

        // A file the change leaves with a name stays, and so does one that had
        // no name before it: this change is not what took that name away, and
        // bytes kept are never a lost field.
        if !staying.is_empty() || held.live.is_empty() {
            kept.push(*id);
        } else {
            // The last name is going. A previous version naming the file is a
            // reason to keep it, and keeping it is exactly what cannot be done:
            // the pool has no room for bytes that nothing names.
            let pins = held.pins(doomed);
            if pins > 0 {
                return Err(VaultError::AttachmentInHistory { versions: pins });
            }
        }

        surviving.insert(*id, staying);
    }

    // Everything that survives closes up towards the front. A file has to come
    // out of the pool and go back in when its number changes, and also when a
    // name is being taken off it, because taking a name off a file that is
    // still in the pool takes the bytes with it.
    let mut carried: Vec<Carried> = Vec::new();
    for (slot, id) in kept.iter().enumerate() {
        let moving = slot != id.id();
        if !moving && !touched.contains(id) {
            continue;
        }

        let left = surviving.get(id).map(Vec::as_slice).unwrap_or_default();
        // Renumbering means rewriting every name the file has. A previous
        // version's name cannot be rewritten, and a second name would be left
        // pointing at a number that is no longer there.
        if moving && (left.len() != 1 || uses.get(id).is_some_and(|held| held.pins(doomed) > 0)) {
            return Err(VaultError::AttachmentPinned);
        }
        let Some(holder) = left.first() else {
            return Err(VaultError::AttachmentPinned);
        };

        // The bytes are read while every file is still in place, because from
        // here on nothing may fail: an entry naming a file that is not in the
        // pool is a state no reader of this database survives, and the two
        // halves of the change must not be separated by a question that can be
        // answered no.
        carried.push(Carried {
            id: *id,
            slot,
            holder: holder.clone(),
            data: bytes(database, *id)?,
        });
    }

    for id in &pool {
        if !kept.contains(id) || carried.iter().any(|file| file.id == *id) {
            lift(database, *id);
        }
    }

    for name in dropping {
        forget(database, name);
    }

    // Lowest slot first, so that each file lands in the lowest one left free:
    // everything below it is either a file that never moved or a file already
    // put back.
    carried.sort_by_key(|file| file.slot);
    for file in carried {
        // The name has to go before the file comes back under it, because
        // adding a file under a name an entry already uses takes away the one
        // that name held.
        forget(database, &file.holder);
        put(database, &file.holder, file.data);
    }

    Ok(())
}

/// A copy of one file's bytes, protected the way the database holds them.
fn bytes(database: &Database, id: AttachmentId) -> Result<Value<Vec<u8>>, VaultError> {
    database
        .attachment(id)
        .map(|attachment| attachment.data.clone())
        .ok_or(VaultError::NoSuchAttachment)
}

/// Lifts a file out of the pool, leaving every name that points at it dangling.
///
/// This is what makes taking a name away possible at all: the library's own
/// removal takes the bytes with the last name pointing at them, and it counts
/// only the names it wrote itself in this session, so on a database read from
/// disk it takes them with the first. With the file already gone from the pool
/// it finds nothing to take.
///
/// A dangling name is a state nothing may read, so this is never called except
/// between the checks and the [`forget`] and [`put`] calls that settle it.
fn lift(database: &mut Database, id: AttachmentId) {
    if let Some(attachment) = database.attachment_mut(id) {
        attachment.remove();
    }
}

/// Drops one name for a file, leaving the pool alone. Safe only because the
/// file has already been lifted out of it.
fn forget(database: &mut Database, held: &Named) {
    if let Some(mut entry) = database.entry_mut(held.entry) {
        entry.remove_attachment_by_name(&held.name);
    }
}

/// Puts a file back into the pool under `holder`'s name, in the lowest slot
/// left free.
fn put(database: &mut Database, holder: &Named, data: Value<Vec<u8>>) {
    if let Some(mut entry) = database.entry_mut(holder.entry) {
        entry.add_attachment(holder.name.clone(), data);
    }
}

/// Whether the pool is the unbroken run from zero that this module keeps it as.
///
/// A database read from disk always is, because the reader numbers files by
/// their position in the header. This is the check that nothing in Coffer has
/// broken it since, and it runs before every save: a hole here means every
/// attachment after it lands on the wrong entry the next time the file is
/// opened, which is the failure this module exists to prevent.
pub(crate) fn unbroken(database: &Database) -> bool {
    let mut slots: Vec<usize> = database
        .iter_all_attachments()
        .map(|attachment| attachment.id().id())
        .collect();
    slots.sort_unstable();
    slots.iter().enumerate().all(|(at, slot)| at == *slot)
}
