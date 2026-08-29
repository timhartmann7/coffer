//! Handing an address from a database to the system.

use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL};

/// What came of asking the system to open an address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    Yes,
    /// Coffer will not open an address of that kind.
    Refused,
    /// Coffer would have, and the system had nothing to open it with.
    NoHandler,
}

/// Opens `url` in whatever the system opens it with, if Coffer will open it at
/// all.
///
/// The scheme allowlist is [`vault_core::url::openable`] and the check happens
/// here, in the one function that can launch something, rather than in the
/// screen that asks. `NSURL` parses `javascript:`, `data:` and `file:` URLs
/// perfectly happily, so nothing about the system's own parsing stands between
/// a hostile URL field and whatever it names.
pub fn open(url: &str) -> Opened {
    let Some(allowed) = vault_core::url::openable(url) else {
        return Opened::Refused;
    };

    // A scheme Coffer allows can still fail to parse as a URL, and that is the
    // address being unusable rather than Coffer refusing it.
    let Some(target) = NSURL::URLWithString(&NSString::from_str(allowed)) else {
        return Opened::NoHandler;
    };

    if NSWorkspace::sharedWorkspace().openURL(&target) {
        Opened::Yes
    } else {
        Opened::NoHandler
    }
}

#[cfg(test)]
mod tests {
    use super::{Opened, open};

    /// Only the refusals are tested here: a test that opened something would
    /// launch a browser on whoever's machine is running the suite. That the
    /// allowed schemes are the right ones is `vault-core`'s test.
    #[test]
    fn nothing_coffer_will_not_open_reaches_the_system() {
        for url in [
            "javascript:alert(document.domain)",
            "JaVaScRiPt:alert(1)",
            "  javascript:alert(1)",
            "java\tscript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "file:///etc/passwd",
            "vbscript:msgbox(1)",
            "x-apple-shortcuts://run-shortcut?name=wipe",
            "",
            "   ",
            "example.com",
            "https:",
        ] {
            assert_eq!(open(url), Opened::Refused, "{url:?}");
        }
    }
}
