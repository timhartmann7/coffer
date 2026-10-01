//! The database Coffer offers to open next time.
//!
//! One path, in one file, in the application's own configuration directory. It
//! is not a secret and it is not the user's data, but it says where the vault
//! lives, so it is written owner-only like everything else Coffer writes.
//!
//! The path is stored as the bytes the filesystem gave, not as text: a name
//! that is not valid UTF-8 is still a name, and a vault Coffer cannot remember
//! is a vault the user has to find again on every launch.

use std::io;
use std::path::{Path, PathBuf};

const REMEMBERED: &str = "last-database";

pub fn remembered(directory: &Path) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;

    let bytes = std::fs::read(directory.join(REMEMBERED)).ok()?;
    if bytes.is_empty() {
        return None;
    }

    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

/// Writes down the vault that has just opened, so that the next launch offers
/// it.
///
/// A snapshot and the copy a lock left are never written down, however they
/// came to be open. Each opens with the vault's password and is an older copy
/// of it, and a launch that offered one as the vault would be offering the
/// passwords of last week as the ones in use.
///
/// Nothing is written when the path is the one already there, which is every
/// unlock of the same vault after the first. The staged write flushes the disk
/// twice, and an unlock has a budget.
pub fn remember(directory: &Path, database: &Path) -> Result<(), io::Error> {
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt as _;

    if vault_core::storage::reserved(database) || remembered(directory).as_deref() == Some(database)
    {
        return Ok(());
    }

    std::fs::create_dir_all(directory)?;

    // The same staged write the database itself gets: a file killed halfway
    // through would hold half a path, and half a path is a directory Coffer
    // would offer to open. The writer creates the file owner-only, which a file
    // that was already there with a wider mode would otherwise keep.
    vault_core::storage::atomic::write_atomic::<io::Error, _>(
        &directory.join(REMEMBERED),
        |writer: &mut dyn Write| writer.write_all(database.as_os_str().as_bytes()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_remembered_until_something_is() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        assert_eq!(remembered(directory.path()), None);
    }

    #[test]
    fn the_path_comes_back_as_it_went_in() {
        let directory = tempfile::tempdir().expect("a scratch directory");

        for path in [
            "/Users/someone/Vault/personal.kdbx",
            "/Users/someone/Vault/clés ユニコード.kdbx",
            "/Users/someone/a directory with spaces/x.kdbx",
            "/Users/someone/new\nline.kdbx",
        ] {
            remember(directory.path(), Path::new(path)).expect("it is written");
            assert_eq!(remembered(directory.path()), Some(PathBuf::from(path)));
        }
    }

    /// A file name is a sequence of bytes, and a database whose name is not
    /// text is still a database.
    #[test]
    fn a_path_that_is_not_utf8_survives() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt as _;

        let directory = tempfile::tempdir().expect("a scratch directory");
        let broken = PathBuf::from(OsString::from_vec(b"/tmp/\xff\xfe.kdbx".to_vec()));

        remember(directory.path(), &broken).expect("it is written");
        assert_eq!(remembered(directory.path()), Some(broken));
    }

    #[test]
    fn a_configuration_directory_that_is_not_there_yet_is_made() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let nested = directory.path().join("Application Support/Coffer");

        remember(&nested, Path::new("/x.kdbx")).expect("it is written");
        assert_eq!(remembered(&nested), Some(PathBuf::from("/x.kdbx")));
    }

    #[test]
    fn only_the_owner_can_read_where_the_vault_is() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("a scratch directory");
        remember(directory.path(), Path::new("/x.kdbx")).expect("it is written");

        let mode = std::fs::metadata(directory.path().join(REMEMBERED))
            .expect("it is there")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    /// A snapshot or a rescue copy opened by the reader is still not the
    /// vault, and the vault that was written down stays written down.
    #[test]
    fn a_file_kept_beside_a_vault_is_never_remembered() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let vault = Path::new("/Users/someone/Coffer/vault.kdbx");
        remember(directory.path(), vault).expect("it is written");

        for beside in [
            "/Users/someone/Coffer/vault.kdbx.1.bak",
            "/Users/someone/Coffer/vault.kdbx.10.bak",
            "/Users/someone/Coffer/vault.kdbx.unsaved.kdbx",
        ] {
            remember(directory.path(), Path::new(beside)).expect("nothing to write");
            assert_eq!(remembered(directory.path()), Some(vault.to_path_buf()));
        }

        // Nor on a first run, with nothing written down before it.
        let empty = tempfile::tempdir().expect("a scratch directory");
        remember(empty.path(), Path::new("/x.kdbx.unsaved.kdbx")).expect("nothing to write");
        assert_eq!(remembered(empty.path()), None);
    }

    /// The same vault unlocked every morning is written once. Each write is a
    /// new file renamed into place, so the file itself says whether one
    /// happened.
    #[test]
    fn the_same_vault_again_writes_nothing() {
        use std::os::unix::fs::MetadataExt as _;

        let directory = tempfile::tempdir().expect("a scratch directory");
        let written = directory.path().join(REMEMBERED);
        let inode = || std::fs::metadata(&written).expect("it is there").ino();

        remember(directory.path(), Path::new("/a.kdbx")).expect("it is written");
        let first = inode();
        for _ in 0..3 {
            remember(directory.path(), Path::new("/a.kdbx")).expect("nothing to write");
            assert_eq!(inode(), first, "the same path was written again");
        }

        remember(directory.path(), Path::new("/b.kdbx")).expect("it is written");
        assert_ne!(inode(), first, "a different vault was not written down");
        assert_eq!(remembered(directory.path()), Some(PathBuf::from("/b.kdbx")));
    }

    #[test]
    fn an_empty_file_is_not_a_path() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        std::fs::write(directory.path().join(REMEMBERED), b"").expect("it is written");
        assert_eq!(remembered(directory.path()), None);
    }
}
