//! Locking, in the one place the timer, the machine and the button all reach.
//!
//! Locking destroys the window. It does not hide one: a hidden window is a
//! webview that still holds every value the reader looked at, in a heap nothing
//! in this process can reach, and the page is torn down so that there is
//! nothing left holding them.
//!
//! The order below is the whole of what this module is. The tree goes first and
//! synchronously, because destroying a window is a message to the event loop
//! and a Mac going to sleep will not wait for it.

use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime};

use crate::autolock::Reason;
use crate::session::Session;
use crate::{clipboard, window};

/// Wipes the vault and takes the window down with it.
pub fn lock<R: Runtime>(app: &AppHandle<R>, reason: Reason) {
    let Some(session) = app.try_state::<Arc<Session>>() else {
        return;
    };

    // Exactly one caller gets `true`, which is what stops two triggers arriving
    // together destroying two windows - the second of them being the one the
    // first one's rebuild had just made.
    if !session.lock(reason) {
        return;
    }

    // A vault that locked with the password still on the clipboard is a lock
    // that did not lock.
    clipboard::revoke_pending();

    // The heap is covered by the allocator; this is the stack the key
    // derivation ran on.
    vault_core::scrub::stack();

    if let Some(main) = app.get_webview_window(window::MAIN) {
        // Destroyed rather than closed. Closing runs the path a page can veto -
        // Tauri prevents a close whenever the webview has a close-requested
        // listener - and a lock that JavaScript can refuse is not a lock.
        let _ = main.destroy();
    }
}
