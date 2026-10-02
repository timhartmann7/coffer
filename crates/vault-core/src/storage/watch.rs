//! Noticing that the database changed underneath us.
//!
//! Coffer never overwrites a file another client has touched. Every open and
//! every save records what the file looked like, and the next save checks that
//! the file still looks that way.

use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use sha2::{Digest, Sha256};

/// What the database looked like the last time Coffer and the disk agreed.
///
/// Deliberately no timestamps. Neither of the two a file carries can decide this
/// question on both of the machines this crate runs on:
///
/// - macOS writes `com.apple.macl` and `com.apple.provenance` onto a file after
///   an application has touched it through a save panel. That moves change time
///   and nothing else, and a vault that read it as somebody else's edit refused
///   to save its own file.
/// - Linux takes both times from a clock that only advances once a timer tick,
///   so a rewrite of the same length inside one tick leaves every field of this
///   identical - and a vault that trusted that would write over the other
///   client's work without ever asking.
///
/// So what is here is only what cannot be wrong: a file of another length, or at
/// another inode, is a different file. Everything else the bytes answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    len: u64,
    inode: u64,
    device: u64,
}

impl Stamp {
    /// Reads the current state of `path`.
    pub fn of(path: &Path) -> Result<Stamp, io::Error> {
        Ok(Stamp::of_metadata(&std::fs::metadata(path)?))
    }

    /// The state a file was in when this was read about it.
    pub(crate) fn of_metadata(metadata: &std::fs::Metadata) -> Stamp {
        Stamp {
            len: metadata.len(),
            inode: metadata.ino(),
            device: metadata.dev(),
        }
    }
}

/// The bytes of the database, hashed.
///
/// Only ever compared against another of these. It is not a signature and it
/// protects nothing: a writer who can rewrite the vault can rewrite this too.
/// What it answers is narrower - whether the file holds what Coffer put there -
/// and for that a plain digest is enough.
pub type Content = [u8; 32];

pub fn digest(bytes: &[u8]) -> Content {
    Sha256::digest(bytes).into()
}

/// How the file on disk relates to a stamp taken earlier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// The file is the one we last agreed with.
    None,
    /// Somebody wrote it.
    Modified,
    /// It is not there any more.
    Gone,
}

/// Compares `path` against a stamp taken earlier, and against the bytes that
/// went with it.
pub fn since(path: &Path, stamp: Stamp, content: Content) -> Result<Change, io::Error> {
    let current = match Stamp::of(path) {
        Ok(current) => current,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Change::Gone),
        Err(error) => return Err(error),
    };

    // A different length, or a different file at the same name. Neither can be
    // hidden and neither needs the bytes read to be seen.
    if current != stamp {
        return Ok(Change::Modified);
    }

    // And then the bytes, every time. This is the only thing that answers a
    // client which rewrote the file in place: it can put the modification time
    // back, and on Linux it does not even have to.
    Ok(if digest(&std::fs::read(path)?) == content {
        Change::None
    } else {
        Change::Modified
    })
}
