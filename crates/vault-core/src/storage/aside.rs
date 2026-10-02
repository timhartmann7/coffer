//! The vault's own file kept under a name of its own, when a backup goes in its
//! place and the file would not open with the backup's password.
//!
//! A file that opens with it is an older or newer state of the same vault, and
//! the ordinary write keeps it as the newest snapshot, as every save keeps what
//! it replaces. One that does not - damaged, under another password, not a
//! vault at all - would sit in the chain only until ten later saves pushed it
//! out, and nothing in Coffer could open it to say what it held. So it is given
//! a name the chain never touches, and nothing in Coffer removes it.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use crate::storage::watch::{self, Stamp};
use crate::storage::{atomic, no_links, sibling};

/// Between the vault's file name and the day it was set aside on.
const INFIX: &str = ".replaced-";

/// The end of every such name. Load-bearing for the reason the copy a lock
/// leaves ends in it: every file panel Coffer opens filters on it, and a reader
/// whose vault file went under another password elsewhere has to be able to
/// pick this one out of "Open another database" and open it with that one.
const EXTENSION: &str = ".kdbx";

/// How many names one day is tried under before the answer is that they are
/// all taken. Far past any number of presses a day, and short of a loop that
/// runs for as long as somebody keeps the folder full.
const NAMES: u32 = 1000;

/// The name `vault`'s file is kept under, set aside on `day`: the day alone the
/// first time, and the day and a number from the second on.
fn name(vault: &Path, day: NaiveDate, number: u32) -> Result<PathBuf, io::Error> {
    let suffix = if number == 1 {
        format!("{INFIX}{day}{EXTENSION}")
    } else {
        format!("{INFIX}{day}-{number}{EXTENSION}")
    };
    sibling(vault, &suffix)
}

/// Gives the file at `vault` a second name of its own beside it, dated `day`,
/// and answers with that name.
///
/// A name that is taken is never written over, a link that leads nowhere
/// included: the next number is tried instead. The second name is a hard link,
/// which is refused at a name that is taken and shares the file rather than
/// copying it. A filesystem that keeps no second name for a file gets a copy:
/// the name is taken first with an exclusive create, and then filled whole
/// through a staged write, so that it holds Coffer's own empty file or every
/// byte of the vault's and never a part of them. The copy is owner-only from
/// birth; a link is tightened (see [`tightened`]).
pub(crate) fn keep(vault: &Path, day: NaiveDate) -> Result<PathBuf, io::Error> {
    keep_by(vault, day, |from, to| std::fs::hard_link(from, to))
}

/// [`keep`], with the second name given by `link`, so that a disk without
/// links - which no suite can mount - can be stood in for.
fn keep_by(
    vault: &Path,
    day: NaiveDate,
    link: impl Fn(&Path, &Path) -> Result<(), io::Error>,
) -> Result<PathBuf, io::Error> {
    let mut linking = true;

    for number in 1..=NAMES {
        let aside = name(vault, day, number)?;

        if linking {
            match link(vault, &aside) {
                Ok(()) => {
                    if let Err(error) = tightened(&aside) {
                        let _ = std::fs::remove_file(&aside);
                        return Err(error);
                    }
                    return Ok(aside);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                // The lock beside the vault was just taken in this folder, so
                // a refusal here is the filesystem's and not the folder's.
                Err(error) if no_links(&error) => linking = false,
                Err(error) => return Err(error),
            }
        }

        match atomic::reserve(&aside) {
            Ok(()) => return copied(vault, &aside).map(|()| aside),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::from(io::ErrorKind::AlreadyExists))
}

/// Takes away what anybody but the owner may do with the file a second name
/// was given to, and gives the owner nothing.
///
/// The link shares the vault's file, so whatever is done to one is done to the
/// other. Made owner-only outright, the way a snapshot is, a vault file its
/// owner had made read only would come back writable - and the write that
/// follows would go over it, where a save checks first and refuses.
///
/// A symbolic link somebody left at the vault's name is given its second name
/// as the link, and is left as it is: a link's own mode means nothing, and
/// asking through it would tighten whatever it leads to, which is a file
/// outside anything Coffer keeps.
fn tightened(path: &Path) -> Result<(), io::Error> {
    use std::os::unix::fs::PermissionsExt;
    let own = std::fs::symlink_metadata(path)?;
    if own.file_type().is_symlink() {
        return Ok(());
    }
    std::fs::set_permissions(
        path,
        std::fs::Permissions::from_mode(own.permissions().mode() & 0o700),
    )
}

/// Fills a name [`keep`] reserved with every byte of the vault's file, or takes
/// the reservation back off the disk when it cannot.
fn copied(vault: &Path, aside: &Path) -> Result<(), io::Error> {
    let filled = File::open(vault).and_then(|mut source| {
        atomic::write_atomic::<io::Error, _>(aside, |writer: &mut dyn io::Write| {
            io::copy(&mut source, writer).map(drop)
        })
    });
    if filled.is_err() {
        let _ = std::fs::remove_file(aside);
    }
    filled
}

/// Takes the second name back after a press that did not go through, and only
/// while the vault's name still holds the very file it was given for: the same
/// file, or the same bytes in a copy. Then nothing goes with it, and a refused
/// press leaves no `-2`, `-3`... behind it. Anything else at the vault's name -
/// a write that landed before it failed, another client's - and the second
/// name stays, because it may be the only one the file has left.
pub(crate) fn withdraw(vault: &Path, aside: &Path) {
    let linked = matches!(
        (Stamp::of(vault), Stamp::of(aside)),
        (Ok(one), Ok(other)) if one == other
    );
    let digest = |path: &Path| std::fs::read(path).map(|bytes| watch::digest(&bytes));
    let copied = || matches!((digest(vault), digest(aside)), (Ok(one), Ok(other)) if one == other);

    if linked || copied() {
        let _ = std::fs::remove_file(aside);
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::storage::{reserved, snapshot, unsaved};

    const DAY: NaiveDate = match NaiveDate::from_ymd_opt(2026, 10, 1) {
        Some(day) => day,
        None => NaiveDate::MIN,
    };

    /// What the name is, and what it is not. The day comes first and the
    /// number after it from the second on, so a folder sorted by name lists a
    /// day's together in the order they were made, and `.kdbx` is always last.
    /// None of them is a name Coffer keeps a file of another kind under beside
    /// a vault - a snapshot, the copy a lock left - which every other part of
    /// Coffer would take it for, and offer or rotate or retire as one.
    #[test]
    fn the_name_a_file_is_kept_under_is_dated_numbered_and_ends_in_kdbx() -> Result<(), io::Error> {
        let folder = Path::new("/Users/someone/Vault");
        for (vault, number, kept) in [
            ("vault.kdbx", 1, "vault.kdbx.replaced-2026-10-01.kdbx"),
            ("vault.kdbx", 2, "vault.kdbx.replaced-2026-10-01-2.kdbx"),
            ("vault.kdbx", 10, "vault.kdbx.replaced-2026-10-01-10.kdbx"),
            (".env.kdbx", 1, ".env.kdbx.replaced-2026-10-01.kdbx"),
            ("vault", 3, "vault.replaced-2026-10-01-3.kdbx"),
            (
                "vault.kdbx.unsaved.kdbx",
                1,
                "vault.kdbx.unsaved.kdbx.replaced-2026-10-01.kdbx",
            ),
        ] {
            let named = name(&folder.join(vault), DAY, number)?;
            assert_eq!(named, folder.join(kept));
            assert!(!reserved(&named), "{kept} is a name Coffer keeps");
            assert_eq!(unsaved::taken_from(&named), None, "{kept} is a copy");
            assert_eq!(snapshot::taken_from(&named), None, "{kept} is a snapshot");
        }
        Ok(())
    }

    /// A disk without links gets a copy of the vault's file under the next free
    /// name, whole and owner-only, and the names already there are left as they
    /// were - a link that leads nowhere among them.
    #[test]
    fn a_disk_without_links_keeps_a_whole_copy_under_the_next_free_name() -> Result<(), io::Error> {
        let folder = tempfile::tempdir()?;
        let vault = folder.path().join("vault.kdbx");
        std::fs::write(&vault, b"a vault nobody can open")?;
        std::fs::write(name(&vault, DAY, 1)?, b"somebody's")?;
        std::os::unix::fs::symlink(folder.path().join("nowhere"), name(&vault, DAY, 2)?)?;

        let refused = |_: &Path, _: &Path| Err(io::Error::from_raw_os_error(libc::EPERM));
        let kept = keep_by(&vault, DAY, refused)?;

        assert_eq!(kept, name(&vault, DAY, 3)?);
        assert_eq!(std::fs::read(&kept)?, b"a vault nobody can open");
        assert_eq!(std::fs::read(name(&vault, DAY, 1)?)?, b"somebody's");
        assert!(!folder.path().join("nowhere").exists());
        assert_eq!(
            std::fs::metadata(&kept)?.permissions().mode() & 0o777,
            0o600
        );
        assert_ne!(Stamp::of(&kept)?, Stamp::of(&vault)?, "the copy is a link");
        Ok(())
    }

    /// A copy that cannot be filled takes its reservation with it, so a press
    /// refused on the way leaves no empty file under a name nothing removes.
    #[test]
    fn a_copy_that_cannot_be_filled_leaves_no_name_behind() -> Result<(), io::Error> {
        let folder = tempfile::tempdir()?;
        let vault = folder.path().join("vault.kdbx");

        let refused = |_: &Path, _: &Path| Err(io::Error::from_raw_os_error(libc::ENOTSUP));
        let answer = keep_by(&vault, DAY, refused);

        assert_eq!(
            answer.map_err(|error| error.kind()),
            Err(io::ErrorKind::NotFound)
        );
        assert_eq!(std::fs::read_dir(folder.path())?.count(), 0);
        Ok(())
    }

    /// Anything a link answers but a name taken or a disk without links is the
    /// answer, and nothing is copied over it.
    #[test]
    fn a_link_refused_for_another_reason_is_the_answer() -> Result<(), io::Error> {
        let folder = tempfile::tempdir()?;
        let vault = folder.path().join("vault.kdbx");
        std::fs::write(&vault, b"a vault")?;

        let full = |_: &Path, _: &Path| Err(io::Error::from_raw_os_error(libc::ENOSPC));
        assert!(keep_by(&vault, DAY, full).is_err());
        assert_eq!(std::fs::read_dir(folder.path())?.count(), 1);
        Ok(())
    }

    /// A link shares the vault's file, so its mode is the vault's: what others
    /// may do is taken away, and the owner is given nothing the vault's file did
    /// not already give them. A file made read only stays read only.
    #[test]
    fn a_second_name_takes_from_others_and_gives_the_owner_nothing() -> Result<(), io::Error> {
        let folder = tempfile::tempdir()?;
        let vault = folder.path().join("vault.kdbx");
        let mode = |path: &Path| -> Result<u32, io::Error> {
            Ok(std::fs::metadata(path)?.permissions().mode() & 0o777)
        };

        for (was, kept) in [(0o644, 0o600), (0o400, 0o400), (0o600, 0o600)] {
            std::fs::write(&vault, b"a vault")?;
            std::fs::set_permissions(&vault, std::fs::Permissions::from_mode(was))?;

            let aside = keep(&vault, DAY)?;
            assert_eq!(mode(&aside)?, kept, "{was:o}");
            assert_eq!(mode(&vault)?, kept, "{was:o}");

            std::fs::remove_file(&aside)?;
            std::fs::remove_file(&vault)?;
        }
        Ok(())
    }

    /// Taken back only while the vault's name holds what the second name does,
    /// by the same file or by the same bytes.
    #[test]
    fn a_second_name_goes_only_while_the_vault_still_holds_its_file() -> Result<(), io::Error> {
        let folder = tempfile::tempdir()?;
        let vault = folder.path().join("vault.kdbx");
        std::fs::write(&vault, b"the vault as it was")?;

        let linked = keep(&vault, DAY)?;
        withdraw(&vault, &linked);
        assert!(!linked.exists(), "a link to the vault's own file stayed");

        let refused = |_: &Path, _: &Path| Err(io::Error::from_raw_os_error(libc::EPERM));
        let copy = keep_by(&vault, DAY, refused)?;
        withdraw(&vault, &copy);
        assert!(!copy.exists(), "a copy of the vault's own bytes stayed");

        let kept = keep(&vault, DAY)?;
        std::fs::remove_file(&vault)?;
        std::fs::write(&vault, b"written since")?;
        withdraw(&vault, &kept);
        assert_eq!(std::fs::read(&kept)?, b"the vault as it was");

        std::fs::remove_file(&vault)?;
        withdraw(&vault, &kept);
        assert!(kept.exists(), "the only name the file had was taken back");
        Ok(())
    }
}
