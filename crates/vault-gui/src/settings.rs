//! What the reader chose, and where it is kept.
//!
//! Five values, in one file, in the application's own configuration directory.
//! None of them is a secret and none of them is the user's data, but they say
//! how long a vault stays open and how long a password stays on the clipboard,
//! so the file is written owner-only like everything else Coffer writes.
//!
//! The lists of what may be chosen live here rather than on the screen. A
//! number the screen cannot show is a number the reader cannot change back, so
//! every value that arrives - from the window, from a file somebody edited, from
//! a version of Coffer that offered something else - is put onto the nearest
//! thing this list holds before anything reads it.
//!
//! The look is the one value that does not settle that way, because a word has
//! no nearest. It settles where it is read instead: a name this version does
//! not know becomes the default, rather than a file Coffer refuses whole.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

const FILE: &str = "settings.json";

/// How long an untouched vault stays open, in seconds.
///
/// There is no "never". The mockup draws a value and no off state, and a vault
/// that never locks itself is the one thing this slice exists to prevent.
pub const IDLE_CHOICES: [u64; 5] = [60, 300, 900, 1800, 3600];

/// How long a copied password stays on the clipboard, in seconds.
pub const CLIPBOARD_CHOICES: [u64; 4] = [15, 30, 60, 300];

/// Which look the window is drawn in.
///
/// `System` is not a look of its own. It is the reader saying the Mac decides,
/// and the two halves resolve it their own way: AppKit for the window, the
/// media query for what is inside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    System,
    /// The spec's default, and the one `design.html` is drawn in.
    #[default]
    Dark,
    Light,
}

/// The three, in the order the mockup puts them.
pub const THEME_CHOICES: [Theme; 3] = [Theme::System, Theme::Dark, Theme::Light];

impl Theme {
    /// The word that goes into the file and across to the window. Written here
    /// and nowhere else: a second list of words would be a second answer to
    /// what the reader chose.
    fn name(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }
}

impl Serialize for Theme {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for Theme {
    /// A word this version does not know is the default rather than a refusal.
    ///
    /// The numbers get that from `settled`, which cannot help here, so the
    /// tolerance sits at the door - and it is the same door for the file and
    /// for the window. A later Coffer offering a fourth look would otherwise
    /// take both timers down with it on the way past.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Theme, D::Error> {
        let said = String::deserialize(deserializer)?;
        Ok(THEME_CHOICES
            .into_iter()
            .find(|theme| theme.name() == said)
            .unwrap_or_default())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub idle_seconds: u64,
    pub clipboard_seconds: u64,
    pub lock_on_sleep: bool,
    pub lock_on_screen_lock: bool,
    pub theme: Theme,
}

impl Default for Settings {
    /// The defaults the spec states: five minutes idle, a minute on the
    /// clipboard, both machine triggers on, and the dark look `design.html` is
    /// drawn in.
    fn default() -> Settings {
        Settings {
            idle_seconds: 300,
            clipboard_seconds: 60,
            lock_on_sleep: true,
            lock_on_screen_lock: true,
            theme: Theme::Dark,
        }
    }
}

impl Settings {
    /// The same settings, with every number forced onto the list the screen
    /// offers. The look is already one of three by construction, so it rides
    /// along on `..self` and a clause for it would be a no-op.
    pub fn settled(self) -> Settings {
        Settings {
            idle_seconds: nearest(self.idle_seconds, &IDLE_CHOICES),
            clipboard_seconds: nearest(self.clipboard_seconds, &CLIPBOARD_CHOICES),
            ..self
        }
    }

    pub fn idle(self) -> Duration {
        Duration::from_secs(self.idle_seconds)
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

        // A file somebody edited by hand, holding numbers this version does not
        // offer. It parses, so the defaults do not cover it: `read` has to
        // settle it, and without that call the screen shows a value it cannot
        // change back to.
        let directory = scratch();
        std::fs::write(
            directory.path().join(FILE),
            br#"{"idleSeconds": 86400, "clipboardSeconds": 7}"#,
        )
        .expect("the file is written");
        let found = read(directory.path());
        assert_eq!(found.idle_seconds, 3600);
        assert_eq!(found.clipboard_seconds, 15);

        // And the same number arriving from the window rather than from a file.
        // What comes back is what was stored, which is what the settings screen
        // draws.
        let held = Preferences::load(Some(directory.path().to_path_buf()));
        let stored = held
            .set(Settings {
                idle_seconds: 86400,
                clipboard_seconds: 7,
                ..Settings::default()
            })
            .expect("it is written");
        assert_eq!(stored.idle_seconds, 3600);
        assert_eq!(stored.clipboard_seconds, 15);
        assert_eq!(held.get(), stored);
        assert_eq!(read(directory.path()), stored);

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
                for theme in THEME_CHOICES {
                    for machine in [true, false] {
                        let wanted = Settings {
                            idle_seconds: idle,
                            clipboard_seconds: clipboard,
                            lock_on_sleep: machine,
                            lock_on_screen_lock: !machine,
                            theme,
                        };
                        write(directory.path(), wanted).expect("it is written");
                        assert_eq!(read(directory.path()), wanted);
                    }
                }
            }
        }
    }

    /// The word is the value. A `Serialize` and a `Deserialize` that disagreed
    /// about one would still round-trip through each other, so the word itself
    /// is what this asserts.
    #[test]
    fn the_word_that_goes_into_the_file_is_the_word_the_window_reads() {
        let written = serde_json::to_string(&Settings::default()).expect("it serialises");
        assert!(
            written.contains(r#""theme":"dark""#),
            "the default look is not written as a bare word: {written}"
        );

        for theme in THEME_CHOICES {
            let word = serde_json::to_string(&theme).expect("it serialises");
            assert!(
                word.starts_with('"') && word.to_lowercase() == word,
                "{theme:?} crosses as {word} rather than as a lowercase word"
            );
            assert_eq!(
                serde_json::from_str::<Theme>(&word).expect("it parses"),
                theme
            );
        }
    }

    /// A copy-paste in `name` that gave two looks one word would make one of
    /// them unreachable from the file and unwritable from the screen, and every
    /// round-trip test would still pass because it goes through the mistake.
    #[test]
    fn no_two_looks_answer_to_the_same_word() {
        let words: Vec<&str> = THEME_CHOICES.iter().map(|theme| theme.name()).collect();
        for (at, word) in words.iter().enumerate() {
            assert!(!words[..at].contains(word), "two looks answer to {word}");
        }
    }

    /// The tolerance is about the name, not about one name. A later Coffer
    /// offering a fourth look must not take the two timers down with it.
    #[test]
    fn a_look_from_a_later_coffer_does_not_take_the_timers_with_it() {
        let directory = scratch();

        for invented in [
            "sepia".to_string(),
            "Dark".to_string(),
            "".to_string(),
            "a".repeat(1024 * 1024),
        ] {
            let document =
                format!(r#"{{"idleSeconds": 900, "clipboardSeconds": 15, "theme": "{invented}"}}"#);
            std::fs::write(directory.path().join(FILE), &document).expect("the file is written");

            let found = read(directory.path());
            assert_eq!(found.theme, Theme::Dark);
            assert_eq!(found.idle_seconds, 900);
            assert_eq!(found.clipboard_seconds, 15);
        }
    }

    /// The other side of that boundary, pinned so that widening it is a
    /// deliberate edit to a test. A number where a word belongs is a broken
    /// file, not a later version of Coffer, and it gets what every other broken
    /// field gets.
    #[test]
    fn a_look_that_is_not_a_word_leaves_the_defaults_standing() {
        let directory = scratch();

        for broken in [
            br#"{"theme": 3}"#.to_vec(),
            br#"{"theme": null}"#.to_vec(),
            br#"{"theme": []}"#.to_vec(),
            br#"{"theme": {"dark": true}}"#.to_vec(),
        ] {
            std::fs::write(directory.path().join(FILE), &broken).expect("the file is written");
            assert_eq!(read(directory.path()), Settings::default());
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
        // The look settles by being one of three rather than by being nearest,
        // so the invariant is stated here rather than the mechanism: whatever
        // the screen offers survives the trip.
        for theme in THEME_CHOICES {
            assert_eq!(
                Settings {
                    theme,
                    ..Settings::default()
                }
                .settled()
                .theme,
                theme
            );
        }
        assert_eq!(Settings::default().theme, Theme::Dark);
        assert_eq!(Settings::default().settled(), Settings::default());
    }
}
