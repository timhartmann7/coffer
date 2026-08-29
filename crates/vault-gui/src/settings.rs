//! What the reader chose, and where it is kept.
//!
//! Four values, in one file, in the application's own configuration directory.
//! None of them is a secret and none of them is the user's data, but they say
//! how long a vault stays open and how long a password stays on the clipboard,
//! so the file is written owner-only like everything else Coffer writes.
//!
//! The lists of what may be chosen live here rather than on the screen. A
//! number the screen cannot show is a number the reader cannot change back, so
//! every value that arrives - from the window, from a file somebody edited, from
//! a version of Coffer that offered something else - is put onto the nearest
//! thing this list holds before anything reads it.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};

const FILE: &str = "settings.json";

/// How long an untouched vault stays open, in seconds.
///
/// There is no "never". The mockup draws a value and no off state, and a vault
/// that never locks itself is the one thing this slice exists to prevent.
pub const IDLE_CHOICES: [u64; 5] = [60, 300, 900, 1800, 3600];

/// How long a copied password stays on the clipboard, in seconds.
pub const CLIPBOARD_CHOICES: [u64; 4] = [15, 30, 60, 300];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub idle_seconds: u64,
    pub clipboard_seconds: u64,
    pub lock_on_sleep: bool,
    pub lock_on_screen_lock: bool,
}

impl Default for Settings {
    /// The defaults the spec states: five minutes idle, a minute on the
    /// clipboard, and both machine triggers on.
    fn default() -> Settings {
        Settings {
            idle_seconds: 300,
            clipboard_seconds: 60,
            lock_on_sleep: true,
            lock_on_screen_lock: true,
        }
    }
}

impl Settings {
    /// The same settings, with every number forced onto the list the screen
    /// offers.
    pub fn settled(self) -> Settings {
        Settings {
            idle_seconds: nearest(self.idle_seconds, &IDLE_CHOICES),
            clipboard_seconds: nearest(self.clipboard_seconds, &CLIPBOARD_CHOICES),
            ..self
        }
    }

    pub fn clipboard(self) -> Duration {
        Duration::from_secs(self.clipboard_seconds)
    }
}

/// The offered value closest to this one.
///
/// Distance and not rounding down: a file holding an hour and a half is closer
/// to an hour than to half of one, and a reader who edited it by hand meant
/// something nearer the top of the list than the bottom.
fn nearest(wanted: u64, offered: &[u64]) -> u64 {
    offered
        .iter()
        .copied()
        .min_by_key(|choice| choice.abs_diff(wanted))
        .unwrap_or(wanted)
}

/// Reads the settings, and never fails.
///
/// A file that will not parse is a file somebody broke, and the answer to that
/// is the defaults rather than a window that will not open. The broken file is
/// left where it is: it is the only copy of whatever they were trying to say.
pub fn read(directory: &Path) -> Settings {
    std::fs::read(directory.join(FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
        .unwrap_or_default()
        .settled()
}

pub fn write(directory: &Path, settings: Settings) -> Result<(), io::Error> {
    use std::io::Write;

    std::fs::create_dir_all(directory)?;
    let payload = serde_json::to_vec_pretty(&settings)?;

    // The same staged write the database itself gets, which creates the file
    // owner-only. A file killed halfway through would hold half a document, and
    // half a document is the defaults on the next launch.
    vault_core::storage::atomic::write_atomic::<io::Error, _>(
        &directory.join(FILE),
        |writer: &mut dyn Write| writer.write_all(&payload),
    )
}

/// The settings this process is running on.
///
/// Held rather than read on every question: the clipboard asks on every copy
/// and the timer asks on every reset, and neither of those is a reason to touch
/// the disk.
pub struct Preferences {
    held: Mutex<Settings>,
    /// Where they are kept, or nothing when this Mac has no configuration
    /// directory Coffer can reach. A window that cannot remember what was
    /// chosen still works; it just forgets on the way out.
    directory: Option<PathBuf>,
}

impl Preferences {
    pub fn load(directory: Option<PathBuf>) -> Preferences {
        let held = directory.as_deref().map(read).unwrap_or_default();
        Preferences {
            held: Mutex::new(held),
            directory,
        }
    }

    fn state(&self) -> MutexGuard<'_, Settings> {
        self.held
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn get(&self) -> Settings {
        *self.state()
    }

    /// Puts a choice into effect and writes it down.
    ///
    /// Answers with what was actually stored, which is not always what was
    /// asked for: a value the screen does not offer is settled onto one it
    /// does, and the screen draws what came back rather than what it sent.
    pub fn set(&self, wanted: Settings) -> Result<Settings, io::Error> {
        let settled = wanted.settled();
        *self.state() = settled;

        if let Some(directory) = self.directory.as_deref() {
            write(directory, settled)?;
        }
        Ok(settled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().expect("a scratch directory")
    }

    #[test]
    fn nothing_written_yet_reads_as_the_defaults_the_spec_states() {
        let directory = scratch();
        let found = read(directory.path());

        assert_eq!(found, Settings::default());
        assert_eq!(found.clipboard(), Duration::from_secs(60));
        assert_eq!(found.idle_seconds, 300);
        assert!(found.lock_on_sleep && found.lock_on_screen_lock);
    }

    /// A settings file is not a database, and a broken one is not a reason to
    /// refuse to open the window.
    #[test]
    fn a_file_that_makes_no_sense_leaves_the_defaults_standing() {
        let directory = scratch();
        let path = directory.path().join(FILE);

        for broken in [
            b"".to_vec(),
            b"{".to_vec(),
            b"null".to_vec(),
            b"[1, 2, 3]".to_vec(),
            b"\xff\xfe not text".to_vec(),
            vec![b'{'; 10 * 1024 * 1024],
        ] {
            std::fs::write(&path, &broken).expect("the file is written");
            assert_eq!(read(directory.path()), Settings::default());
            assert!(path.is_file(), "the broken file was taken away");
        }

        // A directory where the file should be is not readable either.
        std::fs::remove_file(&path).expect("the file goes");
        std::fs::create_dir(&path).expect("a directory takes its place");
        assert_eq!(read(directory.path()), Settings::default());
    }

    /// One field this version does not know, and one it does. Neither may take
    /// the other down.
    #[test]
    fn a_field_from_another_version_leaves_the_rest_alone() {
        let directory = scratch();
        std::fs::write(
            directory.path().join(FILE),
            br#"{"idleSeconds": 900, "somethingLater": {"deep": true}}"#,
        )
        .expect("the file is written");

        let found = read(directory.path());
        assert_eq!(found.idle_seconds, 900);
        assert_eq!(found.clipboard_seconds, 60);
    }

    /// Numbers nobody offered, and numbers that are not numbers.
    #[test]
    fn a_value_the_screen_cannot_show_is_put_on_the_nearest_one_it_can() {
        assert_eq!(
            Settings {
                idle_seconds: 0,
                clipboard_seconds: 1,
                ..Settings::default()
            }
            .settled(),
            Settings {
                idle_seconds: 60,
                clipboard_seconds: 15,
                ..Settings::default()
            }
        );

        assert_eq!(
            Settings {
                idle_seconds: u64::MAX,
                clipboard_seconds: u64::MAX,
                ..Settings::default()
            }
            .settled(),
            Settings {
                idle_seconds: 3600,
                clipboard_seconds: 300,
                ..Settings::default()
            }
        );

        // Halfway between two offered values goes to the nearer, and a tie goes
        // to the first of them.
        assert_eq!(
            Settings {
                idle_seconds: 600,
                ..Settings::default()
            }
            .settled()
            .idle_seconds,
            300
        );

        let directory = scratch();
        for broken in [
            br#"{"idleSeconds": -1}"#.to_vec(),
            br#"{"idleSeconds": 1.5}"#.to_vec(),
            br#"{"idleSeconds": "five minutes"}"#.to_vec(),
            br#"{"lockOnSleep": "yes"}"#.to_vec(),
        ] {
            std::fs::write(directory.path().join(FILE), &broken).expect("the file is written");
            assert_eq!(read(directory.path()), Settings::default());
        }
    }

    #[test]
    fn every_offered_choice_comes_back_the_way_it_went_in() {
        let directory = scratch();

        for idle in IDLE_CHOICES {
            for clipboard in CLIPBOARD_CHOICES {
                for machine in [true, false] {
                    let wanted = Settings {
                        idle_seconds: idle,
                        clipboard_seconds: clipboard,
                        lock_on_sleep: machine,
                        lock_on_screen_lock: !machine,
                    };
                    write(directory.path(), wanted).expect("it is written");
                    assert_eq!(read(directory.path()), wanted);
                }
            }
        }
    }

    #[test]
    fn only_the_owner_can_read_what_the_reader_chose() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = scratch();
        write(directory.path(), Settings::default()).expect("it is written");

        let mode = std::fs::metadata(directory.path().join(FILE))
            .expect("it is there")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_configuration_directory_that_is_not_there_yet_is_made() {
        let directory = scratch();
        let nested = directory.path().join("Application Support/Coffer");

        write(&nested, Settings::default()).expect("it is written");
        assert_eq!(read(&nested), Settings::default());
    }

    /// A change that cannot be written down is still in effect for this run.
    /// Refusing to apply it would mean a reader who cannot write to their own
    /// configuration directory cannot change a timer at all.
    #[test]
    fn a_change_that_cannot_be_saved_is_still_in_force() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = scratch();
        let held = Preferences::load(Some(directory.path().to_path_buf()));
        assert_eq!(held.get(), Settings::default());

        let wanted = Settings {
            idle_seconds: 900,
            ..Settings::default()
        };
        assert_eq!(held.set(wanted).expect("it is written"), wanted);
        assert_eq!(held.get(), wanted);

        // Running as root makes every permission test pass without testing
        // anything.
        // SAFETY: geteuid reads a process property and touches nothing.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }

        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o500))
            .expect("the directory is closed");
        let further = Settings {
            idle_seconds: 60,
            ..Settings::default()
        };
        assert!(held.set(further).is_err());
        assert_eq!(held.get(), further);

        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
            .expect("the directory is opened again");
    }

    /// Nowhere to keep them is not a reason to refuse to run.
    #[test]
    fn a_mac_with_nowhere_to_keep_them_still_runs_on_them() {
        let held = Preferences::load(None);
        let wanted = Settings {
            clipboard_seconds: 15,
            ..Settings::default()
        };

        assert_eq!(held.set(wanted).expect("it is written"), wanted);
        assert_eq!(held.get(), wanted);
    }

    /// What the screen is offered has to be what the screen may store.
    #[test]
    fn every_offered_value_survives_being_settled() {
        for idle in IDLE_CHOICES {
            assert_eq!(nearest(idle, &IDLE_CHOICES), idle);
        }
        for clipboard in CLIPBOARD_CHOICES {
            assert_eq!(nearest(clipboard, &CLIPBOARD_CHOICES), clipboard);
        }
        assert_eq!(Settings::default().settled(), Settings::default());
    }
}
