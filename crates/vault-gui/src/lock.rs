//! Locking, in the one place the timer, the machine, the button and the close
//! button all reach.
//!
//! Locking destroys the window. It does not hide one: a hidden window is a
//! webview that still holds every value the reader looked at, in a heap nothing
//! in this process can reach, and the page is torn down so that there is
//! nothing left holding them.
//!
//! The order below is the whole of what this module is. The tree goes first and
//! synchronously, because destroying a window is a message to the event loop
//! and a Mac going to sleep will not wait for it.
//!
//! The tree is also written out first, inside [`Session::lock`], with what the
//! reader was typing and had not finished written into it before that (see
//! [`crate::drafts`]). A vault is dirty exactly when saving is the thing that
//! failed - somebody else wrote the file, the volume went, the folder turned
//! read only - so a wipe on its own is a session's work ended by a timer nobody
//! was watching. What the file will not
//! take goes into a copy beside it. What cannot be written anywhere is wiped all
//! the same: a vault left unlocked because it had unsaved work would be the
//! whole of the locking gone.

use std::sync::Arc;

use tauri::{AppHandle, Manager, Runtime};

use crate::autolock::Reason;
use crate::session::Session;
use crate::{clipboard, context, window};

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

    // What a menu under the pointer is about is names and ids out of the tree
    // that just went. A menu still open keeps the window's destroy waiting
    // until it is dismissed; nothing chosen in it reaches anything, and one
    // asked for and not drawn yet is not drawn.
    context::forget(app);

    // A vault that locked with the password still on the clipboard is a lock
    // that did not lock.
    clipboard::revoke_pending();

    // The heap is covered by the allocator; this is the stack the key
    // derivation ran on.
    vault_core::scrub::stack();

    if let Some(main) = app.get_webview_window(&window::label()) {
        // Coffer is closing, or the reader closed the window, so the window is
        // not wanted back. Every other reason is a lock, and a lock is a window
        // that returns asking for a password.
        if reason.comes_back() {
            window::rebuilding();
        }

        // Destroyed rather than closed. Closing runs the path a page can veto -
        // Tauri prevents a close whenever the webview has a close-requested
        // listener - and a lock that JavaScript can refuse is not a lock.
        let _ = main.destroy();
    }
}
