//! Ten rotating copies of the database, taken before every write.
//!
//! `<database>.1.bak` is the most recent and `<database>.10.bak` the oldest.
//! Together they cover roughly the last hour of active work; entry history in
//! the database itself covers the months before that.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::storage::{Seen, atomic::write_atomic, sibling};

/// Tightens a snapshot to owner-only.
///
/// A hard link shares its inode with the database, so this also tightens the
/// database. That is the same thing the next save does anyway, and the spec asks
/// for owner-only on both.
///
/// A symbolic link somebody left at the database's name is linked as the link,
/// and left as it is: a link's own mode means nothing, and tightening through
/// it would change a file outside anything Coffer keeps.
fn owner_only(path: &Path) -> Result<(), io::Error> {
    use std::os::unix::fs::PermissionsExt;
    if path.symlink_metadata()?.file_type().is_symlink() {
        return Ok(());
    }
    std::fs::set_permissions(
        path,
        std::fs::Permissions::from_mode(crate::storage::OWNER_ONLY),
    )
}

/// How many snapshots are kept. The eleventh push drops the oldest.
pub const SNAPSHOT_COUNT: u32 = 10;

/// Pushes the current contents of `database` in at slot 1 and shifts every
/// other snapshot down one.
///
/// The shift runs from the oldest slot towards the newest, so a process killed
/// part way through leaves a duplicated snapshot rather than a hole. Nothing is
/// removed except the copy that was already falling off the end.
///
/// **Slot 1 is a hard link, so the caller must replace the database by rename
/// and never in place.** A writer that truncates and rewrites the database
/// would be rewriting the snapshot with it, and one save would silently empty
/// the whole chain. That is why this is not public: the only caller is
/// [`Vault::save`][crate::Vault::save], which replaces the file by rename.
///
/// A database that does not exist yet has nothing to snapshot, which is not an
/// error.
pub(crate) fn rotate(database: &Path) -> Result<(), io::Error> {
    if !database.try_exists()? {
        return Ok(());
    }

    remove_if_present(&slot(database, SNAPSHOT_COUNT)?)?;

    for index in (1..SNAPSHOT_COUNT).rev() {
        let from = slot(database, index)?;
        if from.try_exists()? {
            std::fs::rename(&from, slot(database, index + 1)?)?;
        }
    }

    capture(database, &slot(database, 1)?)
}

/// The path of one snapshot slot, counting from 1 for the most recent.
pub fn slot(database: &Path, index: u32) -> Result<PathBuf, io::Error> {
    sibling(database, &format!(".{index}.bak"))
}

/// Which snapshot slot a path names, when it names one.
///
/// The name is Coffer's own convention, so a file that carries it is treated as
/// one of Coffer's snapshots: it opens like any other database and is not
/// written back, because the next save of the database beside it would rotate
/// it away.
pub fn slot_of(path: &Path) -> Option<u32> {
    parse(path).map(|(_, index)| index)
}

/// The database a snapshot was taken beside, read off the snapshot's name, or
/// nothing when the name is not a snapshot's.
///
/// The name is the whole of the record, as it is for the copy a lock leaves
/// (see [`crate::storage::unsaved::taken_from`]): nothing inside a snapshot
/// or beside it says whose it is.
pub fn taken_from(path: &Path) -> Option<PathBuf> {
    parse(path).map(|(database, _)| path.with_file_name(database))
}

/// A snapshot's name taken apart: the database's file name, and the slot.
fn parse(path: &Path) -> Option<(&str, u32)> {
    let name = path.file_name()?.to_str()?;
    let (database, index) = name.strip_suffix(".bak")?.rsplit_once('.')?;
    if database.is_empty() {
        return None;
    }

    let index: u32 = index.parse().ok()?;
    (1..=SNAPSHOT_COUNT)
        .contains(&index)
        .then_some((database, index))
}

/// A snapshot that exists on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taken {
    /// Which slot it is in, counting from 1 for the most recent.
    pub index: u32,
    pub path: PathBuf,
    /// When it was taken, as far as the filesystem knows. Absent on a
    /// filesystem that does not keep the time.
    pub taken: Option<std::time::SystemTime>,
    /// Which file it is, wherever later saves move it: every save renames each
    /// snapshot a slot further down, which keeps all four things this holds.
    ///
    /// The name's own entry rather than what it leads to. Coffer never puts a
    /// link at a slot, and a link somebody else put there is known as the link:
    /// the file it leads to may be the vault itself, and every later save
    /// would hand that file's identity to the snapshot it pushes into slot 1.
    pub(crate) seen: Seen,
}

/// The snapshots beside `database`, most recent first.
///
/// A slot that is not there is skipped rather than reported: the chain is a
/// history of saves, and a database saved three times has three of them. This
/// is what the screen offers when a database will not open, so it asks about
/// the file the user chose rather than about a database Coffer managed to read.
///
/// A slot that cannot be asked about is skipped the same way, one slot at a
/// time: a link somebody left that leads round in a circle, or a disk that
/// answered with an error. One slot nobody can read is no reason to hide the
/// nine beside it, from the screen or from a removal.
pub fn taken(database: &Path) -> Result<Vec<Taken>, io::Error> {
    let mut found = Vec::new();

    for index in 1..=SNAPSHOT_COUNT {
        let path = slot(database, index)?;
        let Ok(opens) = std::fs::metadata(&path) else {
            continue;
        };
        let Ok(own) = path.symlink_metadata() else {
            continue;
        };

        if opens.is_file() {
            found.push(Taken {
                index,
                path,
                taken: opens.modified().ok(),
                seen: Seen::of_metadata(&own),
            });
        }
    }

    Ok(found)
}

/// Where a snapshot listed earlier is now: the slot beside `database` that
/// holds the same file, or nothing when none does.
///
/// Every save renames each snapshot a slot further down and keeps the file,
/// so a slot read before a save names the neighbour after it, and this is how
/// the one that was shown is found instead. Nothing when the snapshot has been
/// pushed off the end of the chain, or taken away, since. Nothing as well on a
/// filesystem that numbers a file anew when it is renamed, which FAT and exFAT
/// may: there a save between the listing and the asking is a snapshot not
/// found, and never another snapshot found in its place.
pub fn find(database: &Path, shown: &Taken) -> Result<Option<Taken>, io::Error> {
    Ok(taken(database)?
        .into_iter()
        .find(|each| each.seen == shown.seen))
}

/// What removing the snapshots that open with an old master password came to.
#[derive(Debug)]
pub struct Removal {
    /// How many went.
    pub gone: usize,
    /// How many would not go. Each is still beside the vault, still opens with
    /// the old password, and is remembered to be removed by the next press.
    pub left: usize,
    /// Why the first of those would not go. There whenever any are left.
    pub refused: Option<io::Error>,
}

/// Snapshots that open with a master password the vault no longer has.
///
/// Known by the file each one is, as [`Taken::seen`] keeps it, and never by its
/// slot. Every later save renames each one a slot further down, which keeps
/// what is known of it, and puts a snapshot written under the new password in
/// slot 1, which is another file. The time and the length are there as well as
/// the inode because a filesystem that reuses inode numbers could hand a
/// removed snapshot's number to a new one.
#[derive(Default)]
pub(crate) struct Superseded(Vec<Seen>);

impl Superseded {
    /// Every snapshot beside `database` as it stands. Asked straight after the
    /// write that changed the key, when every one of them is older than it.
    pub(crate) fn now(database: &Path) -> Result<Superseded, io::Error> {
        Ok(Superseded(
            taken(database)?.into_iter().map(|each| each.seen).collect(),
        ))
    }

    pub(crate) fn count(&self) -> usize {
        self.0.len()
    }

    /// Takes off the disk every slot beside `database` that holds one of these
    /// files now, and answers with how many went and how many would not.
    ///
    /// A file that will not go stays remembered, to be asked about again; the
    /// rest still go. One that is in no slot any more - pushed off the end of
    /// the chain by later saves - is forgotten with the ones taken. Nothing but
    /// a slot's own name is ever removed, so a link at a slot goes as a link
    /// and the file it leads to stays.
    pub(crate) fn remove(&mut self, database: &Path) -> Removal {
        let listed = match taken(database) {
            Ok(listed) => listed,
            Err(error) => {
                return Removal {
                    gone: 0,
                    left: self.count(),
                    refused: Some(error),
                };
            }
        };

        let mut gone = 0;
        let mut kept: Vec<Seen> = Vec::new();
        let mut refused: Option<io::Error> = None;

        for each in listed {
            if !self.0.contains(&each.seen) {
                continue;
            }
            match remove_if_present(&each.path) {
                Ok(()) => gone += 1,
                Err(error) => {
                    kept.push(each.seen);
                    refused.get_or_insert(error);
                }
            }
        }

        self.0.retain(|old| kept.contains(old));
        Removal {
            gone,
            left: self.count(),
            refused,
        }
    }
}

/// Takes every snapshot beside `database` off the disk. Only for a database
/// that has gone for good: see [`crate::storage::unsaved::retire`].
pub(crate) fn clear(database: &Path) -> Result<(), io::Error> {
    for index in 1..=SNAPSHOT_COUNT {
        remove_if_present(&slot(database, index)?)?;
    }
    Ok(())
}

fn remove_if_present(path: &Path) -> Result<(), io::Error> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Puts the current contents of the database in slot 1.
///
/// A hard link is the whole of it on a normal filesystem: the snapshot and the
/// database name the same bytes, and the atomic save that follows gives the
/// database a new inode, leaving the snapshot holding what was there before.
/// Nothing is copied, so a hundred-megabyte database costs nothing to snapshot
/// and the database path is never absent for even a moment.
///
/// Filesystems that refuse hard links fall back to a copy through the atomic
/// writer, so that a snapshot is either whole or absent.
fn capture(database: &Path, target: &Path) -> Result<(), io::Error> {
    match std::fs::hard_link(database, target) {
        Ok(()) => return owner_only(target),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            std::fs::remove_file(target)?;
            if std::fs::hard_link(database, target).is_ok() {
                return owner_only(target);
            }
        }
        Err(_) => {}
    }

    write_atomic::<io::Error, _>(target, |writer: &mut dyn Write| {
        let mut source = File::open(database)?;
        io::copy(&mut source, writer)?;
        Ok(())
    })
}
