//! What the save panel offers for a copy on another disk: the folder it opens
//! on, and the name it fills in.

use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use super::{Copied, Last};
use crate::disks;

/// What every copy's name ends in, as every vault's does: the panel's filter
/// shows nothing else.
const EXTENSION: &str = ".kdbx";

/// How many names one day is tried under before the plain one is offered and
/// the write is left to refuse it. Far past any number of copies a day, and
/// short of a loop that runs for as long as somebody keeps the folder full.
const NAMES: u32 = 1000;

/// Where the panel opens: the folder the last copy on another disk went to
/// while it is a folder, another disk this Mac has mounted, the folder the last
/// copy on the vault's own disk went to, or beside the vault.
pub fn starting_in(copied: &Copied, vault: &Path) -> Option<PathBuf> {
    let beside = vault.parent()?;
    let folder = |last: &Option<Last>| last.as_ref().map(|last| PathBuf::from(&last.folder));
    Some(first_of(
        folder(&copied.other_disk),
        || disks::others(beside),
        folder(&copied.same_disk),
        beside,
    ))
}

/// The order [`starting_in`] takes, apart so that the suite can hand it disks.
/// The mount table is read only when the folder on another disk will not do.
fn first_of(
    away: Option<PathBuf>,
    others: impl FnOnce() -> Vec<PathBuf>,
    near: Option<PathBuf>,
    beside: &Path,
) -> PathBuf {
    away.filter(|folder| folder.is_dir())
        .or_else(|| others().into_iter().next())
        .or_else(|| near.filter(|folder| folder.is_dir()))
        .unwrap_or_else(|| beside.to_owned())
}

/// The name a copy is offered under: the vault's, and the day, numbered past
/// any name `folder` already holds. A copy never replaces a file, so the name
/// offered is one it can take, and a second copy on one day sits beside the
/// first. The vault's name is its file's without `.kdbx` and without leading
/// dots, which would make the copy a file the Finder hides; one with nothing
/// left is a vault's.
pub fn name(vault: &Path, today: NaiveDate, folder: Option<&Path>) -> String {
    let file = vault
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = file
        .len()
        .checked_sub(EXTENSION.len())
        .filter(|&end| {
            file.get(end..)
                .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION))
        })
        .and_then(|end| file.get(..end))
        .unwrap_or(&file);
    let stem = match stem.trim_start_matches('.') {
        "" => "vault",
        named => named,
    };
    let day = today.format("%Y-%m-%d");
    let named = |number: u32| match number {
        1 => format!("{stem} {day}{EXTENSION}"),
        _ => format!("{stem} {day} {number}{EXTENSION}"),
    };
    (1..=NAMES)
        .map(named)
        .find(|name| folder.is_none_or(|folder| folder.join(name).symlink_metadata().is_err()))
        .unwrap_or_else(|| named(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().expect("a scratch directory")
    }

    fn first_of_october() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 1).expect("a day")
    }

    /// The folder on another disk while it is a folder; then another disk,
    /// read only now; then the folder on the vault's disk; then beside the
    /// vault. A file where a folder was is not a folder.
    #[test]
    fn the_panel_opens_where_the_last_copy_on_another_disk_went_while_it_is_a_folder() {
        let beside = scratch();
        let away = scratch();
        let near = scratch();
        let stick = PathBuf::from("/Volumes/Stick");
        let unasked = || -> Vec<PathBuf> { panic!("the mount table was read") };

        assert_eq!(
            first_of(Some(away.path().to_owned()), unasked, None, beside.path()),
            away.path()
        );

        let gone = away.path().join("gone");
        assert_eq!(
            first_of(
                Some(gone.clone()),
                || vec![stick.clone()],
                None,
                beside.path()
            ),
            stick
        );

        let file = away.path().join("a file");
        std::fs::write(&file, b"not a folder").expect("it is written");
        assert_eq!(
            first_of(
                Some(file),
                Vec::new,
                Some(near.path().to_owned()),
                beside.path()
            ),
            near.path()
        );

        assert_eq!(
            first_of(Some(gone.clone()), Vec::new, Some(gone), beside.path()),
            beside.path()
        );
        assert_eq!(first_of(None, Vec::new, None, beside.path()), beside.path());
    }

    /// The vault's name and the day, numbered past whatever the folder holds -
    /// a file, a folder, a link that leads nowhere - and never a hidden file.
    #[test]
    fn the_copy_is_named_after_the_vault_and_the_day_and_never_a_name_that_is_held() {
        let folder = scratch();
        let today = first_of_october();
        let vault = Path::new("/Users/someone/Coffer/vault.kdbx");

        assert_eq!(name(vault, today, None), "vault 2026-10-01.kdbx");
        assert_eq!(
            name(vault, today, Some(folder.path())),
            "vault 2026-10-01.kdbx"
        );

        std::fs::write(folder.path().join("vault 2026-10-01.kdbx"), b"a copy").expect("written");
        std::fs::create_dir(folder.path().join("vault 2026-10-01 2.kdbx")).expect("made");
        std::os::unix::fs::symlink(
            folder.path().join("nowhere"),
            folder.path().join("vault 2026-10-01 3.kdbx"),
        )
        .expect("the link is made");
        assert_eq!(
            name(vault, today, Some(folder.path())),
            "vault 2026-10-01 4.kdbx"
        );

        for nameless in ["/.kdbx", "/", "/..kdbx", ""] {
            assert_eq!(
                name(Path::new(nameless), today, None),
                "vault 2026-10-01.kdbx",
                "{nameless}"
            );
        }
        for (vault, named) in [
            ("/x/.hidden.kdbx", "hidden 2026-10-01.kdbx"),
            ("/x/Work.KDBX", "Work 2026-10-01.kdbx"),
            ("/x/archive.tar.kdbx", "archive.tar 2026-10-01.kdbx"),
            ("/x/no extension", "no extension 2026-10-01.kdbx"),
            ("/x/kdbx", "kdbx 2026-10-01.kdbx"),
        ] {
            assert_eq!(name(Path::new(vault), today, None), named, "{vault}");
        }

        let odd = "a\u{202E}b\nc";
        assert_eq!(
            name(Path::new(&format!("/{odd}.kdbx")), today, None),
            format!("{odd} 2026-10-01.kdbx")
        );
    }

    /// A folder somebody filled with every numbered name of the day is not
    /// searched for ever: the plain name is offered, and the write's refusal
    /// of a name that holds a file is what the reader then meets. One name
    /// short of full, the last one is found.
    #[test]
    fn a_folder_holding_every_numbered_name_is_offered_the_plain_one() {
        let folder = scratch();
        let today = first_of_october();
        let vault = Path::new("/Users/someone/Coffer/vault.kdbx");

        std::fs::write(folder.path().join("vault 2026-10-01.kdbx"), b"").expect("written");
        for number in 2..NAMES {
            std::fs::write(
                folder
                    .path()
                    .join(format!("vault 2026-10-01 {number}.kdbx")),
                b"",
            )
            .expect("written");
        }
        assert_eq!(
            name(vault, today, Some(folder.path())),
            format!("vault 2026-10-01 {NAMES}.kdbx")
        );

        std::fs::write(
            folder.path().join(format!("vault 2026-10-01 {NAMES}.kdbx")),
            b"",
        )
        .expect("written");
        assert_eq!(
            name(vault, today, Some(folder.path())),
            "vault 2026-10-01.kdbx"
        );
    }
}
