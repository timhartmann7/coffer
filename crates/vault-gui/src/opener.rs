//! What Coffer hands the system to open for the reader: an address from a
//! database, and the vault's own file in the Finder.

use std::path::Path;

use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSArray, NSString, NSURL};
use vault_core::storage::{self, OnDisk};

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

/// Why the Finder cannot be asked to show a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unshown {
    /// Nothing is at the name.
    Gone,
    /// A path no file URL can hold: one with a NUL inside it.
    Unusable,
}

/// Whether the Finder can be asked to show `file`: something is at the name,
/// and the name is one a file URL can hold. Asked before anything is handed to
/// the main thread, so that the answer is the caller's to give, and nothing
/// waits on that thread for it.
pub fn showable(file: &Path) -> Result<(), Unshown> {
    if storage::on_disk(file) == OnDisk::Gone {
        return Err(Unshown::Gone);
    }
    NSURL::from_file_path(file)
        .map(drop)
        .ok_or(Unshown::Unusable)
}

/// Selects `file` in a Finder window and brings the Finder forward, so that the
/// reader sees where it lives and can carry it somewhere with the Finder's own
/// tools.
///
/// On the main thread: it hands the screen to another application, and AppKit
/// promises nothing about that from any other. The URL is built from the
/// path's bytes, so a name that is not text is still shown.
pub fn show(file: &Path) {
    if let Some(url) = NSURL::from_file_path(file) {
        NSWorkspace::sharedWorkspace()
            .activateFileViewerSelectingURLs(&NSArray::from_slice(&[&*url]));
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    use super::{Opened, Unshown, open, showable};

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

    /// Only whether a file can be shown is tested: showing one would bring the
    /// Finder forward on whoever's machine is running the suite. Nothing at
    /// the name is gone, a link that leads nowhere is still something, and a
    /// name no URL can hold is said to be.
    #[test]
    fn nothing_that_is_not_there_is_handed_to_the_finder() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let vault = directory.path().join("vault.kdbx");
        std::fs::write(&vault, b"a vault").expect("it is written");
        let link = directory.path().join("link.kdbx");
        std::os::unix::fs::symlink(directory.path().join("nowhere"), &link)
            .expect("the link is made");

        assert_eq!(showable(&vault), Ok(()));
        assert_eq!(showable(&link), Ok(()));
        assert_eq!(
            showable(&directory.path().join("gone.kdbx")),
            Err(Unshown::Gone)
        );
        assert_eq!(showable(&PathBuf::new()), Err(Unshown::Gone));
        assert_eq!(
            showable(&PathBuf::from(OsString::from_vec(b"/tmp/a\0b".to_vec()))),
            Err(Unshown::Unusable)
        );
    }
}
