//! Coffer's own folder in the reader's home.
//!
//! A first vault goes into it when nobody says otherwise, and a launch that
//! remembers no vault looks in it before greeting the reader as somebody who
//! has none. Both are answers about the same two names, so the names are here
//! and nowhere else.
//!
//! Not `~/Documents`: a Mac set up with the default answers synchronises that
//! folder to iCloud, and a vault Coffer put into a sync folder without being
//! asked is the one thing this application says it does not do.

use std::path::{Path, PathBuf};

/// The folder, directly inside the home folder.
pub const FOLDER: &str = "Coffer";

/// What a first vault is called, and the name a search of the folder prefers.
const FIRST: &str = "vault.kdbx";

/// Where a first vault goes when nobody has said otherwise.
pub fn first(home: &Path) -> PathBuf {
    home.join(FOLDER).join(FIRST)
}

/// A vault already sitting in the folder, for a launch that remembers none.
///
/// Coffer 0.1.0 wrote a vault down when it was picked in a file panel rather
/// than when it opened, so a vault made in Coffer was forgotten by the next
/// launch, which showed the screen for somebody with nothing over a folder
/// holding everything they had. This is what finds it again.
///
/// Only what could be the reader's vault: a file whose name ends in `.kdbx`.
/// Not a snapshot or the copy a lock left, which open with the same password
/// and are older than the vault; not a folder; not an empty file, which is what
/// a creation killed half way leaves; not a name the Finder hides, which on a
/// volume that is not APFS is where macOS keeps a file's attributes. A link
/// counts when it leads to a vault, because a link is how somebody who keeps
/// the file elsewhere would put one here.
///
/// With several, `vault.kdbx` wins, because that is the name Coffer gives the
/// one it makes; after it the one written last, because that is the one in
/// use. Two written at the same moment go by name, so that two launches never
/// offer different files from the same folder.
///
/// A folder that cannot be read is a folder in which nothing was found. This is
/// a courtesy on the first-run screen, and a refusal here would stand between
/// the reader and making a vault at all.
pub fn found(home: &Path) -> Option<PathBuf> {
    let preferred = first(home);
    if is_vault(&preferred) {
        return Some(preferred);
    }

    std::fs::read_dir(home.join(FOLDER))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_vault(path))
        .map(|path| (modified(&path), path))
        .max_by(|(one, first), (other, second)| one.cmp(other).then_with(|| second.cmp(first)))
        .map(|(_, path)| path)
}

/// Where a file is, the way its owner would write it: under their home folder
/// it starts with `~`, which is how `design.html` draws a place. Anywhere else,
/// and on an account with no home folder, it is the whole path.
pub fn shown(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(inside) if !inside.as_os_str().is_empty() => format!("~/{}", inside.display()),
        _ => path.display().to_string(),
    }
}

fn is_vault(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return false;
    };
    let hidden = name.as_encoded_bytes().first() == Some(&b'.');
    // Without regard to case, the way the file panels filter: a Mac volume
    // ignores it, and `Vault.KDBX` is the same vault to the reader.
    let kdbx = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("kdbx"));

    if hidden || !kdbx || vault_core::storage::reserved(path) {
        return false;
    }

    // Followed, so that a link to a vault is a vault and a link to a folder or
    // to nothing is not.
    std::fs::metadata(path).is_ok_and(|about| about.is_file() && about.len() > 0)
}

/// When a file was last written. A filesystem that keeps no time puts it
/// behind every file that has one.
fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path)
        .and_then(|about| about.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    use std::time::{Duration, SystemTime};

    use super::*;

    /// A home folder with Coffer's folder in it, empty.
    fn home() -> (tempfile::TempDir, PathBuf) {
        let home = tempfile::tempdir().expect("a scratch directory");
        let folder = home.path().join(FOLDER);
        std::fs::create_dir(&folder).expect("the folder is made");
        (home, folder)
    }

    /// A file that could be a vault, written at a chosen moment.
    fn vault(at: &Path, written: SystemTime) {
        std::fs::write(at, b"not empty").expect("the file is written");
        File::options()
            .write(true)
            .open(at)
            .and_then(|file| file.set_modified(written))
            .expect("the time is set");
    }

    fn hours_ago(hours: u64) -> SystemTime {
        SystemTime::now() - Duration::from_secs(hours * 3600)
    }

    #[test]
    fn a_first_vault_goes_into_coffer_in_the_home_folder() {
        assert_eq!(
            first(Path::new("/Users/someone")),
            PathBuf::from("/Users/someone/Coffer/vault.kdbx")
        );
    }

    /// The place a vault is made is the first place one is looked for. Two
    /// spellings of it were how a vault made here came to be invisible.
    #[test]
    fn the_vault_coffer_made_is_the_one_it_finds() {
        let (home, folder) = home();
        vault(&first(home.path()), hours_ago(48));
        vault(&folder.join("newer.kdbx"), hours_ago(1));

        assert_eq!(found(home.path()), Some(first(home.path())));
    }

    #[test]
    fn nothing_is_found_where_there_is_no_folder() {
        let home = tempfile::tempdir().expect("a scratch directory");
        assert_eq!(found(home.path()), None);

        // A home that is not there at all, and a folder that is a file.
        assert_eq!(found(&home.path().join("gone")), None);
        std::fs::write(home.path().join(FOLDER), b"a file").expect("the file is written");
        assert_eq!(found(home.path()), None);
    }

    #[test]
    fn nothing_is_found_in_an_empty_folder() {
        let (home, _) = home();
        assert_eq!(found(home.path()), None);
    }

    /// The copy a lock left opens with the vault's password and is older than
    /// the vault. Offered as the vault, it would be yesterday's passwords.
    #[test]
    fn a_rescue_copy_or_a_snapshot_on_its_own_is_not_a_vault() {
        let (home, folder) = home();
        vault(&folder.join("vault.kdbx.unsaved.kdbx"), hours_ago(1));
        vault(&folder.join("vault.kdbx.1.bak"), hours_ago(1));
        vault(&folder.join("vault.kdbx.lock"), hours_ago(1));

        assert_eq!(found(home.path()), None);
    }

    #[test]
    fn a_folder_with_a_vault_s_name_is_not_a_vault() {
        let (home, folder) = home();
        std::fs::create_dir(folder.join("vault.kdbx")).expect("the folder is made");
        std::fs::create_dir(folder.join("x.kdbx")).expect("the folder is made");

        assert_eq!(found(home.path()), None);
    }

    /// A vault kept somewhere else and linked in is found at the link, which
    /// is the name the reader put there. A link to a folder or to nothing is
    /// not a vault.
    #[test]
    fn a_link_counts_only_when_it_leads_to_a_vault() {
        let (home, folder) = home();
        symlink(folder.join("nowhere.kdbx"), folder.join("dangling.kdbx")).expect("a link");
        std::fs::create_dir(home.path().join("elsewhere")).expect("the folder is made");
        symlink(home.path().join("elsewhere"), folder.join("folder.kdbx")).expect("a link");
        assert_eq!(found(home.path()), None);

        let kept = home.path().join("elsewhere/personal.kdbx");
        vault(&kept, hours_ago(1));
        symlink(&kept, folder.join("personal.kdbx")).expect("a link");
        assert_eq!(found(home.path()), Some(folder.join("personal.kdbx")));
    }

    /// Without `vault.kdbx`, the one written last is the one in use. Two
    /// written at the same moment go by name, so the answer does not change
    /// between launches.
    #[test]
    fn of_several_the_newest_is_found_and_a_tie_goes_by_name() {
        let (spread, folder) = home();
        vault(&folder.join("old.kdbx"), hours_ago(72));
        vault(&folder.join("work.kdbx"), hours_ago(2));
        vault(&folder.join("home.kdbx"), hours_ago(24));
        assert_eq!(found(spread.path()), Some(folder.join("work.kdbx")));

        let (tied, folder) = home();
        let moment = hours_ago(5);
        for name in ["charlie.kdbx", "alpha.kdbx", "bravo.kdbx"] {
            vault(&folder.join(name), moment);
        }
        for _ in 0..10 {
            assert_eq!(found(tied.path()), Some(folder.join("alpha.kdbx")));
        }
    }

    /// `vault.kdbx` wins even when something else was written since, because
    /// that is the one Coffer made.
    #[test]
    fn the_name_coffer_gives_its_own_wins_over_a_newer_file() {
        let (home, folder) = home();
        vault(&folder.join("vault.kdbx"), hours_ago(500));
        vault(&folder.join("imported.kdbx"), hours_ago(1));

        assert_eq!(found(home.path()), Some(folder.join("vault.kdbx")));
    }

    /// What a creation killed half way leaves: a name taken and nothing in it.
    /// Offering it would be offering a file that opens as nothing at all.
    #[test]
    fn an_empty_file_is_not_a_vault() {
        let (home, folder) = home();
        std::fs::write(folder.join("vault.kdbx"), b"").expect("the file is written");
        assert_eq!(found(home.path()), None);

        vault(&folder.join("personal.kdbx"), hours_ago(3));
        assert_eq!(found(home.path()), Some(folder.join("personal.kdbx")));
    }

    /// On a volume that is not APFS, macOS keeps a file's attributes in a
    /// hidden twin whose name ends the same way.
    #[test]
    fn a_hidden_file_is_not_a_vault() {
        let (home, folder) = home();
        vault(&folder.join("._vault.kdbx"), hours_ago(1));
        vault(&folder.join(".kdbx"), hours_ago(1));

        assert_eq!(found(home.path()), None);
    }

    #[test]
    fn the_extension_is_read_without_regard_to_case() {
        let (home, folder) = home();
        vault(&folder.join("Personal.KDBX"), hours_ago(1));
        vault(&folder.join("notes.txt"), hours_ago(0));
        vault(&folder.join("vault.kdbx.txt"), hours_ago(0));

        assert_eq!(found(home.path()), Some(folder.join("Personal.KDBX")));
    }

    /// Only the folder itself. A vault in a folder inside it is one the reader
    /// put somewhere on purpose, and the file panel is how they point at it.
    #[test]
    fn a_vault_in_a_folder_inside_is_not_looked_for() {
        let (home, folder) = home();
        std::fs::create_dir(folder.join("old")).expect("the folder is made");
        vault(&folder.join("old/vault.kdbx"), hours_ago(1));

        assert_eq!(found(home.path()), None);
    }

    /// A folder that cannot be read has nothing found in it, and says nothing
    /// about it: the first-run screen still has to offer a new vault.
    #[test]
    fn an_unreadable_folder_is_one_where_nothing_was_found() {
        // SAFETY: geteuid reads a process property and touches nothing.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }

        let (home, folder) = home();
        vault(&folder.join("personal.kdbx"), hours_ago(1));
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o000))
            .expect("the folder is closed");

        let seen = found(home.path());

        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))
            .expect("the folder is opened again");
        assert_eq!(seen, None);
    }

    #[test]
    fn a_place_under_the_home_folder_is_written_from_the_tilde() {
        let home = Some(Path::new("/Users/anna"));

        assert_eq!(
            shown(Path::new("/Users/anna/Coffer/vault.kdbx"), home),
            "~/Coffer/vault.kdbx"
        );
        // A neighbour whose name begins the same way is not inside it.
        assert_eq!(
            shown(Path::new("/Users/annabel/vault.kdbx"), home),
            "/Users/annabel/vault.kdbx"
        );
        assert_eq!(
            shown(Path::new("/Volumes/Stick/vault.kdbx"), home),
            "/Volumes/Stick/vault.kdbx"
        );
        assert_eq!(shown(Path::new("/Users/anna"), home), "/Users/anna");
        assert_eq!(
            shown(Path::new("/Users/anna/Coffer/vault.kdbx"), None),
            "/Users/anna/Coffer/vault.kdbx"
        );
    }
}
