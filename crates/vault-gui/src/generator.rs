//! The recipes the password generators open with: for each, the last one that
//! made a password.
//!
//! Held here rather than in the window, because every lock destroys the window,
//! and a reader who set the generator up for their bank's rules set it up again
//! after each one. Written down beside the settings (`kept.rs`), because the
//! same is true of a launch and nothing in it is a secret. Not in the vault: a
//! database somebody opens in another client is no place for how Coffer's
//! reader likes their passwords made, and the format has no field for it.
//!
//! The password's generator and the one under a field of the reader's own each
//! remember their own (see [`dto::Purpose`]).

use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use vault_core::generate::Recipe;

use crate::dto::{self, Purpose};
use crate::kept::{self, Kept};

const FILE: &str = "generator.json";

/// The recipes, one for each generator, in the shape they are written down.
/// A generator the file says nothing about - or says something about that is
/// not a recipe - opens with the default.
#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(default)]
pub struct Recipes {
    password: dto::Recipe,
    field: dto::Recipe,
}

impl Recipes {
    fn of(&self, purpose: Purpose) -> &dto::Recipe {
        match purpose {
            Purpose::Password => &self.password,
            Purpose::Field => &self.field,
        }
    }

    fn of_mut(&mut self, purpose: Purpose) -> &mut dto::Recipe {
        match purpose {
            Purpose::Password => &mut self.password,
            Purpose::Field => &mut self.field,
        }
    }

    /// Each recipe settled, because a file somebody edited by hand can hold a
    /// length or a list the screen has no way to show.
    fn settled(self) -> Recipes {
        let settle = |recipe: dto::Recipe| dto::Recipe::of(&recipe.recipe().settled());
        Recipes {
            password: settle(self.password),
            field: settle(self.field),
        }
    }
}

pub type Remembered = Kept<Recipes>;

/// What was written down, settled, or the defaults when nothing was or what
/// was is not a pair of recipes.
pub fn remembered(directory: Option<PathBuf>) -> Remembered {
    let found = directory
        .as_deref()
        .and_then(|directory| kept::read::<Recipes>(directory, FILE))
        .unwrap_or_default()
        .settled();
    Kept::new(directory, FILE, found)
}

impl Kept<Recipes> {
    /// The recipe a generator opens with.
    pub fn recipe(&self, purpose: Purpose) -> Recipe {
        self.get().of(purpose).recipe()
    }

    /// Remembers the recipe that just made a password, for the generator that
    /// made it.
    ///
    /// Written only when it differs from what that generator held: "Make
    /// another one", and the panel opening again, make a password from the
    /// recipe already held, and that is no reason to flush the disk.
    pub fn keep(&self, purpose: Purpose, recipe: &Recipe) -> Result<(), io::Error> {
        let kept = dto::Recipe::of(recipe);
        self.update(|held| {
            let slot = held.of_mut(purpose);
            if *slot == kept {
                return false;
            }
            *slot = kept;
            true
        })
    }
}

#[cfg(test)]
mod tests {
    use vault_core::generate::Alphabet;

    use super::*;

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().expect("a scratch directory")
    }

    fn pin() -> Recipe {
        Recipe {
            length: 6,
            alphabets: vec![Alphabet::Digits],
            similar: true,
            avoid: String::new(),
        }
    }

    fn banking() -> Recipe {
        Recipe {
            length: 12,
            alphabets: vec![Alphabet::Lower, Alphabet::Digits, Alphabet::Symbols],
            similar: false,
            avoid: "\"'\\`".to_owned(),
        }
    }

    fn reloaded(directory: &tempfile::TempDir) -> Remembered {
        remembered(Some(directory.path().to_owned()))
    }

    #[test]
    fn nothing_kept_yet_is_the_default_recipe() {
        let directory = scratch();
        for purpose in [Purpose::Password, Purpose::Field] {
            assert_eq!(reloaded(&directory).recipe(purpose), Recipe::default());
            assert_eq!(remembered(None).recipe(purpose), Recipe::default());
        }
    }

    /// What the reader set up comes back after a lock, which keeps the process,
    /// and after a launch, which does not.
    #[test]
    fn a_kept_recipe_comes_back_after_a_launch() {
        let directory = scratch();
        let first = reloaded(&directory);

        first
            .keep(Purpose::Password, &banking())
            .expect("it is written");
        assert_eq!(first.recipe(Purpose::Password), banking());
        drop(first);

        let second = reloaded(&directory);
        assert_eq!(second.recipe(Purpose::Password), banking());
        second
            .keep(Purpose::Password, &pin())
            .expect("it is written");
        assert_eq!(reloaded(&directory).recipe(Purpose::Password), pin());
    }

    /// A PIN made for a card's field is not what the password's generator
    /// opens with next: one press of "Put it in the field" would make four
    /// digits the account's password. And the bank's rules set up for the
    /// password are not lost to it.
    #[test]
    fn each_generator_remembers_its_own_recipe() {
        let directory = scratch();
        let held = reloaded(&directory);

        held.keep(Purpose::Password, &banking())
            .expect("it is written");
        held.keep(Purpose::Field, &pin()).expect("it is written");
        assert_eq!(held.recipe(Purpose::Password), banking());
        assert_eq!(held.recipe(Purpose::Field), pin());

        let next = reloaded(&directory);
        assert_eq!(next.recipe(Purpose::Password), banking());
        assert_eq!(next.recipe(Purpose::Field), pin());
    }

    /// A configuration directory Coffer cannot write to costs the next launch
    /// the recipe, not this one.
    #[test]
    fn a_recipe_that_cannot_be_written_down_is_still_held() {
        let directory = scratch();
        let blocked = directory.path().join("not a directory");
        std::fs::write(&blocked, b"a file where the directory would go").expect("it is written");

        let held = remembered(Some(blocked));
        assert!(held.keep(Purpose::Field, &pin()).is_err());
        assert_eq!(held.recipe(Purpose::Field), pin());
    }

    /// A file somebody edited by hand, or one written by another version of
    /// Coffer, comes back as something the screen can show, or as the default.
    #[test]
    fn a_file_that_makes_no_sense_leaves_the_default_or_settles() {
        let directory = scratch();
        let path = directory.path().join(FILE);

        for broken in [
            b"".to_vec(),
            b"{".to_vec(),
            b"null".to_vec(),
            b"[1, 2]".to_vec(),
            br#"{"password": {"alphabets": ["emoji"]}}"#.to_vec(),
            br#"{"field": {"length": "long"}}"#.to_vec(),
            br#"{"length": 12, "alphabets": ["digits"]}"#.to_vec(),
            b"\xff\xfe not text".to_vec(),
        ] {
            std::fs::write(&path, &broken).expect("the file is written");
            let found = reloaded(&directory);
            for purpose in [Purpose::Password, Purpose::Field] {
                assert_eq!(
                    found.recipe(purpose),
                    Recipe::default(),
                    "{}",
                    String::from_utf8_lossy(&broken)
                );
            }
        }

        std::fs::write(
            &path,
            br#"{"field": {"length": 100000, "alphabets": ["digits", "digits"], "avoid": "99\u0000x", "later": true}}"#,
        )
        .expect("the file is written");
        let found = reloaded(&directory);
        let settled = found.recipe(Purpose::Field);
        assert_eq!(settled.length, 64);
        assert_eq!(settled.alphabets, vec![Alphabet::Digits]);
        assert_eq!(settled.avoid, "9x");
        assert!(
            !settled.similar,
            "a switch the file left out takes its default"
        );
        assert_eq!(found.recipe(Purpose::Password), Recipe::default());
    }

    /// "Make another one" makes a password from the recipe already held, and
    /// that is no reason to write the file again. Taking the file away after
    /// the first write is what shows the second wrote nothing.
    #[test]
    fn the_recipe_already_held_is_not_written_again() {
        let directory = scratch();
        let path = directory.path().join(FILE);
        let held = reloaded(&directory);

        held.keep(Purpose::Field, &pin()).expect("it is written");
        std::fs::remove_file(&path).expect("the file was written");
        held.keep(Purpose::Field, &pin())
            .expect("nothing to write is not a failure");
        assert!(!path.exists(), "the same recipe was written twice");
        assert_eq!(held.recipe(Purpose::Field), pin());

        // What is compared is what is held, not what was ever on disk: going
        // to another recipe and back writes both.
        let longer = Recipe { length: 8, ..pin() };
        held.keep(Purpose::Field, &longer).expect("it is written");
        held.keep(Purpose::Field, &pin()).expect("it is written");
        assert_eq!(reloaded(&directory).recipe(Purpose::Field), pin());
    }

    /// A Mac with no configuration directory Coffer can reach still has a
    /// generator that keeps its recipe across locks, which keep the process.
    #[test]
    fn with_nowhere_to_write_a_recipe_lasts_as_long_as_the_process() {
        let held = remembered(None);
        held.keep(Purpose::Password, &pin())
            .expect("nowhere to write is not a failure");
        assert_eq!(held.recipe(Purpose::Password), pin());
    }

    /// The slider makes a password at every step and the window does not wait
    /// for one to come back before asking for the next, and two generators can
    /// be open at once, so recipes are kept from several threads at once.
    /// Whatever is held when they are done is what the window opens with until
    /// the next launch, and the next launch has to open with the same.
    #[test]
    fn recipes_kept_at_once_leave_on_disk_the_ones_that_are_held() {
        let directory = scratch();
        let steps = 16;
        for round in 0..5 {
            let held = reloaded(&directory);
            let start = std::sync::Barrier::new(steps);
            std::thread::scope(|scope| {
                for step in 0..steps {
                    let (held, start) = (&held, &start);
                    scope.spawn(move || {
                        let purpose = if step % 2 == 0 {
                            Purpose::Password
                        } else {
                            Purpose::Field
                        };
                        let recipe = Recipe {
                            length: 8 + step + round,
                            ..Recipe::default()
                        };
                        start.wait();
                        // The command does not answer a failed write either:
                        // the recipe is held whatever the disk says.
                        let _ = held.keep(purpose, &recipe);
                    });
                }
            });

            let next = reloaded(&directory);
            for purpose in [Purpose::Password, Purpose::Field] {
                assert_eq!(next.recipe(purpose), held.recipe(purpose), "round {round}");
            }
        }
    }

    #[test]
    fn the_file_is_written_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let directory = scratch();
        reloaded(&directory)
            .keep(Purpose::Field, &pin())
            .expect("it is written");

        let mode = std::fs::metadata(directory.path().join(FILE))
            .expect("the file is there")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
