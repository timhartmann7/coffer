//! Copies of a vault the reader keeps on another disk: where the last ones went
//! and when, and so how long the vault has gone without one.
//!
//! One file in the application's configuration directory (`kept.rs`), keyed by
//! the vault's path, owner-only like everything Coffer writes. Not in the
//! vault: the format has no field for it, and a field of Coffer's own is the one
//! thing `SPEC.md` section 12 rules out. Not in `settings.json` either, which
//! the window sends back whole. Nothing in it is a secret: dates, folders and
//! the names of disks.
//!
//! A copy on another disk is what this is for, and only such a copy counts. A
//! copy on the vault's own disk is written down as well, and said to be where it
//! is, and the vault goes on counting: the day that disk fails, it fails with
//! the vault.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::NaiveDateTime;
use serde::{Deserialize, Deserializer, Serialize};
use vault_core::storage;
use vault_core::{Vault, VaultError};

use crate::disks;
use crate::error::Failure;
use crate::kept::{self, Kept};

pub mod offer;

const FILE: &str = "copies.json";

/// A month without a copy on another disk is when the status bar says so.
const QUIET_DAYS: u64 = 30;

const DAY: u64 = 86_400;

/// How many vaults' copies are kept. Everyone has one or two; a bound is what
/// keeps a file nobody reads from growing with every vault ever opened.
const KEPT_VAULTS: usize = 64;

/// One copy, as it is written down.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Last {
    /// When, in seconds since 1970.
    pub at: u64,
    /// The folder it went into, which the next panel opens on while it is a
    /// folder. Text, lossily: a name that is not text is a folder the panel
    /// would not open on either.
    pub folder: String,
    /// The name of the disk it went to, when that is mounted in `/Volumes`.
    pub volume: Option<String>,
}

/// The newest copy of one vault on another disk, and the newest on its own.
/// Both are kept: the one is what the reminder counts from, and the other is
/// what the reader was last told about.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Copied {
    pub other_disk: Option<Last>,
    pub same_disk: Option<Last>,
}

impl Copied {
    /// When the newest of the two was made.
    fn newest(&self) -> u64 {
        [&self.other_disk, &self.same_disk]
            .into_iter()
            .flatten()
            .map(|last| last.at)
            .max()
            .unwrap_or(0)
    }
}

/// Every vault's copies, by the vault's path as text.
#[derive(Serialize, Clone, Default)]
#[serde(transparent)]
pub struct Copies(BTreeMap<String, Copied>);

impl<'de> Deserialize<'de> for Copies {
    /// A vault's record this version cannot read is left out, rather than
    /// taking every other vault's with it.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Copies, D::Error> {
        let found = BTreeMap::<String, serde_json::Value>::deserialize(deserializer)?;
        Ok(Copies(
            found
                .into_iter()
                .filter_map(|(vault, copied)| Some((vault, serde_json::from_value(copied).ok()?)))
                .collect(),
        ))
    }
}

impl Copies {
    /// Keeps the vaults copied to most recently, up to the bound, and always
    /// `keep`, the one just copied.
    fn trim(&mut self, keep: &str) {
        if self.0.len() <= KEPT_VAULTS {
            return;
        }
        let mut others: Vec<(u64, String)> = self
            .0
            .iter()
            .filter(|(vault, _)| vault.as_str() != keep)
            .map(|(vault, copied)| (copied.newest(), vault.clone()))
            .collect();
        others.sort_unstable_by(|one, other| other.cmp(one));
        others.truncate(KEPT_VAULTS - 1);
        let staying: Vec<String> = others.into_iter().map(|(_, vault)| vault).collect();
        self.0
            .retain(|vault, _| vault.as_str() == keep || staying.contains(vault));
    }
}

pub type Record = Kept<Copies>;

/// What was written down, or nothing when nothing was or what was is not a
/// record of copies. A file that will not read is left where it is, as
/// `kept::read` leaves every one.
pub fn record(directory: Option<PathBuf>) -> Record {
    let found = directory
        .as_deref()
        .and_then(|directory| kept::read::<Copies>(directory, FILE))
        .unwrap_or_default();
    Kept::new(directory, FILE, found)
}

/// A copy just written: what is written down about it, and whether it went to
/// the vault's own disk.
pub struct Landed {
    pub last: Last,
    pub same_disk: bool,
}

impl Landed {
    /// A copy of the vault at `vault` just written into `folder`. Which disk
    /// that is, and what it is called, are asked now, while the folder is
    /// certainly there.
    pub fn at(vault: &Path, folder: &Path, now: u64) -> Landed {
        Landed {
            last: Last {
                at: now,
                folder: folder.to_string_lossy().into_owned(),
                volume: disks::volume(folder),
            },
            same_disk: vault
                .parent()
                .is_none_or(|beside| disks::same(folder, beside)),
        }
    }
}

/// What the window is told about copies of the open vault.
pub struct Told {
    /// The newest copy on another disk.
    pub other_disk: Option<Last>,
    /// The newest copy on the vault's own disk, while it is newer than the
    /// newest on another: the one the reader last made, and one that would go
    /// with the vault.
    pub same_disk: Option<Last>,
    /// Whole days without a copy on another disk: see [`overdue`].
    pub overdue: Option<u64>,
}

impl Kept<Copies> {
    /// The copies of the vault at `vault`, each only when it is believed. One
    /// dated more than a day after `now` is not - a clock that was wrong, or a
    /// file somebody edited - because it would silence the reminder until that
    /// date came round.
    pub fn of(&self, vault: &Path, now: u64) -> Copied {
        let believed = |last: Option<Last>| last.filter(|last| last.at <= now.saturating_add(DAY));
        let copied = self.get().0.remove(&key(vault)).unwrap_or_default();
        Copied {
            other_disk: believed(copied.other_disk),
            same_disk: believed(copied.same_disk),
        }
    }

    /// Writes down a copy of the vault at `vault` that has just landed. Held
    /// at once; a write that fails costs the next launch the date, not the
    /// copy.
    pub fn note(&self, vault: &Path, landed: &Landed) -> Result<(), io::Error> {
        let key = key(vault);
        self.update(|held| {
            let copied = held.0.entry(key.clone()).or_default();
            let slot = if landed.same_disk {
                &mut copied.same_disk
            } else {
                &mut copied.other_disk
            };
            *slot = Some(landed.last.clone());
            held.trim(&key);
            true
        })
    }

    /// What the window is told about copies of what is open: nothing for
    /// anything [`source`] refuses.
    pub fn told(&self, vault: &Vault, now: u64) -> Option<Told> {
        let path = source(vault).ok()?;
        let Copied {
            other_disk,
            same_disk,
        } = self.of(path, now);
        let overdue = overdue(other_disk.as_ref(), vault.made(), vault.count(), now);
        let same_disk =
            same_disk.filter(|near| other_disk.as_ref().is_none_or(|away| near.at > away.at));
        Some(Told {
            other_disk,
            same_disk,
            overdue,
        })
    }
}

/// The file a copy elsewhere is a copy of, or why what is open has none: a
/// format Coffer will not write anywhere, or one of the files Coffer keeps
/// beside a vault - a snapshot, the copy a lock left - which hold an older
/// state of one, and a copy of which is not a copy of the vault.
pub fn source(vault: &Vault) -> Result<&Path, Failure> {
    if let Some(why) = (!vault.copyable()).then(|| vault.read_only()).flatten() {
        return Err(VaultError::from(why).into());
    }
    if storage::reserved(vault.path()) {
        return Err(Failure::refused(
            "open the vault itself to keep a copy of it",
        ));
    }
    Ok(vault.path())
}

/// The vault's path as the record is keyed by it.
fn key(vault: &Path) -> String {
    vault.to_string_lossy().into_owned()
}

/// Whole days without a copy on another disk, once that is a month or more and
/// the vault holds entries; nothing otherwise. Counted from the last such copy,
/// or from when the vault was made when there was none. A vault that does not
/// say when it was made is never counted: there is nothing to count from. One
/// dated before 1970 counts from 1970, which is long enough to say "over a
/// year". One dated after now has had no time to need a copy.
fn overdue(
    away: Option<&Last>,
    made: Option<NaiveDateTime>,
    entries: usize,
    now: u64,
) -> Option<u64> {
    if entries == 0 {
        return None;
    }
    let since = match away {
        Some(last) => last.at,
        None => u64::try_from(made?.and_utc().timestamp()).unwrap_or(0),
    };
    let days = now.saturating_sub(since) / DAY;
    (days >= QUIET_DAYS).then_some(days)
}

/// The clock, in seconds since 1970. A clock set before then reads as 1970.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::PermissionsExt;

    use chrono::NaiveDate;
    use vault_core::LockPolicy;

    use super::*;
    use crate::fixtures::{RICH, SECRET, fixture, password};

    const NOW: u64 = 1_790_000_000;

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().expect("a scratch directory")
    }

    /// The vault at `at`, opened as the session would open it, with the
    /// password every fixture has.
    fn open(at: &Path) -> Result<Vault, VaultError> {
        Vault::open(
            at,
            vault_core::MasterKey::from_password(password(SECRET)),
            LockPolicy::Respect,
        )
    }

    /// A fixture copied to `at` and opened there.
    fn opened(name: &str, at: &Path) -> Vault {
        std::fs::copy(fixture(name), at).expect("the fixture copies");
        open(at).expect("the file opens")
    }

    /// The code a refusal crosses with.
    fn code(refused: Failure) -> String {
        serde_json::to_value(refused).expect("a failure serialises")["code"]
            .as_str()
            .expect("a code")
            .to_owned()
    }

    fn reloaded(directory: &tempfile::TempDir) -> Record {
        record(Some(directory.path().to_owned()))
    }

    fn last(at: u64, folder: &str) -> Last {
        Last {
            at,
            folder: folder.to_owned(),
            volume: None,
        }
    }

    fn landed(at: u64, folder: &str, same_disk: bool) -> Landed {
        Landed {
            last: last(at, folder),
            same_disk,
        }
    }

    fn days(count: u64) -> u64 {
        count * DAY
    }

    /// `days` before [`NOW`], as a date the file would hold.
    fn made(before: u64) -> Option<NaiveDateTime> {
        let at = i64::try_from(NOW - days(before)).expect("a date in range");
        chrono::DateTime::from_timestamp(at, 0).map(|moment| moment.naive_utc())
    }

    #[test]
    fn nothing_copied_yet_is_never() {
        let directory = scratch();
        let vault = Path::new("/Users/someone/Coffer/vault.kdbx");
        assert_eq!(reloaded(&directory).of(vault, NOW), Copied::default());
        assert_eq!(record(None).of(vault, NOW), Copied::default());
    }

    /// One vault's copies are its own, they come back after a launch, and a
    /// copy on the vault's own disk is kept beside the one on another rather
    /// than over it.
    #[test]
    fn a_copy_is_remembered_for_its_own_vault_and_after_a_launch() {
        let directory = scratch();
        let mine = Path::new("/Users/someone/Coffer/vault.kdbx");
        let theirs = Path::new("/Users/someone/Coffer/work.kdbx");

        let held = reloaded(&directory);
        held.note(mine, &landed(NOW - days(3), "/Volumes/Stick", false))
            .expect("it is written");
        held.note(
            mine,
            &landed(NOW - days(1), "/Users/someone/Documents", true),
        )
        .expect("it is written");

        let expected = Copied {
            other_disk: Some(last(NOW - days(3), "/Volumes/Stick")),
            same_disk: Some(last(NOW - days(1), "/Users/someone/Documents")),
        };
        assert_eq!(held.of(mine, NOW), expected);
        assert_eq!(held.of(theirs, NOW), Copied::default());
        drop(held);

        let next = reloaded(&directory);
        assert_eq!(next.of(mine, NOW), expected);
        assert_eq!(next.of(theirs, NOW), Copied::default());
    }

    /// Beside the vault is the vault's disk, and that is what is written down;
    /// a folder on this Mac's disk has no name of a disk to show.
    #[test]
    fn a_copy_beside_the_vault_is_on_the_same_disk() {
        let beside = scratch();
        let elsewhere = scratch();
        let vault = beside.path().join("vault.kdbx");

        for folder in [beside.path(), elsewhere.path()] {
            let landed = Landed::at(&vault, folder, NOW);
            assert!(landed.same_disk, "{}", folder.display());
            assert_eq!(landed.last.volume, None);
            assert_eq!(landed.last.at, NOW);
            assert_eq!(landed.last.folder, folder.to_string_lossy());
        }
    }

    /// A folder on a mount that is not the vault's disk - `/dev`, which is
    /// `devfs` on every Mac - is another disk, and a copy written down there
    /// is one the reminder counts from: the line goes. The one place a copy
    /// is called another disk's on any machine this runs on, so a mount table
    /// that read as nothing would show here.
    #[test]
    fn a_copy_on_another_mount_is_not_on_the_vaults_disk() {
        let directory = scratch();
        let vault = opened(RICH, &directory.path().join("vault.kdbx"));
        let held = record(None);
        assert!(
            held.told(&vault, NOW)
                .and_then(|told| told.overdue)
                .is_some(),
            "the fixture is not overdue to begin with"
        );

        let landed = Landed::at(vault.path(), Path::new("/dev"), NOW);
        assert!(!landed.same_disk);
        assert_eq!(landed.last.volume, None);
        held.note(vault.path(), &landed).expect("noted");

        let told = held.told(&vault, NOW).expect("the vault is told about");
        assert_eq!(told.overdue, None);
        assert_eq!(
            told.other_disk.map(|away| away.folder),
            Some("/dev".to_owned())
        );
        assert!(told.same_disk.is_none());
    }

    #[test]
    fn only_the_owner_can_read_where_copies_went() {
        let directory = scratch();
        reloaded(&directory)
            .note(
                Path::new("/vault.kdbx"),
                &landed(NOW, "/Volumes/Stick", false),
            )
            .expect("it is written");

        let mode = std::fs::metadata(directory.path().join(FILE))
            .expect("the file is there")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    /// A file somebody edited, or another version of Coffer wrote, leaves every
    /// record it can read standing, and a file that is not a record at all is
    /// nothing, and left where it is.
    #[test]
    fn a_record_that_makes_no_sense_leaves_the_others_standing() {
        let directory = scratch();
        let path = directory.path().join(FILE);
        let good = r#""/vault.kdbx": {"otherDisk": {"at": 1789000000, "folder": "/Volumes/Stick", "volume": "Stick"}}"#;
        let long = "x".repeat(1 << 20);

        for broken in [
            r#""/a.kdbx": {"otherDisk": {"at": "yesterday", "folder": "/"}}"#.to_owned(),
            r#""/b.kdbx": null"#.to_owned(),
            r#""/c.kdbx": [1]"#.to_owned(),
            r#""/d.kdbx": {"otherDisk": {"at": -1, "folder": "/"}}"#.to_owned(),
            format!(r#""/e.kdbx": {{"sameDisk": {{"at": 1, "folder": "{long}", "later": true}}}}"#),
        ] {
            std::fs::write(&path, format!("{{{good}, {broken}}}")).expect("the file is written");
            let found = reloaded(&directory);
            assert_eq!(
                found.of(Path::new("/vault.kdbx"), NOW).other_disk,
                Some(Last {
                    at: 1_789_000_000,
                    folder: "/Volumes/Stick".to_owned(),
                    volume: Some("Stick".to_owned()),
                }),
                "{}",
                &broken[..broken.len().min(60)]
            );
        }

        for broken in [
            b"".to_vec(),
            b"{".to_vec(),
            b"null".to_vec(),
            b"[1, 2]".to_vec(),
            b"\xff\xfe not text".to_vec(),
            vec![b'{'; 10 << 20],
        ] {
            std::fs::write(&path, &broken).expect("the file is written");
            let found = reloaded(&directory);
            assert_eq!(found.of(Path::new("/vault.kdbx"), NOW), Copied::default());
            assert_eq!(
                std::fs::read(&path).expect("the file is still there"),
                broken,
                "a file that would not read was not left as it was"
            );
        }

        std::fs::remove_file(&path).expect("the file goes");
        std::fs::create_dir(&path).expect("a directory where the file would be");
        assert_eq!(
            reloaded(&directory).of(Path::new("/vault.kdbx"), NOW),
            Copied::default()
        );
    }

    /// A copy dated past tomorrow would silence the reminder until then, so it
    /// is not believed and the vault counts from when it was made; an hour
    /// ahead is a clock a little out, and is.
    #[test]
    fn a_copy_dated_after_tomorrow_is_not_believed() {
        let directory = scratch();
        let vault = Path::new("/vault.kdbx");
        let held = reloaded(&directory);

        for (at, believed) in [
            (NOW + 3600, true),
            (NOW + DAY, true),
            (NOW + days(2), false),
            (u64::MAX, false),
        ] {
            held.note(vault, &landed(at, "/Volumes/Stick", false))
                .expect("it is written");
            let copied = held.of(vault, NOW);
            assert_eq!(copied.other_disk.is_some(), believed, "{at}");
            let counted = overdue(copied.other_disk.as_ref(), made(40), 1, NOW);
            assert_eq!(counted, (!believed).then_some(40), "{at}");
        }
    }

    /// Ten thousand vaults copied leave the bound, the newest of them, and
    /// always the one just copied, whatever the others' dates say. Held in
    /// memory for the ten thousand, which is where the bound is kept, and
    /// written for the last, which is what the next launch reads.
    #[test]
    fn the_record_keeps_the_newest_vaults_and_always_the_one_just_copied() {
        let held = record(None);
        for number in 0..10_000_u64 {
            held.note(
                Path::new(&format!("/vault {number}.kdbx")),
                &landed(NOW + number, "/Volumes/Stick", false),
            )
            .expect("nowhere to write is not a failure");
        }

        let kept = held.get().0;
        assert_eq!(kept.len(), KEPT_VAULTS);
        for number in (10_000 - KEPT_VAULTS as u64)..10_000 {
            assert!(
                kept.contains_key(&format!("/vault {number}.kdbx")),
                "{number}"
            );
        }

        let directory = scratch();
        let written = Kept::new(Some(directory.path().to_owned()), FILE, held.get());
        written
            .note(
                Path::new("/the oldest.kdbx"),
                &landed(1, "/Volumes/Stick", false),
            )
            .expect("it is written");
        let kept = reloaded(&directory).get().0;
        assert_eq!(kept.len(), KEPT_VAULTS);
        assert!(kept.contains_key("/the oldest.kdbx"));
        assert!(
            !kept.contains_key(&format!("/vault {}.kdbx", 10_000 - KEPT_VAULTS)),
            "the oldest of the others stayed and the bound grew"
        );
    }

    /// A configuration directory Coffer cannot write to costs the next launch
    /// the copy's date, not this run.
    #[test]
    fn a_record_that_cannot_be_written_still_holds_for_this_run() {
        let directory = scratch();
        let blocked = directory.path().join("not a directory");
        std::fs::write(&blocked, b"a file where the directory would go").expect("it is written");

        let held = record(Some(blocked));
        let vault = Path::new("/vault.kdbx");
        assert!(
            held.note(vault, &landed(NOW, "/Volumes/Stick", false))
                .is_err()
        );
        assert_eq!(
            held.of(vault, NOW).other_disk,
            Some(last(NOW, "/Volumes/Stick"))
        );
    }

    /// A vault whose path is not text is kept under its path as text could
    /// say it, and found again under the same.
    #[test]
    fn a_vault_whose_name_is_not_text_is_still_remembered() {
        let directory = scratch();
        let vault = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/\xff\xfe.kdbx".to_vec()));
        reloaded(&directory)
            .note(&vault, &landed(NOW, "/Volumes/Stick", false))
            .expect("it is written");
        assert_eq!(
            reloaded(&directory).of(&vault, NOW).other_disk,
            Some(last(NOW, "/Volumes/Stick"))
        );
    }

    #[test]
    fn quiet_until_a_month_and_never_for_an_empty_vault() {
        let away = |before: u64| last(NOW - days(before), "/Volumes/Stick");
        assert_eq!(overdue(Some(&away(29)), made(400), 1, NOW), None);
        assert_eq!(overdue(Some(&away(30)), made(400), 1, NOW), Some(30));
        assert_eq!(overdue(Some(&away(400)), made(400), 1, NOW), Some(400));
        assert_eq!(overdue(Some(&away(400)), made(400), 0, NOW), None);
        assert_eq!(overdue(None, made(400), 0, NOW), None);
    }

    #[test]
    fn counts_from_when_the_vault_was_made_when_there_was_no_copy() {
        assert_eq!(overdue(None, made(40), 3, NOW), Some(40));
        assert_eq!(overdue(None, made(29), 3, NOW), None);
        let recent = last(NOW - days(3), "/Volumes/Stick");
        assert_eq!(overdue(Some(&recent), made(40), 3, NOW), None);
    }

    #[test]
    fn a_vault_made_in_1600_is_overdue_and_one_made_in_3000_is_not() {
        let year = |year: i32| {
            NaiveDate::from_ymd_opt(year, 1, 1).and_then(|day| day.and_hms_opt(0, 0, 0))
        };
        let old = overdue(None, year(1600), 1, NOW).expect("1600 is long ago");
        assert!(old >= 365, "{old}");
        assert_eq!(overdue(None, year(3000), 1, NOW), None);
    }

    #[test]
    fn a_vault_that_does_not_say_when_it_was_made_is_never_counted() {
        assert_eq!(overdue(None, None, 10, NOW), None);
    }

    /// A clock that reads 1970 counts nothing from a vault made since, and
    /// never wraps round into a number of days nobody could have waited.
    #[test]
    fn a_clock_before_1970_counts_nothing() {
        assert_eq!(overdue(None, made(40), 1, 0), None);
        assert_eq!(
            overdue(Some(&last(NOW, "/Volumes/Stick")), made(40), 1, 0),
            None
        );
    }

    /// Only a vault of its own is told about, refused before any panel
    /// otherwise, and refused for what it is. Its snapshot and the copy a lock
    /// left open with its password and hold an older state of it, and a copy
    /// of either is not a copy of the vault. A KDBX 3 vault holding files is a
    /// format Coffer writes nowhere. A vault on a medium that takes no write is
    /// the one a copy elsewhere is most for.
    #[test]
    fn the_open_vault_is_told_about_only_when_it_can_be_copied() {
        let directory = scratch();
        let held = record(None);

        for (fixture, name, refused) in [
            (RICH, "vault.kdbx", None),
            (RICH, "vault.kdbx.1.bak", Some("refused")),
            (RICH, "vault.kdbx.unsaved.kdbx", Some("refused")),
            ("rich-kdbx31.kdbx", "old.kdbx", Some("readOnly")),
        ] {
            let vault = opened(fixture, &directory.path().join(name));
            assert_eq!(
                held.told(&vault, NOW).is_some(),
                refused.is_none(),
                "{name}"
            );
            assert_eq!(source(&vault).err().map(code).as_deref(), refused, "{name}");
        }

        // Running as root makes every permission test pass without testing
        // anything.
        // SAFETY: geteuid reads a process property and touches nothing.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let place = scratch();
        let path = place.path().join("vault.kdbx");
        std::fs::copy(fixture(RICH), &path).expect("the fixture copies");
        std::fs::set_permissions(place.path(), std::fs::Permissions::from_mode(0o500))
            .expect("the folder is closed");
        let read_only = open(&path);
        std::fs::set_permissions(place.path(), std::fs::Permissions::from_mode(0o700))
            .expect("the folder is opened again");
        let read_only = read_only.expect("a vault on a medium that takes no write opens");
        assert_eq!(read_only.read_only(), Some(vault_core::ReadOnly::Place));
        assert!(held.told(&read_only, NOW).is_some());
        assert!(source(&read_only).is_ok());
    }

    /// A copy on the vault's own disk is said only while it is the newest:
    /// once a copy on another disk follows it, it is the past.
    #[test]
    fn a_copy_on_the_vaults_own_disk_is_told_only_while_it_is_the_newest() {
        let directory = scratch();
        let vault = opened(RICH, &directory.path().join("vault.kdbx"));
        let held = record(None);

        held.note(
            vault.path(),
            &landed(NOW - days(40), "/Volumes/Stick", false),
        )
        .expect("noted");
        held.note(vault.path(), &landed(NOW - days(2), "/Users/someone", true))
            .expect("noted");
        let told = held.told(&vault, NOW).expect("the vault is told about");
        assert_eq!(told.same_disk.map(|near| near.at), Some(NOW - days(2)));
        assert_eq!(
            told.overdue,
            Some(40),
            "a copy on the vault's disk reset the count"
        );

        held.note(
            vault.path(),
            &landed(NOW - days(1), "/Volumes/Stick", false),
        )
        .expect("noted");
        let told = held.told(&vault, NOW).expect("the vault is told about");
        assert!(told.same_disk.is_none());
        assert_eq!(told.overdue, None);
        assert_eq!(told.other_disk.map(|away| away.at), Some(NOW - days(1)));
    }
}
