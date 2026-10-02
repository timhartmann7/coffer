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
        avoid: String::new(),
    }
}

#[test]
fn a_password_is_made_of_the_characters_that_were_asked_for() {
    for (alphabets, allowed) in [
        (vec![Alphabet::Lower], "abcdefghijkmnopqrstuvwxyz"),
        (vec![Alphabet::Upper], "ABCDEFGHJKLMNPQRSTUVWXYZ"),
        // A PIN keeps 0 and 1: there is no letter beside them to be taken for.
        (vec![Alphabet::Digits], "0123456789"),
        (
            vec![Alphabet::Digits, Alphabet::Upper],
            "23456789ABCDEFGHJKLMNPQRSTUVWXYZ",
        ),
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
    let digits = password(&recipe(24, &[Alphabet::Digits, Alphabet::Symbols], false))
        .expect("a password is made");
    assert!(!digits.chars().any(|character| "01|".contains(character)));
}

/// Leaving 0 and 1 out of four digits leaves 4,096 PINs where there are
/// 10,000, and protects nothing: there is no letter in a PIN to take them for.
/// The look-alike rule, on by default, does not touch one, and the screen is
/// told so rather than offering a switch that would do nothing.
#[test]
fn a_pin_keeps_every_digit_whatever_the_look_alike_rule_says() {
    let pin = recipe(4, &[Alphabet::Digits], false);
    assert_eq!(pin.look_alikes(), "");
    assert_eq!(recipe(8, &Alphabet::ALL, false).look_alikes(), "0O1lI|");

    let mut seen = std::collections::HashSet::new();
    for _ in 0..2_000 {
        seen.extend(password(&pin).expect("a PIN is made").chars());
    }
    assert!(
        seen.contains(&'0') && seen.contains(&'1'),
        "a PIN never held 0 or 1"
    );

    // Asked for by hand, they still stay out.
    let avoided = avoiding(64, &[Alphabet::Digits], "01");
    assert!(
        !password(&avoided)
            .expect("a PIN is made")
            .contains(['0', '1'])
    );
    // And a kind thinned to nothing beside the digits is still not a PIN.
    assert!(!recipe(8, &[Alphabet::Digits, Alphabet::Symbols], false).is_pin());
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
            avoid: String::new(),
        })
        .expect("a password is made");
        for character in made.chars() {
            *counted.entry(character).or_default() += 1;
        }
        let made = password(&Recipe {
            length: 30,
            alphabets: Alphabet::ALL.to_vec(),
            similar: true,
            avoid: String::new(),
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

fn avoiding(length: usize, alphabets: &[Alphabet], avoid: &str) -> Recipe {
    Recipe {
        avoid: avoid.to_owned(),
        ..recipe(length, alphabets, false)
    }
}

/// A card, a phone and a door ask for four or six digits. Eight was the
/// shortest the generator made, so a PIN could not be made at all.
#[test]
fn digits_alone_make_a_pin_as_short_as_four() {
    for (asked, expected) in [(4, 4), (6, 6), (0, 4), (3, 4), (10_000, 64)] {
        let made = password(&recipe(asked, &[Alphabet::Digits], true)).expect("a PIN is made");
        assert_eq!(made.chars().count(), expected, "a PIN of {asked}");
        assert!(made.chars().all(|character| character.is_ascii_digit()));
    }

    // Anything but digits alone is a password, and four of those is not one.
    for alphabets in [
        vec![Alphabet::Lower],
        vec![Alphabet::Digits, Alphabet::Symbols],
        Alphabet::ALL.to_vec(),
    ] {
        let made = password(&recipe(4, &alphabets, false)).expect("a password is made");
        assert_eq!(made.chars().count(), 8, "{alphabets:?} made shorter than 8");
    }

    assert!(recipe(4, &[Alphabet::Digits, Alphabet::Digits], false).is_pin());
    assert!(!recipe(4, &[], false).is_pin());
    assert!(!recipe(4, &[Alphabet::Digits, Alphabet::Lower], false).is_pin());
    assert_eq!(recipe(4, &[Alphabet::Digits], false).lengths(), 4..=64);
    assert_eq!(recipe(4, &Alphabet::ALL, false).lengths(), 8..=64);
}

/// The quote, the apostrophe and the backslash are what a bank turns away.
#[test]
fn characters_to_avoid_never_appear() {
    let refused = "\"'\\`";
    for _ in 0..200 {
        let made =
            password(&avoiding(64, &[Alphabet::Symbols], refused)).expect("a password is made");
        assert!(
            !made.chars().any(|character| refused.contains(character)),
            "a character the reader asked to leave out came through"
        );
    }

    // Leaving out a letter leaves out that letter, not its capital.
    let mut capitals = 0;
    for _ in 0..50 {
        let made = password(&avoiding(64, &[Alphabet::Lower, Alphabet::Upper], "a"))
            .expect("a password is made");
        assert!(!made.contains('a'));
        capitals += made.matches('A').count();
    }
    assert!(capitals > 0, "avoiding a took A out as well");
}

/// Whatever is left is still drawn from evenly, and nothing at all is not a
/// password.
#[test]
fn avoiding_everything_is_refused_rather_than_made_from_nothing() {
    assert!(matches!(
        password(&avoiding(8, &[Alphabet::Digits], "0123456789")),
        Err(VaultError::NothingToGenerateFrom)
    ));

    // Look-alikes already out of a password, and the rest avoided by hand.
    assert!(matches!(
        password(&avoiding(
            8,
            &[Alphabet::Digits, Alphabet::Upper],
            "23456789ABCDEFGHJKLMNPQRSTUVWXYZ"
        )),
        Err(VaultError::NothingToGenerateFrom)
    ));

    // One character left is one character, repeated: thin, but what was asked.
    let made = password(&avoiding(8, &[Alphabet::Digits], "012345678")).expect("one is left");
    assert_eq!(&*made, "99999999");
}

/// What a reader types into "Avoid these characters" is text, and text can be
/// anything at all. None of it may reach the alphabet or slow the draw down.
#[test]
fn characters_to_avoid_that_no_alphabet_holds_change_nothing() {
    let noise = format!(
        "\u{0}\u{202e}\u{2066} \t\n\u{fffd}é😀{}",
        "\u{301}".repeat(1024 * 1024)
    );
    let settled = avoiding(24, &Alphabet::ALL, &noise).settled();
    assert_eq!(settled.avoid, "");

    let made = password(&avoiding(24, &[Alphabet::Lower], &noise)).expect("a password is made");
    assert_eq!(made.chars().count(), 24);
    assert!(made.chars().all(|character| character.is_ascii_lowercase()));
}

#[test]
fn a_settled_recipe_is_one_the_screen_can_show() {
    let settled = Recipe {
        length: 2,
        alphabets: vec![
            Alphabet::Symbols,
            Alphabet::Digits,
            Alphabet::Symbols,
            Alphabet::Digits,
        ],
        similar: true,
        avoid: "~~a\"a\u{0}9".to_owned(),
    }
    .settled();

    assert_eq!(settled.alphabets, vec![Alphabet::Digits, Alphabet::Symbols]);
    assert_eq!(settled.length, 8, "two is no length for a password");
    assert_eq!(
        settled.avoid, "~a\"9",
        "each character once, in the order given"
    );
    assert!(settled.similar);

    assert_eq!(recipe(1, &[Alphabet::Digits], false).settled().length, 4);
    assert_eq!(recipe(5, &[Alphabet::Digits], false).settled().length, 5);
    assert_eq!(
        recipe(usize::MAX, &[Alphabet::Digits], false)
            .settled()
            .length,
        64
    );
    // A recipe with nothing chosen settles too: making a password from it is
    // what refuses, with the sentence that says why.
    let nothing = recipe(0, &[], false).settled();
    assert_eq!((nothing.length, nothing.alphabets.len()), (8, 0));

    // Settling what is already settled changes nothing.
    assert_eq!(settled.clone().settled(), settled);

    // Every character settled into the list to avoid stays out of what is made.
    let made = password(&Recipe {
        length: 64,
        ..settled.clone()
    })
    .expect("a password is made");
    assert!(
        !made
            .chars()
            .any(|character| settled.avoid.contains(character))
    );
}

/// The generator says which kind a password lacks, and does nothing else
/// about it: a password of twelve is made without a digit about one time in
/// three, and that is the even draw, not a fault.
#[test]
fn a_kind_the_password_happens_to_lack_is_named_and_nothing_else() {
    let wanted = recipe(12, &[Alphabet::Lower, Alphabet::Digits], false);

    assert_eq!(wanted.lacking("abcdefghjkmn"), vec![Alphabet::Digits]);
    assert_eq!(wanted.lacking("234567892345"), vec![Alphabet::Lower]);
    assert!(wanted.lacking("abcdefghjk23").is_empty());
    // A kind nobody chose is not missing.
    assert!(!wanted.lacking("abcdefghjk23").contains(&Alphabet::Upper));
    // A kind every character of which is avoided could not have been there.
    let thinned = avoiding(12, &[Alphabet::Lower, Alphabet::Digits], "0123456789");
    assert!(thinned.lacking("abcdefghjkmn").is_empty());
    // So could a kind thinned to nothing by the look-alike rule alone.
    let thinned_by_two_rules = Recipe {
        avoid: "23456789".to_owned(),
        ..recipe(12, &[Alphabet::Lower, Alphabet::Digits], false)
    };
    assert!(thinned_by_two_rules.lacking("abcdefghjkmn").is_empty());

    // Over many draws the even generator does make passwords that lack a kind
    // that was asked for. A generator that never did would be forcing one of
    // each, which is a narrower set of passwords than the one the reader chose.
    let short = recipe(8, &[Alphabet::Lower, Alphabet::Digits], false);
    let lacking = (0..2_000)
        .filter(|_| {
            let made = password(&short).expect("a password is made");
            !short.lacking(&made).is_empty()
        })
        .count();
    assert!(lacking > 0, "every password held every kind");
    assert!(lacking < 2_000, "no password held every kind");
}
