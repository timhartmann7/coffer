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

pub fn remember(directory: &Path, database: &Path) -> Result<(), io::Error> {
    use std::io::Write as _;
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::fs::OpenOptionsExt as _;

    std::fs::create_dir_all(directory)?;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(directory.join(REMEMBERED))?;

    file.write_all(database.as_os_str().as_bytes())
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

    #[test]
    fn an_empty_file_is_not_a_path() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        std::fs::write(directory.path().join(REMEMBERED), b"").expect("it is written");
        assert_eq!(remembered(directory.path()), None);
    }
}
