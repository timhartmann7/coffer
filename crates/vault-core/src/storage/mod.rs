//! Everything Coffer does to the filesystem around a database file.

pub mod atomic;
pub mod lock;
pub mod watch;

mod process;

use std::io;
use std::path::{Path, PathBuf};

/// The mode every file Coffer creates is born with. Never applied after the
/// fact: a file that exists for even a moment as world-readable has already
/// leaked.
pub const OWNER_ONLY: u32 = 0o600;

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
