//! A small file of Coffer's own in the application's configuration directory,
//! and the value it keeps.
//!
//! What the reader chose and what the generator was last asked for each live in
//! one, and both are kept the same way. Neither is a secret and neither is the
//! user's data, but the first says how long a vault stays open, so every such
//! file is written owner-only like everything else Coffer writes.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// A value this process runs on, and the file it is kept in.
///
/// Held rather than read on every question: the clipboard asks for the
/// settings on every copy and the timer on every reset, and neither is a
/// reason to touch the disk. A question is answered from memory and never
/// waits for a write, which matters because some of them are asked on the
/// thread the window is drawn on.
///
/// Writes take turns. Two writes of one file from one process stage it under
/// the same temporary name, and each took the other's away: the file was left
/// missing, or holding a value that was no longer the one in effect. Each
/// write, when its turn comes, writes what is held then, so the last one to
/// land is always the newest.
pub struct Kept<T> {
    held: Mutex<T>,
    writing: Mutex<()>,
    /// Where it is written, or nothing when this Mac has no configuration
    /// directory Coffer can reach. A window that cannot remember what was
    /// chosen still works; it just forgets on the way out.
    directory: Option<PathBuf>,
    name: &'static str,
}

impl<T: Clone + Serialize> Kept<T> {
    /// Holds `found`, which the caller read and settled, and keeps it under
    /// `name` from now on.
    pub fn new(directory: Option<PathBuf>, name: &'static str, found: T) -> Kept<T> {
        Kept {
            held: Mutex::new(found),
            writing: Mutex::new(()),
            directory,
            name,
        }
    }

    fn state(&self) -> MutexGuard<'_, T> {
        self.held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn get(&self) -> T {
        self.state().clone()
    }

    /// Changes what is held and writes it down. `change` answers whether it
    /// changed anything, and nothing changed is nothing written. Held at once,
    /// so a write that fails costs the next launch the value, not this run.
    pub fn update(&self, change: impl FnOnce(&mut T) -> bool) -> Result<(), io::Error> {
        if !change(&mut self.state()) {
            return Ok(());
        }
        let Some(directory) = self.directory.as_deref() else {
            return Ok(());
        };
        let _turn = self
            .writing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        write(directory, self.name, &self.get())
    }
}

/// Reads one, or nothing when there is none that makes sense.
///
/// A file that will not parse is a file somebody broke, and the answer to that
/// is the defaults rather than a window that will not open. The broken file is
/// left where it is: it is the only copy of whatever they were trying to say.
pub fn read<T: DeserializeOwned>(directory: &Path, name: &str) -> Option<T> {
    std::fs::read(directory.join(name))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

pub fn write<T: Serialize>(directory: &Path, name: &str, value: &T) -> Result<(), io::Error> {
    use std::io::Write;

    std::fs::create_dir_all(directory)?;
    let payload = serde_json::to_vec_pretty(value)?;

    // The same staged write the database itself gets, which creates the file
    // owner-only. A file killed halfway through would hold half a document, and
    // half a document is the defaults on the next launch.
    vault_core::storage::atomic::write_atomic::<io::Error, _>(
        &directory.join(name),
        |writer: &mut dyn Write| writer.write_all(&payload),
    )
}
