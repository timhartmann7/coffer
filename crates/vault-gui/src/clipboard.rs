//! Putting a secret on the pasteboard, and taking it off again.
//!
//! Three things separate a copied password from anything else on the clipboard.
//! It carries the marker types the clipboard-history tools agree to skip, so it
//! does not land in Maccy, Alfred or Spotlight. It is written for this host
//! only, so Universal Clipboard does not carry it to a phone. And it is removed
//! on a timer, because the pasteboard server holds a copy of the value that no
//! amount of `Zeroizing` on this side can reach.
//!
//! Nothing here ever reads what is on the pasteboard. Reading the general
//! pasteboard programmatically raises a user-facing alert on macOS 15.4 and
//! later and lists Coffer in System Settings for good. The change count is the
//! only thing Coffer looks at, and it is a number, not a value.

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use objc2::rc::Retained;
use objc2_app_kit::{
    NSPasteboard, NSPasteboardContentsOptions, NSPasteboardType, NSPasteboardTypeString,
};
use objc2_foundation::{NSString, ns_string};

/// The type that tells a clipboard manager this value is a secret, and the one
/// that tells it the value is short-lived. Between them they are what keeps a
/// password out of clipboard history: <http://nspasteboard.org>.
fn concealed() -> &'static NSPasteboardType {
    ns_string!("org.nspasteboard.ConcealedType")
}

fn transient() -> &'static NSPasteboardType {
    ns_string!("org.nspasteboard.TransientType")
}

/// Which pasteboard is being written to.
///
/// `NSPasteboard` itself cannot cross a thread boundary, so the timer that
/// clears the value carries this instead and asks for the pasteboard again on
/// the other side.
#[derive(Clone)]
enum Board {
    /// The one every application shares.
    General,
    /// A private pasteboard, which is how the tests write a secret without
    /// touching the clipboard of whoever is running them.
    #[cfg(test)]
    Named(String),
}

impl Board {
    fn open(&self) -> Retained<NSPasteboard> {
        match self {
            Board::General => NSPasteboard::generalPasteboard(),
            #[cfg(test)]
            Board::Named(name) => NSPasteboard::pasteboardWithName(&NSString::from_str(name)),
        }
    }
}

/// What a write leaves behind: the change count the pasteboard had when Coffer
/// wrote to it. It holds no part of the secret, which is what lets it travel to
/// the timer thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Receipt {
    change_count: isize,
}

/// What Coffer last put on a pasteboard and has not taken off again.
///
/// The timer that clears it is a sleeping thread, and a sleeping thread dies
/// with the process. This is what lets the application take the value back on
/// the way out instead of leaving it on the clipboard for good.
static PENDING: Mutex<Option<(Board, Receipt)>> = Mutex::new(None);

fn pending() -> MutexGuard<'static, Option<(Board, Receipt)>> {
    PENDING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Copies a secret to the clipboard and schedules its removal.
pub fn copy(secret: &str, after: Duration) {
    copy_to(Board::General, secret, after);
}

fn copy_to(board: Board, secret: &str, after: Duration) {
    let receipt = write(&board.open(), secret);
    *pending() = Some((board.clone(), receipt));

    // One thread per copy, sleeping. A copy that happened while an earlier
    // timer was still waiting bumps the change count, so the earlier timer
    // finds a pasteboard that is not the one it wrote and leaves it alone.
    std::thread::spawn(move || {
        std::thread::sleep(after);
        revoke(&board.open(), receipt);
        spent(receipt);
    });
}

/// Takes the copied secret off the pasteboard now rather than when its timer
/// says so.
///
/// Coffer calls this as it exits. The alternative is a password that outlives
/// the application on the clipboard of whoever quit while the toast was still
/// counting down, which is the one thing the timer exists to prevent.
pub fn revoke_pending() {
    let Some((board, receipt)) = pending().take() else {
        return;
    };
    revoke(&board.open(), receipt);
}

/// Forgets a receipt once its timer has been and gone, unless a later copy has
/// already replaced it.
fn spent(receipt: Receipt) {
    let mut pending = pending();
    if pending.as_ref().is_some_and(|(_, held)| *held == receipt) {
        *pending = None;
    }
}

fn write(pasteboard: &NSPasteboard, secret: &str) -> Receipt {
    // SAFETY: reading an AppKit constant declared in an `extern "C"` block.
    let plain: &NSPasteboardType = unsafe { NSPasteboardTypeString };

    // Clears the pasteboard and marks the write as belonging to this host, so
    // that Universal Clipboard does not carry the password to another device.
    let change_count =
        pasteboard.prepareForNewContentsWithOptions(NSPasteboardContentsOptions::CurrentHostOnly);

    // The markers go on before the value does. A history tool polling the
    // pasteboard between the two would otherwise see a plain string with
    // nothing telling it to look away.
    let empty = ns_string!("");
    pasteboard.setString_forType(empty, concealed());
    pasteboard.setString_forType(empty, transient());
    pasteboard.setString_forType(&NSString::from_str(secret), plain);

    Receipt { change_count }
}

/// Clears the pasteboard, unless somebody has written to it since.
///
/// Whether the value is still there is answered by the change count and never
/// by reading the contents. Clearing somebody else's clipboard would be a
/// worse failure than leaving a password on it for a minute longer.
fn revoke(pasteboard: &NSPasteboard, receipt: Receipt) -> bool {
    if pasteboard.changeCount() != receipt.change_count {
        return false;
    }

    pasteboard.clearContents();
    true
}

#[cfg(test)]
mod tests {
    use objc2::msg_send;

    use super::*;

    /// A pasteboard of its own for one test, released when the test is done.
    /// Nothing here touches the clipboard the machine is actually using.
    struct Private(Retained<NSPasteboard>);

    impl Private {
        fn new() -> Private {
            Private(NSPasteboard::pasteboardWithUniqueName())
        }

        fn board(&self) -> Board {
            Board::Named(self.0.name().to_string())
        }

        /// What is on this pasteboard, read back.
        ///
        /// Reading is only ever done here, and only on a pasteboard the test
        /// made: reading the general one programmatically raises an alert on
        /// macOS 15.4 and later, which is why nothing outside these tests does.
        fn value(&self) -> Option<String> {
            // SAFETY: reading an AppKit constant declared in an `extern "C"`
            // block.
            let plain: &NSPasteboardType = unsafe { NSPasteboardTypeString };
            self.0.stringForType(plain).map(|value| value.to_string())
        }

        fn types(&self) -> Vec<String> {
            self.0
                .types()
                .map(|types| types.iter().map(|kind| kind.to_string()).collect())
                .unwrap_or_default()
        }
    }

    impl Drop for Private {
        fn drop(&mut self) {
            // SAFETY: `releaseGlobally` takes no arguments and returns
            // `oneway void`, which the bindings skip. The receiver is a
            // pasteboard this test made, never the general one: releasing that
            // would tear down the clipboard for every application running.
            unsafe { msg_send![&*self.0, releaseGlobally] }
        }
    }

    #[test]
    fn a_copied_secret_is_marked_as_one_and_is_the_secret() {
        let private = Private::new();
        write(&private.0, "hunter2");

        assert_eq!(private.value().as_deref(), Some("hunter2"));

        let types = private.types();
        assert!(
            types
                .iter()
                .any(|kind| kind == "org.nspasteboard.ConcealedType"),
            "{types:?}"
        );
        assert!(
            types
                .iter()
                .any(|kind| kind == "org.nspasteboard.TransientType"),
            "{types:?}"
        );
        assert!(
            types.iter().any(|kind| kind == "public.utf8-plain-text"),
            "{types:?}"
        );
    }

    /// Quitting inside the minute is the case the timer cannot cover on its
    /// own: the thread that would have cleared the pasteboard dies with the
    /// process.
    #[test]
    fn the_secret_can_be_taken_back_before_its_timer_runs() {
        let private = Private::new();
        copy_to(private.board(), "hunter2", Duration::from_secs(600));
        assert_eq!(private.value().as_deref(), Some("hunter2"));

        revoke_pending();
        assert!(private.types().is_empty());

        // Nothing is left to revoke, and a second call is not somebody else's
        // clipboard being cleared.
        revoke_pending();
    }

    #[test]
    fn the_timer_takes_the_secret_off_the_clipboard() {
        let private = Private::new();
        copy_to(private.board(), "hunter2", Duration::from_millis(20));
        assert_eq!(private.value().as_deref(), Some("hunter2"));

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !private.types().is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(private.types().is_empty(), "the pasteboard was not cleared");
    }

    #[test]
    fn a_timer_that_is_late_leaves_the_next_value_alone() {
        let private = Private::new();
        let ours = write(&private.0, "hunter2");
        let theirs = write(&private.0, "something somebody else copied");

        assert!(
            !revoke(&private.0, ours),
            "a stale receipt cleared the board"
        );
        assert!(!private.types().is_empty());
        assert!(revoke(&private.0, theirs));
        assert!(private.types().is_empty());
    }

    #[test]
    fn values_that_are_not_ordinary_passwords_survive_the_trip() {
        let private = Private::new();

        for secret in [
            "",
            "10 MB: it is still just a string",
            "\u{202e}drawkcab",
            "line\nbreak\ttab",
            "ユニコード 🔐 é\u{301}",
            "\u{301}\u{301}\u{301}\u{301}",
        ] {
            write(&private.0, secret);
            assert_eq!(private.value().as_deref(), Some(secret), "{secret:?}");
        }

        let long = "a".repeat(10 * 1024 * 1024);
        write(&private.0, &long);
        assert_eq!(private.value().map(|value| value.len()), Some(long.len()));
    }
}
