//! The vault's own templates: the entries a database keeps to make new ones
//! from.
//!
//! The format names one group as the place they are kept, in
//! `Meta/EntryTemplatesGroup`, and KeePass 2 and MacPass offer what is in it
//! when an entry is made. The library reads and writes the field and does
//! nothing else with it, so the rule for what counts is here, once, and both
//! the tree that marks the group and the copy that is made from one of its
//! entries ask it.

use keepass::Database;
use keepass::db::{EntryId, GroupId};

use crate::bin::Bin;

/// The group `Meta/EntryTemplatesGroup` names, while an entry can be made from
/// what it holds.
///
/// It has to be there: a file can name a group that has gone, and a nil UUID
/// is KeePass's way of naming none at all. It is not the top of the vault,
/// whose entries are the reader's own entries and not templates of anything.
/// And it is not the recycle bin or inside it: what is there is deleted, and
/// nothing is made from it until it is put back.
pub(crate) fn group(database: &Database) -> Option<GroupId> {
    let named = database
        .meta
        .entry_templates_group
        .filter(|uuid| !uuid.is_nil())
        .map(GroupId::from_uuid)?;
    database.group(named)?;
    if named == database.root().id() {
        return None;
    }
    if Bin::of(database).standing(database, named).binned() {
        return None;
    }
    Some(named)
}

/// Whether `entry` is one of the templates: held by that group itself. What
/// sits in a folder inside it is not, the way KeePass 2 draws such a folder as
/// a list of its own rather than as more of the same.
pub(crate) fn holds(database: &Database, entry: EntryId) -> bool {
    let Some(templates) = group(database) else {
        return false;
    };
    database
        .entry(entry)
        .is_some_and(|found| found.parent().id() == templates)
}
