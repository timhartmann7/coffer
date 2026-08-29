//! Handing an address from a database to the system.

use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL};

/// Opens `url` in whatever the system opens it with, if Coffer will open it at
/// all.
///
/// The scheme allowlist is [`vault_core::url::openable`] and the check happens
/// here, in the one function that can launch something, rather than in the
/// screen that asks. `NSURL` parses `javascript:`, `data:` and `file:` URLs
/// perfectly happily, so nothing about the system's own parsing stands between
/// a hostile URL field and whatever it names.
pub fn open(url: &str) -> bool {
    let Some(allowed) = vault_core::url::openable(url) else {
        return false;
    };

    let Some(target) = NSURL::URLWithString(&NSString::from_str(allowed)) else {
        return false;
    };

    NSWorkspace::sharedWorkspace().openURL(&target)
}

#[cfg(test)]
mod tests {
    use super::open;

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
            assert!(!open(url), "{url:?}");
        }
    }
}
