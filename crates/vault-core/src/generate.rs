//! Passwords made of random characters, and nothing cleverer than that.
//!
//! Every character comes from the operating system's randomness through
//! `OsRng`, and every character of the chosen alphabet is equally likely. There
//! is no word list, no pronounceable scheme and no rule that a password must
//! contain one of each kind: each of those narrows the set of passwords the
//! generator can produce, and none of them is ours to invent.

use std::ops::RangeInclusive;

use rand_core::{OsRng, TryRngCore};
use zeroize::Zeroizing;

use crate::error::VaultError;

/// The lengths the generator will produce.
///
/// The slider on the generator screen offers exactly this range, so a length
/// outside it came from something that is not the screen and is brought back
/// inside rather than trusted. The two are written down twice, in
/// `docs/ipc.md`, along with the other pairs that have to move together.
const LENGTHS: RangeInclusive<usize> = 8..=64;

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

    fn characters(self) -> &'static str {
        match self {
            Alphabet::Lower => "abcdefghijklmnopqrstuvwxyz",
            Alphabet::Upper => "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
            Alphabet::Digits => "0123456789",
            Alphabet::Symbols => "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~",
        }
    }
}

/// The characters that are hard to tell apart in the fonts a password is read
/// in, left out unless they are asked for.
const SIMILAR: &str = "0O1lI|";

/// What to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub length: usize,
    /// The kinds of character to draw from. At least one is needed.
    pub alphabets: Vec<Alphabet>,
    /// Whether characters that look like one another may appear.
    pub similar: bool,
}

impl Recipe {
    /// The characters a password from this recipe can be made of.
    fn characters(&self) -> Vec<char> {
        let mut found: Vec<char> = Vec::new();
        for alphabet in &self.alphabets {
            for character in alphabet.characters().chars() {
                if !self.similar && SIMILAR.contains(character) {
                    continue;
                }
                if !found.contains(&character) {
                    found.push(character);
                }
            }
        }
        found
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

    let length = recipe.length.clamp(*LENGTHS.start(), *LENGTHS.end());
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
