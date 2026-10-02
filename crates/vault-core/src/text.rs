//! What a KeePass file can carry as text.
//!
//! The database's field names and values become XML element content. XML 1.0
//! cannot represent most control characters at all, so a value holding one
//! would be written into a file that no conformant reader, Coffer's own
//! included, could open again. The rule is checked once, here, on the way in.

use crate::error::VaultError;

/// Whether a string can survive a trip through a KeePass file.
///
/// Tab, newline and carriage return are the three control characters XML
/// allows; the rest of C0 and C1, and the two non-characters at the end of the
/// basic plane, are not representable. Rust strings are already valid UTF-8, so
/// surrogates cannot arrive here.
pub(crate) fn writable(text: &str) -> Result<(), VaultError> {
    let acceptable = text.chars().all(|character| match character {
        '\t' | '\n' | '\r' => true,
        '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' => false,
        '\u{fffe}' | '\u{ffff}' => false,
        _ => true,
    });

    if acceptable {
        Ok(())
    } else {
        Err(VaultError::UnwritableText)
    }
}

/// Whether a tag goes into a KeePass file and comes back the same tag.
///
/// The format keeps an entry's tags in one string with semicolons between, and
/// readers split on a comma and a tab as well, so a tag holding one of those
/// comes back as two. A tag padded with spaces comes back trimmed, and an empty
/// one is no tag at all. Each is refused rather than written and silently
/// changed, wherever a tag is written, with a sentence about tags: the file can
/// hold every one of those characters, just not inside a tag.
pub(crate) fn tag(tag: &str) -> Result<(), VaultError> {
    writable(tag)?;
    if tag.is_empty() || tag.trim() != tag || tag.contains([';', ',', '\t']) {
        return Err(VaultError::UnwritableTag);
    }
    Ok(())
}
