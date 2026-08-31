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

/// One previous version of one entry: which entry, and where in its history.
///
/// The position rather than the entry, because that is what can be dropped. A
/// removal that has to be got out of the way costs the versions naming the file
/// and nothing else; clearing the whole history of an entry the reader never
/// asked about would be taking months of their work to move one file.
pub(crate) type Held = (EntryId, usize);

/// Everything that names one file.
#[derive(Default)]
struct Uses {
    /// The entries that name it as they are now.
    live: Vec<Named>,
    /// Every previous version that names it.
    pinned: Vec<Held>,
}

impl Uses {
    /// The previous versions outside `doomed` that name this file. Entries in
    /// `doomed` are going away, versions and all, so what they hold is not
    /// something to preserve.
    fn holders(&self, doomed: &[EntryId]) -> Vec<Held> {
        self.pinned
            .iter()
            .filter(|(entry, _)| !doomed.contains(entry))
            .copied()
            .collect()
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
                found
                    .entry(attachment.id())
                    .or_default()
                    .pinned
                    .push((id, index));
            }
        }
    }

    // The names come out of hash maps, so without this they arrive in whatever
    // order the hasher gives that run. Which name a file comes back under has
    // to be the same answer every time; the reason is in [`lift`].
    for held in found.values_mut() {
        held.live.sort_by(|one, two| {
            (one.entry.to_string(), &one.name).cmp(&(two.entry.to_string(), &two.name))
        });
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
    slot: usize,
    holder: Named,
    data: Value<Vec<u8>>,
}

/// One file's place in the pool once the change has happened.
struct Placed {
    id: AttachmentId,
    slot: usize,
    /// The name it comes back under, when it has to come back at all.
    holder: Option<Named>,
}

/// Why a set of names cannot be taken away.
enum Refusal {
    /// Previous versions of these entries name files that would have to be
    /// renumbered, and the format gives no way to rewrite what a version points
    /// at. Clearing those versions is what makes the change possible, and it is
    /// a thing to offer rather than to do quietly.
    Versions {
        /// Every version that is holding something, by the entry it belongs to
        /// and its position in that entry's history.
        holders: Vec<Held>,
    },
    /// A file two entries name at once, which nothing in Coffer makes: putting
    /// it back under one name would leave the other pointing at nothing, and
    /// there is no way through the library to give a second name a number.
    Shared,
}

impl From<Refusal> for VaultError {
    fn from(refusal: Refusal) -> VaultError {
        match refusal {
            Refusal::Versions { .. } => VaultError::AttachmentInHistory,
            Refusal::Shared => VaultError::AttachmentPinned,
        }
    }
}

/// The previous versions standing between one file's name and going.
///
/// Empty when nothing is, which includes the case where the removal is
/// impossible for some other reason: this answers "which versions would have to
/// go", not "can this happen".
///
/// One refusal names the first thing in the way and there can be another behind
/// it, so a caller working through this has to ask again after every round.
pub(crate) fn blocking(database: &Database, entry: EntryId, name: &str) -> Vec<Held> {
    let dropping = [Named {
        entry,
        name: name.to_owned(),
    }];

    match layout(database, &dropping, &[]) {
        Err(Refusal::Versions { mut holders }) => {
            holders.sort_by_key(|(entry, index)| (entry.to_string(), *index));
            holders.dedup();
            holders
        }
        _ => Vec::new(),
    }
}

/// Where every file ends up once these names are gone, or why it cannot happen.
///
/// The pool has to come out of this an unbroken run from zero, and a file a
/// previous version names has to come out of it holding the number it went in
/// with. Those two together are the whole of the arithmetic.
///
/// **What must not be done is compact the pool.** Closing every survivor up
/// towards the front moves nearly every file, so one entry anywhere in the vault
/// having one version was enough to make every file in it permanently
/// unremovable - and the recycle bin permanently un-emptyable with it. Instead
/// the files that cannot be renumbered keep the numbers they have, and the ones
/// that can fill whatever that leaves free.
fn layout(
    database: &Database,
    dropping: &[Named],
    doomed: &[EntryId],
) -> Result<Vec<Placed>, Refusal> {
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
            let holders = held.holders(doomed);
            if !holders.is_empty() {
                return Err(Refusal::Versions { holders });
            }
        }

        surviving.insert(*id, staying);
    }

    // Whether a file may be given a different number.
    //
    // Two things stand in the way and only these two. A previous version points
    // at the number, and a version cannot be rewritten. Or a second entry names
    // the file, and [`put`] writes one name: the file would come back under that
    // one and the other would be left pointing at a number now holding somebody
    // else's bytes.
    //
    // Neither matters to a file that keeps the number it has. Lifting it out and
    // putting it straight back lands it on the same number - every number below
    // is occupied, so that is the lowest one free - and every other name that
    // was never touched still resolves to it.
    let movable = |id: &AttachmentId| {
        surviving.get(id).is_some_and(|left| left.len() == 1)
            && uses.get(id).unwrap_or(&unnamed).holders(doomed).is_empty()
    };

    let room = kept.len();
    let mut taken: HashSet<usize> = HashSet::new();
    let mut settled: HashMap<AttachmentId, usize> = HashMap::new();

    // The ones that cannot be given a different number first, each on the number
    // it already has. A number past the end of the pool it is about to become is
    // the one shape this cannot answer: something has to give up that slot and
    // nothing may.
    for id in &kept {
        if movable(id) {
            continue;
        }
        if id.id() >= room {
            return Err(refusal_for(&uses, &unnamed, id, doomed));
        }
        settled.insert(*id, id.id());
        taken.insert(id.id());
    }

    let mut free = (0..room).filter(|slot| !taken.contains(slot));
    for id in &kept {
        if settled.contains_key(id) {
            continue;
        }
        // Cannot run out: as many slots are free as there are files left to
        // place. Refusing rather than asserting keeps the ban on panics whole.
        let Some(slot) = free.next() else {
            return Err(Refusal::Shared);
        };
        settled.insert(*id, slot);
    }

    let mut placed = Vec::with_capacity(kept.len());
    for id in &kept {
        let slot = settled.get(id).copied().unwrap_or(id.id());
        // A file has to come out of the pool and go back in when its number
        // changes, and also when a name is being taken off it, because taking a
        // name off a file that is still in the pool takes the bytes with it.
        if slot == id.id() && !touched.contains(id) {
            placed.push(Placed {
                id: *id,
                slot,
                holder: None,
            });
            continue;
        }
        // The layout above gave every file that cannot move the number it
        // already had, so this can only be a file that may. Refusing rather than
        // asserting keeps the ban on panics whole.
        if slot != id.id() && !movable(id) {
            return Err(refusal_for(&uses, &unnamed, id, doomed));
        }
        // The first by the order `uses` sorted them into, which is what makes
        // the choice the same one every time. See [`lift`]: the entry that
        // added a file to the pool is the one that was first among its names
        // then, and taking names away never makes an earlier one appear, so
        // that entry is still the one chosen here for as long as it has a name
        // at all. That is what keeps the library's own bookkeeping, which knows
        // only about that one entry, from taking away a name nothing puts back.
        let Some(holder) = surviving.get(id).and_then(|left| left.first()) else {
            return Err(Refusal::Shared);
        };
        placed.push(Placed {
            id: *id,
            slot,
            holder: Some(holder.clone()),
        });
    }

    Ok(placed)
}

/// Which of the two refusals a file that cannot be rewritten is: history in the
/// way, which a reader can do something about, or a file named twice, which
/// they cannot.
fn refusal_for(
    uses: &HashMap<AttachmentId, Uses>,
    unnamed: &Uses,
    id: &AttachmentId,
    doomed: &[EntryId],
) -> Refusal {
    let holders = uses.get(id).unwrap_or(unnamed).holders(doomed);
    if holders.is_empty() {
        Refusal::Shared
    } else {
        Refusal::Versions { holders }
    }
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
    let placed = layout(database, dropping, doomed)?;

    // The bytes are read while every file is still in place, because from here
    // on nothing may fail: an entry naming a file that is not in the pool is a
    // state no reader of this database survives, and the two halves of the
    // change must not be separated by a question that can be answered no.
    let mut carried: Vec<Carried> = Vec::new();
    for file in &placed {
        let Some(holder) = &file.holder else { continue };
        carried.push(Carried {
            slot: file.slot,
            holder: holder.clone(),
            data: bytes(database, file.id)?,
        });
    }

    let staying: HashSet<AttachmentId> = placed
        .iter()
        .filter(|file| file.holder.is_none())
        .map(|file| file.id)
        .collect();

    let pool: Vec<AttachmentId> = database
        .iter_all_attachments()
        .map(|attachment| attachment.id())
        .collect();
    for id in pool {
        if !staying.contains(&id) {
            lift(database, id);
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
///
/// One thing it does do: for a file this session put into the pool, the library
/// knows about the one entry that put it there, and takes away every name that
/// entry gives the file. Which is safe for exactly one reason, and it is worth
/// stating.
///
/// A file is put into the pool under the first of its names in
/// [`uses`]'s order, and the names a file has only ever get fewer. So the entry
/// the library knows about is the first-named entry as it was then, and it is
/// still the first-named entry now unless every name it had is being dropped.
/// Either the names this takes away are the ones [`put`] is about to write
/// back, or they are names the caller asked to be rid of. A file that arrived
/// in the database has no bookkeeping at all and nothing of it is taken away
/// here.
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
