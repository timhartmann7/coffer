//! Putting the window's own buttons where the title bar draws them.
//!
//! The frame is hidden and the title bar is drawn in HTML, so macOS keeps the
//! close, minimise and zoom buttons where a standard title bar would have put
//! them: near the top, in a band half the height of Coffer's. Tauri's
//! `trafficLightPosition` cannot fix it, because the inset it stores is applied
//! when the native view redraws and the webview covers that view for good.
//!
//! So the three buttons are moved here instead. The hard part is not the
//! arithmetic, it is *when*: AppKit lays them out again on every resize, and a
//! correction that arrives even one frame later is a resize that shows them
//! jumping between the system's position and Coffer's, once per frame.
//!
//! Measured on a real window, in this order:
//!
//! - AppKit puts the middle of a button a fixed distance below the top of the
//!   *window*, whatever the title bar container is doing. Resizing that
//!   container, which is what `tao` does for a window it draws itself, moves
//!   nothing.
//! - The container's own frame notification arrives **before** AppKit places
//!   the buttons, so a correction made there is overwritten immediately.
//! - A button's own frame notification arrives **after**. That is the moment,
//!   and it is the only one: correcting from there holds through every resize.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{NSButton, NSView, NSViewFrameDidChangeNotification, NSWindow, NSWindowButton};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint};
use tauri::{Runtime, WebviewWindow, WindowEvent};

/// The height of the title bar in `frontend/src/lib/components/Titlebar.svelte`,
/// which is `h-11` in the mockup. The buttons are centred in it.
const TITLE_BAR: f64 = 44.0;

/// How far the leftmost button sits from the left edge. The bar's own padding
/// is `px-4`, and the buttons line up with it.
const FROM_LEFT: f64 = 16.0;

thread_local! {
    /// What each window is watching, so that it can stop when the window goes.
    ///
    /// On the main thread and nowhere else, which is where every one of these
    /// is made and dropped: an AppKit reference is not a thing to send between
    /// threads, and the notifications only arrive here anyway.
    static WATCHED: RefCell<HashMap<String, Watch>> = RefCell::new(HashMap::new());

    /// Whether a correction is already running. Moving one button tells the
    /// other two that a frame changed, and AppKit answers a move with a layout
    /// of its own; without this the three of them talk each other into a stack
    /// overflow.
    static CORRECTING: Cell<bool> = const { Cell::new(false) };
}

/// The observers one window holds, taken off the notification centre when it is
/// dropped.
struct Watch {
    tokens: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        let centre = NSNotificationCenter::defaultCenter();
        for token in self.tokens.drain(..) {
            // SAFETY: the token came from this centre and is removed once.
            unsafe { centre.removeObserver(ProtocolObject::as_ref(&*token)) };
        }
    }
}

/// Centres the window buttons, and keeps them centred.
pub fn centre_buttons<R: Runtime>(window: &WebviewWindow<R>) {
    let Some(native) = native(window) else {
        return;
    };

    balance(window);
    watch(window.label(), &native);

    let handle = window.clone();
    let label = window.label().to_owned();
    window.on_window_event(move |event| match event {
        // A window that has gone stops being watched. The buttons are the
        // system's and outlive nothing, but the observers are Coffer's, and
        // slice 4 builds this window again on every unlock.
        WindowEvent::Destroyed => unwatch(&handle, label.clone()),
        // The notification covers every layout AppKit does on its own. This
        // covers what it might not: a move to a display of another scale, where
        // the buttons change size. It moves nothing that is already in place,
        // so it can never be the late half of a flicker.
        WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => balance(&handle),
        _ => {}
    });
}

fn native<R: Runtime>(window: &WebviewWindow<R>) -> Option<Retained<NSWindow>> {
    let handle = window.ns_window().ok()?;

    // SAFETY: `ns_window` hands back the `NSWindow` this webview lives in, and
    // AppKit owns it for as long as the window is open.
    unsafe { Retained::retain(handle.cast::<NSWindow>()) }
}

/// Asks each button to say when its frame changes, and puts it back when it
/// does.
fn watch(label: &str, window: &NSWindow) {
    if MainThreadMarker::new().is_none() {
        return;
    }

    let centre = NSNotificationCenter::defaultCenter();
    let mut tokens = Vec::new();

    for which in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        let Some(button) = window.standardWindowButton(which) else {
            continue;
        };
        button.setPostsFrameChangedNotifications(true);

        let held: Retained<NSView> = Retained::into_super(Retained::into_super(button.clone()));
        let block = RcBlock::new(move |_: std::ptr::NonNull<NSNotification>| {
            if CORRECTING.get() {
                return;
            }
            // A notification about this view's frame arrives on the thread
            // that changed it, which is the thread that draws.
            let Some(window) = held.window() else {
                return;
            };

            CORRECTING.set(true);
            lay_out(&window);
            CORRECTING.set(false);
        });

        // SAFETY: the observer is registered against a button of a live window,
        // and the token it hands back is removed when `Watch` is dropped.
        let token = unsafe {
            centre.addObserverForName_object_queue_usingBlock(
                Some(NSViewFrameDidChangeNotification),
                Some(&*button),
                None,
                &block,
            )
        };
        tokens.push(token);
    }

    WATCHED.with(|watched| {
        watched
            .borrow_mut()
            .insert(label.to_owned(), Watch { tokens })
    });
}

fn unwatch<R: Runtime>(window: &WebviewWindow<R>, label: String) {
    if MainThreadMarker::new().is_some() {
        WATCHED.with(|watched| watched.borrow_mut().remove(&label));
        return;
    }

    let _ = window.run_on_main_thread(move || {
        WATCHED.with(|watched| watched.borrow_mut().remove(&label));
    });
}

fn balance<R: Runtime>(window: &WebviewWindow<R>) {
    let Some(native) = native(window) else {
        return;
    };

    if MainThreadMarker::new().is_some() {
        lay_out(&native);
        return;
    }

    let native = SendWindow(native);
    let _ = window.run_on_main_thread(move || {
        // The whole carrier is moved across, not the reference inside it: that
        // reference is the half that is not `Send`.
        let carried = &native;
        lay_out(&carried.0);
    });
}

/// A window on its way to the main thread.
///
/// `Retained<NSWindow>` is deliberately not `Send`, because most of AppKit is
/// only safe on the main thread. This carries the reference across and holds it
/// there: the message may be queued, and a window released in the meantime
/// would leave the closure with nothing to move.
struct SendWindow(Retained<NSWindow>);

// SAFETY: the window is only ever touched by the closure below, which runs on
// the main thread. Holding the reference is what keeps it alive until then, and
// retaining and releasing are themselves thread-safe.
unsafe impl Send for SendWindow {}

/// Puts the three buttons where the title bar draws them, and leaves alone
/// anything that is already there.
///
/// Doing nothing when there is nothing to do is what lets this be called from
/// more than one place: a call that arrives late finds the work done rather
/// than moving a button a second time.
fn lay_out(window: &NSWindow) {
    let (Some(close), Some(miniaturise), Some(zoom)) = (
        window.standardWindowButton(NSWindowButton::CloseButton),
        window.standardWindowButton(NSWindowButton::MiniaturizeButton),
        window.standardWindowButton(NSWindowButton::ZoomButton),
    ) else {
        return;
    };

    // The buttons sit in the view the system keeps for the title bar it is not
    // drawing, which is anchored to the top of the window and stays the height
    // a standard title bar would be.
    // SAFETY: reading the view hierarchy of a live window on the main thread.
    let Some(inside) = (unsafe { close.superview() }) else {
        return;
    };

    // AppKit counts from the bottom, and the bottom of that view is where a
    // standard title bar would end rather than where Coffer's does.
    let height = close.frame().size.height;
    let above = ((TITLE_BAR - height) / 2.0).max(0.0);
    let bottom = (inside.frame().size.height - above - height).max(0.0);

    // Whatever the system leaves between them, kept as it is. A run that
    // measured nothing would pile all three on top of each other, so it does
    // not run at all.
    let spacing = miniaturise.frame().origin.x - close.frame().origin.x;
    if spacing <= 0.0 {
        return;
    }

    for (index, each) in [close, miniaturise, zoom].into_iter().enumerate() {
        settle(
            &each,
            NSPoint::new(FROM_LEFT + index as f64 * spacing, bottom),
        );
    }
}

/// Moves one button, and only when it is not already there.
fn settle(button: &NSButton, wanted: NSPoint) {
    let at = button.frame().origin;
    if (at.x - wanted.x).abs() < 0.5 && (at.y - wanted.y).abs() < 0.5 {
        return;
    }
    button.setFrameOrigin(wanted);
}
