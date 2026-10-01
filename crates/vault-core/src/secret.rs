//! The one type a secret is allowed to leave the vault in.

use std::fmt;

use zeroize::Zeroizing;

use crate::error::VaultError;

/// A value revealed out of the vault: a protected field, or the bytes of an
/// attachment.
///
/// It is deliberately hard to misuse. There is no `Clone`, so a secret cannot be
/// duplicated by accident; no `Display` and no `Serialize`, so it cannot be
/// formatted into a log line or an IPC payload; and `Debug` prints `[redacted]`,
/// so a struct that contains one is safe to print. The buffer is wiped when the
/// value is dropped.
pub struct SecretValue(Zeroizing<Vec<u8>>);

impl SecretValue {
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        SecretValue(Zeroizing::new(bytes))
    }

    /// Hands out the bytes. Every caller of this is a place where a secret can
    /// escape, so there should be very few of them.
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    /// Hands out the bytes as text, or `None` when they are not UTF-8. Field
    /// values are text; attachments are not.
    pub fn expose_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }

    /// The part of a text value between two positions, as a secret of its own.
    ///
    /// The positions are counted in UTF-16 code units, because that is how the
    /// text node a revealed value is shown in counts them, and a selection in
    /// that node is what asks for a part: one recovery code out of ten, one per
    /// line. Counting in bytes or in characters would copy something other
    /// than what the reader selected the moment the value held an accent or
    /// an emoji.
    ///
    /// Refused rather than rounded. A range that is empty or runs backwards, an
    /// end past the last character, and an end between the two halves of a
    /// character outside the basic plane are none of them a selection a reader
    /// can make, and cutting a character in half would copy one the value does
    /// not contain.
    pub fn part(&self, from: usize, to: usize) -> Result<SecretValue, VaultError> {
        let text = self.expose_str().ok_or(VaultError::NoSuchPart)?;
        let (Some(start), Some(end)) = (byte_at(text, from), byte_at(text, to)) else {
            return Err(VaultError::NoSuchPart);
        };

        let taken = text
            .get(start..end)
            .filter(|part| !part.is_empty())
            .ok_or(VaultError::NoSuchPart)?;
        Ok(SecretValue::new(taken.as_bytes().to_vec()))
    }
}

/// Where a position counted in UTF-16 code units falls in the UTF-8 of the same
/// text, or nothing when it is past the end or inside a character.
fn byte_at(text: &str, units: usize) -> Option<usize> {
    let mut counted = 0;
    for (index, character) in text.char_indices() {
        if counted == units {
            return Some(index);
        }
        if counted > units {
            return None;
        }
        counted += character.len_utf16();
    }
    (counted == units).then_some(text.len())
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn secret(text: &str) -> SecretValue {
        SecretValue::new(text.as_bytes().to_vec())
    }

    fn part(text: &str, from: usize, to: usize) -> Option<String> {
        secret(text)
            .part(from, to)
            .ok()
            .and_then(|part| part.expose_str().map(str::to_owned))
    }

    /// What a text node would hand back for the same two positions, which is
    /// the answer `part` has to agree with.
    fn sliced(text: &str, from: usize, to: usize) -> Option<String> {
        let units: Vec<u16> = text.encode_utf16().collect();
        String::from_utf16(units.get(from..to)?).ok()
    }

    /// The case the range exists for: ten recovery codes, one per line, and a
    /// reader who wants the third.
    #[test]
    fn a_line_out_of_several_is_that_line_and_nothing_else() {
        let codes = "1111-aaaa\n2222-bbbb\n3333-cccc\n4444-dddd";

        assert_eq!(part(codes, 20, 29).as_deref(), Some("3333-cccc"));
        assert_eq!(part(codes, 0, 9).as_deref(), Some("1111-aaaa"));
        assert_eq!(part(codes, 30, 39).as_deref(), Some("4444-dddd"));
        assert_eq!(part(codes, 0, 39).as_deref(), Some(codes));
        // The line break itself, when that is what was selected.
        assert_eq!(part(codes, 9, 10).as_deref(), Some("\n"));
    }

    /// A carriage return and a line feed are two units in a text node and two
    /// here, so a value written on Windows is cut where it was selected.
    #[test]
    fn a_crlf_counts_as_the_two_characters_it_is() {
        let codes = "one\r\ntwo";
        assert_eq!(part(codes, 5, 8).as_deref(), Some("two"));
        assert_eq!(part(codes, 3, 5).as_deref(), Some("\r\n"));
    }

    /// An emoji is two units in UTF-16 and four bytes in UTF-8. Either end of
    /// the range may sit on either side of it, and never inside it.
    #[test]
    fn a_character_outside_the_basic_plane_is_taken_whole_or_not_at_all() {
        let text = "a🔐b";

        assert_eq!(part(text, 0, 1).as_deref(), Some("a"));
        assert_eq!(part(text, 1, 3).as_deref(), Some("🔐"));
        assert_eq!(part(text, 3, 4).as_deref(), Some("b"));
        assert_eq!(part(text, 0, 4).as_deref(), Some(text));

        assert!(secret(text).part(2, 4).is_err(), "a range began inside 🔐");
        assert!(secret(text).part(0, 2).is_err(), "a range ended inside 🔐");
        assert!(secret(text).part(2, 2).is_err());
    }

    /// An accent written after its letter is a character of its own, so a
    /// range between the two is a real part of the value rather than half of
    /// one.
    #[test]
    fn a_combining_accent_is_a_character_of_its_own() {
        let text = "e\u{301}\u{301}";
        assert_eq!(part(text, 0, 1).as_deref(), Some("e"));
        assert_eq!(part(text, 1, 3).as_deref(), Some("\u{301}\u{301}"));
    }

    #[test]
    fn a_range_the_value_does_not_have_is_refused() {
        let text = "hunter2";

        for (from, to) in [
            (0, 8),
            (8, 9),
            (7, 7),
            (3, 3),
            (5, 2),
            (usize::MAX, 1),
            (0, usize::MAX),
        ] {
            let refused = secret(text).part(from, to);
            assert!(
                matches!(refused, Err(VaultError::NoSuchPart)),
                "{from}..{to} was not refused"
            );
        }
        assert!(secret("").part(0, 0).is_err());
        assert!(secret("").part(0, 1).is_err());
    }

    /// The refusal names what was wrong and not what was in the value.
    #[test]
    fn a_refusal_says_nothing_of_the_value() {
        let refused = secret("correct horse").part(0, 99);
        assert!(matches!(refused, Err(VaultError::NoSuchPart)));
        if let Err(refused) = refused {
            let said = format!("{refused} {refused:?}");
            assert!(!said.contains("correct"), "{said}");
            assert!(!said.contains("horse"), "{said}");
        }
    }

    /// An attachment is bytes, and a part of bytes counted in UTF-16 is not
    /// anything at all.
    #[test]
    fn a_value_that_is_not_text_has_no_parts() {
        let bytes = SecretValue::new(vec![0xff, 0xfe, 0x00, 0x41]);
        assert!(bytes.part(0, 1).is_err());
    }

    /// A megabyte on one line is still cut where it was asked to be, and the
    /// part is a secret like the whole: nothing of it in `Debug`.
    #[test]
    fn a_megabyte_is_cut_where_it_was_asked_to_be() {
        let long = format!("{}needle{}", "a".repeat(1 << 20), "b".repeat(1 << 20));
        let at = 1 << 20;
        let found = secret(&long).part(at, at + 6);
        assert_eq!(
            found.as_ref().ok().and_then(SecretValue::expose_str),
            Some("needle")
        );
        assert_eq!(format!("{found:?}"), "Ok([redacted])");
    }

    proptest! {
        /// Any two positions a text node could report give the same part a
        /// text node would, and any it could not are refused.
        #[test]
        fn a_part_is_what_the_text_node_would_have_selected(
            text in "(?s).{0,40}",
            from in 0usize..90,
            to in 0usize..90,
        ) {
            let expected = sliced(&text, from, to).filter(|part| !part.is_empty());
            prop_assert_eq!(part(&text, from, to), expected);
        }
    }
}
