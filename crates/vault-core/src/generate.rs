//! Passwords made of random characters, and nothing cleverer than that.
//!
//! Every character comes from the operating system's randomness through
//! `OsRng`, and every character of the chosen alphabet is equally likely. There
//! is no word list, no pronounceable scheme and no rule that a password must
//! contain one of each kind: each of those narrows the set of passwords the
//! generator can produce, and none of them is ours to invent. What the
//! generator does instead is say which kind a password it made happens to lack
//! ([`Recipe::lacking`]), so that the reader, who knows what the website asks
//! for, can draw another.

use std::ops::RangeInclusive;

use rand_core::{OsRng, TryRngCore};
use zeroize::Zeroizing;

use crate::error::VaultError;

/// The longest password the generator makes.
const LONGEST: usize = 64;

/// The shortest password the generator makes out of anything but digits.
const SHORTEST: usize = 8;

/// The shortest it makes out of digits alone. Four or six digits is what a
/// card, a phone or a door asks for, and a PIN of eight is one nobody can use.
const SHORTEST_PIN: usize = 4;

/// The kinds of character a password can be made of.
///
/// Each is a contiguous run of ASCII except for the symbols, which are every
/// printable ASCII character that is neither a letter, a digit nor a space.
/// Picking a favourite handful out of those would be a rule of our own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet {
    Lower,
    Upper,
    Digits,
    Symbols,
}

impl Alphabet {
    /// Every alphabet, in the order the screen lists them.
    pub const ALL: [Alphabet; 4] = [
        Alphabet::Lower,
        Alphabet::Upper,
        Alphabet::Digits,
        Alphabet::Symbols,
    ];

    /// Every character of the alphabet, in order. The screen writes the
    /// symbols out from here rather than from a list of its own, so what it
    /// says the generator draws from is what it draws from.
    pub fn characters(self) -> &'static str {
        match self {
            Alphabet::Lower => "abcdefghijklmnopqrstuvwxyz",
            Alphabet::Upper => "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
            Alphabet::Digits => "0123456789",
            Alphabet::Symbols => "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~",
        }
    }
}

/// The characters that are hard to tell apart in the fonts a password is read
/// in, left out unless they are asked for. The screen names them from what
/// [`Recipe::look_alikes`] answers.
///
/// Not out of a PIN. Digits alone have no letter or bar to be taken for, and
/// leaving 0 and 1 out of four digits would only leave 4,096 PINs where there
/// are 10,000.
const SIMILAR: &str = "0O1lI|";

/// What to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub length: usize,
    /// The kinds of character to draw from. At least one is needed.
    pub alphabets: Vec<Alphabet>,
    /// Whether characters that look like one another may appear.
    pub similar: bool,
    /// Characters the reader asked to leave out, whichever alphabet they are
    /// in: the quote and the backslash a bank turns away. Leaving characters
    /// out is choosing the set to draw from, which is the reader's to do; what
    /// is left is still drawn from evenly.
    pub avoid: String,
}

impl Default for Recipe {
    /// What the generator opens with before the reader has chosen anything:
    /// twenty-four letters and digits, no look-alikes.
    fn default() -> Recipe {
        Recipe {
            length: 24,
            alphabets: vec![Alphabet::Lower, Alphabet::Upper, Alphabet::Digits],
            similar: false,
            avoid: String::new(),
        }
    }
}

impl Recipe {
    /// Whether this makes a PIN: digits and nothing else.
    pub fn is_pin(&self) -> bool {
        !self.alphabets.is_empty()
            && self
                .alphabets
                .iter()
                .all(|alphabet| *alphabet == Alphabet::Digits)
    }

    /// The lengths a password from this recipe may have.
    ///
    /// The slider on the generator screen offers exactly this range, because
    /// the screen is told it rather than keeping a copy. A length outside it
    /// came from something that is not the screen and is brought back inside
    /// rather than trusted.
    pub fn lengths(&self) -> RangeInclusive<usize> {
        if self.is_pin() {
            SHORTEST_PIN..=LONGEST
        } else {
            SHORTEST..=LONGEST
        }
    }

    /// The same recipe, put in terms the generator keeps: each alphabet once,
    /// a length it makes, and only the characters to avoid that some alphabet
    /// holds, each once, in the order they were given.
    ///
    /// What is remembered between one opening of the generator and the next is
    /// a settled recipe, so a file somebody edited by hand comes back as
    /// something the screen can show.
    pub fn settled(self) -> Recipe {
        let alphabets: Vec<Alphabet> = Alphabet::ALL
            .into_iter()
            .filter(|alphabet| self.alphabets.contains(alphabet))
            .collect();

        let mut avoid = String::new();
        for character in self.avoid.chars() {
            let drawn = Alphabet::ALL
                .iter()
                .any(|alphabet| alphabet.characters().contains(character));
            if drawn && !avoid.contains(character) {
                avoid.push(character);
            }
        }

        let mut settled = Recipe {
            length: self.length,
            alphabets,
            similar: self.similar,
            avoid,
        };
        let lengths = settled.lengths();
        settled.length = settled.length.clamp(*lengths.start(), *lengths.end());
        settled
    }

    /// Whether a character may appear in a password from this recipe, given
    /// that its alphabet was chosen.
    fn allows(&self, character: char) -> bool {
        let thinned = !self.similar && !self.is_pin() && SIMILAR.contains(character);
        !thinned && !self.avoid.contains(character)
    }

    /// The characters the look-alike rule takes out of this recipe: none for a
    /// PIN, which it does not apply to.
    pub fn look_alikes(&self) -> &'static str {
        if self.is_pin() { "" } else { SIMILAR }
    }

    /// The characters a password from this recipe can be made of.
    fn characters(&self) -> Vec<char> {
        let mut found: Vec<char> = Vec::new();
        for alphabet in &self.alphabets {
            for character in alphabet.characters().chars() {
                if self.allows(character) && !found.contains(&character) {
                    found.push(character);
                }
            }
        }
        found
    }

    /// The kinds of character this recipe asked for that a password from it
    /// happens not to hold.
    ///
    /// Drawing evenly means a password of twelve out of eighty-eight
    /// characters goes without a digit about one time in three, and a website
    /// that wants one turns it away. Saying so is all the generator does about
    /// it: drawing again until every kind turned up would be a rule of its
    /// own, and a narrower set of passwords than the one the reader chose. A
    /// kind the recipe leaves nothing of - every digit avoided - is not one the
    /// password could have held, and is not said to be missing.
    pub fn lacking(&self, password: &str) -> Vec<Alphabet> {
        Alphabet::ALL
            .into_iter()
            .filter(|alphabet| self.alphabets.contains(alphabet))
            .filter(|alphabet| {
                let possible = alphabet
                    .characters()
                    .chars()
                    .any(|character| self.allows(character));
                let held = password
                    .chars()
                    .any(|character| alphabet.characters().contains(character));
                possible && !held
            })
            .collect()
    }
}

/// Makes a password.
///
/// The result is wiped when it is dropped, and it is a secret from the moment
/// it exists: it goes to the field it was made for and nowhere else.
pub fn password(recipe: &Recipe) -> Result<Zeroizing<String>, VaultError> {
    let alphabet = recipe.characters();
    if alphabet.is_empty() {
        return Err(VaultError::NothingToGenerateFrom);
    }

    let lengths = recipe.lengths();
    let length = recipe.length.clamp(*lengths.start(), *lengths.end());
    let mut password = Zeroizing::new(String::with_capacity(length * 4));

    for _ in 0..length {
        let at = index(alphabet.len())?;
        password.push(*alphabet.get(at).ok_or(VaultError::RandomnessUnavailable)?);
    }

    Ok(password)
}

/// A number below `count`, drawn so that every value is equally likely.
///
/// A byte taken modulo `count` favours the low values whenever `count` does not
/// divide 256, which for an alphabet of 94 characters means the first 68 of
/// them come up half as often again as the rest. The bytes that would land in
/// the short last run are drawn again instead.
fn index(count: usize) -> Result<usize, VaultError> {
    // The alphabets together are 94 characters, so a single byte covers them.
    let limit = 256 - (256 % count);

    loop {
        let mut byte = [0u8; 1];
        OsRng
            .try_fill_bytes(&mut byte)
            .map_err(|_| VaultError::RandomnessUnavailable)?;

        let drawn = usize::from(byte[0]);
        if drawn < limit {
            return Ok(drawn % count);
        }
    }
}
