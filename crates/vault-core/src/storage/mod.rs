//! Everything Coffer does to the filesystem around a database file.

pub mod atomic;
pub mod lock;
pub mod snapshot;
pub mod watch;

mod process;

use std::io;
use std::path::{Path, PathBuf};

/// The mode every file Coffer creates is born with. Never applied after the
/// fact: a file that exists for even a moment as world-readable has already
/// leaked.
pub(crate) const OWNER_ONLY: u32 = 0o600;

/// Builds a sibling path by appending a suffix to the database's file name, so
/// that snapshots, lock files and temporary files all land in the directory the
/// user chose. Nothing Coffer writes goes to a system temporary directory: those
/// are world-readable on some systems and survive reboots on others.
pub(crate) fn sibling(database: &Path, suffix: &str) -> Result<PathBuf, io::Error> {
    let name = database.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "database path does not name a file",
        )
    })?;

    let mut name = name.to_os_string();
    name.push(suffix);
    Ok(database.with_file_name(name))
}

/// Flushes the directory entry itself, so that a rename survives a power cut.
/// Without this the renamed file can be durable while the name still points at
/// the old inode.
pub(crate) fn sync_directory(directory: &Path) -> Result<(), io::Error> {
    std::fs::File::open(directory)?.sync_all()
}

/// Whether an error means the place refuses writes rather than that something
/// went wrong.
///
/// A read-only disk image, a Time Machine snapshot, a stick macOS mounted
/// read-only and a share exported read-only all answer `EROFS`. The same share
/// with the permission denied at the far end, a directory belonging to somebody
/// else, and a folder locked in Finder answer `EACCES` or `EPERM`, which reach
/// Rust as one kind. None of them is a failure worth reporting, because reading
/// a database needs no write at all.
///
/// Deliberately only those two. A full volume, a directory that has gone and a
/// disk that is failing are none of them, and a vault that opened read-only on
/// one of those would be hiding a real fault behind a label.
pub(crate) fn refuses_writes(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem
    )
}

pub(crate) fn parent_of(database: &Path) -> Result<&Path, io::Error> {
    database
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "database path has no parent directory",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::refuses_writes;
    use std::io;

    /// The two errnos are the whole of the rule, and which `ErrorKind` the
    /// standard library maps them onto is the one thing a toolchain could
    /// change under it. A read-only mount cannot be made inside a test, so this
    /// is where `EROFS` is pinned at all.
    #[test]
    fn only_a_place_that_will_not_take_a_write_is_read_as_one() {
        for errno in [libc::EACCES, libc::EPERM, libc::EROFS] {
            assert!(
                refuses_writes(&io::Error::from_raw_os_error(errno)),
                "errno {errno} is a place that will not take a write"
            );
        }

        for errno in [libc::ENOSPC, libc::ENOENT, libc::EIO, libc::ENOTDIR] {
            assert!(
                !refuses_writes(&io::Error::from_raw_os_error(errno)),
                "errno {errno} is a fault and must not be read as a read-only place"
            );
        }
    }
}
