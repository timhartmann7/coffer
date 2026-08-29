//! Which addresses out of a database Coffer will hand to the system.
//!
//! The URL field is free text. `SPEC.md` calls a `javascript:` URL in it the
//! one real code execution path in this application, so the rule about which
//! schemes may be opened lives here, in one place, and every screen and command
//! that wants to follow an address asks this module first.

/// The schemes Coffer opens. Everything else is text on a screen and nothing
/// more.
const OPENABLE: [&str; 4] = ["http", "https", "mailto", "ftp"];

/// The address to open, when the database's URL field holds one Coffer is
/// willing to hand to the system, and `None` when it does not.
///
/// The value comes back trimmed, and it is the trimmed value that must be
/// opened: whoever opens something other than what was checked has not checked
/// anything. Surrounding whitespace is dropped because a file written by hand
/// often carries it; a control character *inside* the address is a refusal,
/// because browsers and `NSURL` drop tabs and newlines from the middle of a URL
/// before they act on it, and `java<tab>script:` is how that is used.
pub fn openable(url: &str) -> Option<&str> {
    let trimmed =
        url.trim_matches(|character: char| character.is_whitespace() || character.is_control());

    if trimmed.chars().any(char::is_control) {
        return None;
    }

    let (scheme, rest) = trimmed.split_once(':')?;
    if rest.is_empty() {
        return None;
    }

    OPENABLE
        .iter()
        .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
        .then_some(trimmed)
}
