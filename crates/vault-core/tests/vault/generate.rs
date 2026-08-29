//! The password generator: what it draws from, and whether it draws evenly.

use std::collections::HashMap;

use vault_core::VaultError;
use vault_core::generate::{Alphabet, Recipe, password};

/// What the generator promises to draw from, written out here so that a change
/// to the alphabets is a change to this test rather than a silent one.
const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!\"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~";

fn recipe(length: usize, alphabets: &[Alphabet], similar: bool) -> Recipe {
    Recipe {
        length,
        alphabets: alphabets.to_vec(),
        similar,
    }
}

#[test]
fn a_password_is_made_of_the_characters_that_were_asked_for() {
    for (alphabets, allowed) in [
        (vec![Alphabet::Lower], "abcdefghijkmnopqrstuvwxyz"),
        (vec![Alphabet::Upper], "ABCDEFGHJKLMNPQRSTUVWXYZ"),
        (vec![Alphabet::Digits], "23456789"),
        (vec![Alphabet::Symbols], "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{}~"),
    ] {
        let made = password(&recipe(64, &alphabets, false)).expect("a password is made");
        assert_eq!(made.chars().count(), 64);
        for character in made.chars() {
            assert!(
                allowed.contains(character),
                "a character came out of an alphabet nobody asked for"
            );
        }
    }
}

#[test]
fn a_password_with_nothing_to_draw_from_is_refused_rather_than_made_weak() {
    assert!(matches!(
        password(&recipe(24, &[], false)),
        Err(VaultError::NothingToGenerateFrom)
    ));

    // An alphabet the look-alike rule thins out makes a password from what is
    // left rather than putting a character back: which characters may appear is
    // the reader's decision and not the generator's.
    let digits = password(&recipe(24, &[Alphabet::Digits], false)).expect("a password is made");
    assert!(
        digits
            .chars()
            .all(|character| "23456789".contains(character))
    );
}

#[test]
fn characters_that_look_like_one_another_stay_out_until_they_are_asked_for() {
    let strict = password(&recipe(
        64,
        &[Alphabet::Lower, Alphabet::Upper, Alphabet::Digits],
        false,
    ))
    .expect("a password is made");
    for character in strict.chars() {
        assert!(
            !"0O1lI|".contains(character),
            "a look-alike came through when it was not asked for"
        );
    }

    // Asked for, they are back in the alphabet, which is what the count below
    // measures rather than any one password.
    let mut seen = 0;
    for _ in 0..40 {
        let loose = password(&recipe(
            64,
            &[Alphabet::Lower, Alphabet::Upper, Alphabet::Digits],
            true,
        ))
        .expect("a password is made");
        seen += loose.chars().filter(|c| "0O1lI".contains(*c)).count();
    }
    assert!(seen > 0, "the look-alikes never came back");
}

#[test]
fn a_length_the_screen_could_not_have_asked_for_is_brought_back_into_range() {
    for (asked, expected) in [(8, 8), (64, 64), (24, 24), (0, 8), (1, 8), (10_000, 64)] {
        let made = password(&recipe(asked, &[Alphabet::Lower], false)).expect("a password is made");
        assert_eq!(made.chars().count(), expected, "a length of {asked}");
    }
}

#[test]
fn two_passwords_in_a_row_are_not_the_same_one() {
    let first = password(&recipe(32, &Alphabet::ALL, false)).expect("a password is made");
    let second = password(&recipe(32, &Alphabet::ALL, false)).expect("a password is made");
    assert_ne!(*first, *second);
}

/// A byte taken modulo the size of the alphabet favours the characters at the
/// start of it: with ninety-four characters, the first sixty-eight would come
/// up half as often again as the rest. That is a real weakening of every
/// password the generator makes, and it is invisible in any one of them.
#[test]
fn the_generator_does_not_favour_the_start_of_the_alphabet() {
    let alphabet: Vec<char> = [LOWER, UPPER, DIGITS, SYMBOLS]
        .iter()
        .flat_map(|kind| kind.chars())
        .collect();
    assert_eq!(
        alphabet.len(),
        94,
        "the alphabet is the size the bound assumes"
    );

    let short = 256 % alphabet.len();
    let mut counted: HashMap<char, usize> = HashMap::new();
    let draws = 300 * alphabet.len();

    for _ in 0..(draws / 94) {
        let made = password(&Recipe {
            length: 64,
            alphabets: Alphabet::ALL.to_vec(),
            similar: true,
        })
        .expect("a password is made");
        for character in made.chars() {
            *counted.entry(character).or_default() += 1;
        }
        let made = password(&Recipe {
            length: 30,
            alphabets: Alphabet::ALL.to_vec(),
            similar: true,
        })
        .expect("a password is made");
        for character in made.chars() {
            *counted.entry(character).or_default() += 1;
        }
    }

    let total: usize = counted.values().sum();
    let favoured: usize = alphabet
        .iter()
        .take(short)
        .map(|character| counted.get(character).copied().unwrap_or_default())
        .sum();
    let rest: usize = total - favoured;

    let per_favoured = favoured as f64 / short as f64;
    let per_rest = rest as f64 / (alphabet.len() - short) as f64;
    let ratio = per_favoured / per_rest;

    // A biased draw puts this at 1.5. Ten standard deviations of the count sit
    // well inside the bound, so an honest run does not trip it.
    assert!(
        (0.9..1.1).contains(&ratio),
        "the draw is not even: the first {short} characters came up {ratio} times as often"
    );
    assert_eq!(
        counted.len(),
        alphabet.len(),
        "some character of the alphabet never came up at all"
    );
}
