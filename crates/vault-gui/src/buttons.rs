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
use std::sync::atomic::{AtomicU64, Ordering};

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{
    NSButton, NSView, NSViewFrameDidChangeNotification, NSWindow, NSWindowButton, NSWindowStyleMask,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint};
use tauri::{Runtime, WebviewWindow, WindowEvent};

/// The height of the title bar in `frontend/src/lib/components/Titlebar.svelte`,
/// which is `h-11` in the mockup. The buttons are centred in it.
const TITLE_BAR: f64 = 44.0;

/// How far the leftmost button sits from the left edge. The bar's own padding
/// is `px-4`, and the buttons line up with it.
const FROM_LEFT: f64 = 16.0;

/// How far apart two coordinates may be and still count as the same place.
/// Under anything AppKit moves a button by, and over what a coordinate
/// conversion rounds away.
const SAME_PLACE: f64 = 0.5;

/// Which window a set of observers belongs to.
///
/// Not the window's label. Locking destroys the window and builds another with
/// the same label, and the two overlap: the runtime delivers a window's own
/// `Destroyed` listeners *after* the callback that rebuilt it, so a map keyed
/// by label has the old window's teardown taking the observers off its
/// replacement. The replacement then loses its corrections and its buttons jump
/// on every resize, with no error and nothing leaked - the exact flicker this
/// module exists to prevent, on the first auto-lock.
static NEXT: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// What each window is watching, so that it can stop when the window goes.
    ///
    /// On the main thread and nowhere else, which is where every one of these
    /// is made and dropped: an AppKit reference is not a thing to send between
    /// threads, and the notifications only arrive here anyway.
    static WATCHED: RefCell<HashMap<u64, Watch>> = RefCell::new(HashMap::new());

    /// Whether a correction is already running.
    ///
    /// Moving one button tells the other two that a frame changed, and AppKit
    /// answers a move with a layout of its own, so a pass reached from inside
    /// another one is a pass that would fight it.
    static CORRECTING: Cell<bool> = const { Cell::new(false) };

    /// Whether something asked for a pass while one was running.
    ///
    /// The ask is kept rather than dropped. A frame notification arriving
    /// mid-pass is usually this module's own move and means nothing, but it is
    /// also how AppKit says it has just put a button back, and dropping one of
    /// those leaves that button where AppKit wanted it with nothing left to
    /// notice. It is the last button in the row that it happens to, because
    /// that is the one still being moved when the notification lands.
    static AGAIN: Cell<bool> = const { Cell::new(false) };

    /// The gap AppKit leaves between two buttons, remembered from a row that
    /// was evenly spaced rather than measured again on every pass. See
    /// [`spacing`].
    static SPACING: Cell<f64> = const { Cell::new(0.0) };
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

    let which = NEXT.fetch_add(1, Ordering::Relaxed);
    balance(window);
    watch(which, &native);

    let handle = window.clone();
    window.on_window_event(move |event| match event {
        // A window that has gone stops being watched. The buttons are the
        // system's and outlive nothing, but the observers are Coffer's, and
        // this window is built again on every unlock.
        WindowEvent::Destroyed => unwatch(&handle, which),
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
fn watch(which: u64, window: &NSWindow) {
    if MainThreadMarker::new().is_none() {
        return;
    }

    let centre = NSNotificationCenter::defaultCenter();
    let mut tokens = Vec::new();

    for kind in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        let Some(button) = window.standardWindowButton(kind) else {
            continue;
        };
        button.setPostsFrameChangedNotifications(true);

        let held: Retained<NSView> = Retained::into_super(Retained::into_super(button.clone()));
        let block = RcBlock::new(move |_: std::ptr::NonNull<NSNotification>| {
            // A notification about this view's frame arrives on the thread that
            // changed it, which is the thread that draws.
            if let Some(window) = held.window() {
                lay_out(&window);
            }
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

    WATCHED.with(|watched| watched.borrow_mut().insert(which, Watch { tokens }));
}

fn unwatch<R: Runtime>(window: &WebviewWindow<R>, which: u64) {
    if MainThreadMarker::new().is_some() {
        WATCHED.with(|watched| watched.borrow_mut().remove(&which));
        return;
    }

    let _ = window.run_on_main_thread(move || {
        WATCHED.with(|watched| watched.borrow_mut().remove(&which));
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
///
/// One pass at a time, whichever way it was reached, and never a pass that is
/// lost: an ask arriving mid-pass is answered once the pass is over.
///
/// Twice at most. The second pass is for the button AppKit moved back while the
/// first one was running; a third would only be wanted if AppKit and Coffer were
/// moving the row against each other inside one pass, and spinning here until
/// they agreed would be a hang where the complaint was a flicker.
fn lay_out(window: &NSWindow) {
    one_pass_at_a_time(|| settle_row(window));
}

/// The protocol of the above, with nothing of AppKit in it: a pass runs alone,
/// an ask that arrives while one is running is answered once, and a second ask
/// inside that answer is where it stops.
fn one_pass_at_a_time(mut pass: impl FnMut()) {
    if CORRECTING.replace(true) {
        AGAIN.set(true);
        return;
    }

    pass();
    if AGAIN.replace(false) {
        pass();
    }

    AGAIN.set(false);
    CORRECTING.set(false);
}

fn settle_row(window: &NSWindow) {
    // A full screen takes the title bar out of this window and hangs it in one
    // of AppKit's own, where the row is revealed by a mouse at the top edge
    // rather than drawn by anybody's title bar. A position measured against
    // this window's frame would land nowhere near the buttons that come back.
    if window.styleMask().contains(NSWindowStyleMask::FullScreen) {
        return;
    }

    let (Some(close), Some(miniaturise), Some(zoom)) = (
        window.standardWindowButton(NSWindowButton::CloseButton),
        window.standardWindowButton(NSWindowButton::MiniaturizeButton),
        window.standardWindowButton(NSWindowButton::ZoomButton),
    ) else {
        return;
    };

    // The buttons sit in a view the system keeps for the title bar it is not
    // drawing. Which view that is, and how tall, is AppKit's business.
    // SAFETY: reading the view hierarchy of a live window on the main thread.
    let Some(inside) = (unsafe { close.superview() }) else {
        return;
    };

    let Some(spacing) = spacing(&close, &miniaturise, &zoom) else {
        return;
    };

    // Measured down from the top of the window rather than up from the bottom
    // of the view the buttons are in. That view's height is AppKit's to change,
    // and it changes: on a zoom, on a live resize, on a toolbar arriving. A
    // position taken from it therefore lands somewhere new after each of those,
    // while Coffer's title bar has not moved at all, because it hangs from the
    // top of the window. So the top of the window is what the row is placed
    // against, and the row stopped moving when the window was zoomed.
    let bottom = below_the_top(window.frame().size.height, close.frame().size.height);

    for (index, each) in [close, miniaturise, zoom].into_iter().enumerate() {
        // `None` is the window's own coordinate system, whose origin is the
        // bottom left corner of its frame. Converting rather than assuming is
        // what keeps this right when AppKit gives the view holding the buttons
        // an origin or a height of its own.
        let wanted = inside.convertPoint_fromView(
            NSPoint::new(FROM_LEFT + index as f64 * spacing, bottom),
            None,
        );
        settle(&each, wanted);
    }
}

/// The gap AppKit leaves between two of the buttons.
///
/// Believed only from a row that is evenly spaced, and remembered when it is.
/// The two rows this module ever sees are AppKit's and Coffer's, and Coffer's is
/// AppKit's moved sideways, so both are even and either one answers. A row where
/// AppKit has put some of the buttons back and not the others cannot be even,
/// and the gap across it is a number nobody chose: laying the three of them out
/// on it crowds the row, and the rightmost button, placed two gaps along, moves
/// twice as far as any of the error - which is the one seen jittering.
fn spacing(close: &NSButton, miniaturise: &NSButton, zoom: &NSButton) -> Option<f64> {
    even_gap(
        miniaturise.frame().origin.x - close.frame().origin.x,
        zoom.frame().origin.x - miniaturise.frame().origin.x,
    )
}

/// The arithmetic of the above, with nothing of AppKit in it.
fn even_gap(left: f64, right: f64) -> Option<f64> {
    if left > 0.0 && (left - right).abs() < SAME_PLACE {
        SPACING.set(left);
    }

    // Nothing until a row has been seen whole. A run that guessed would pile
    // all three buttons on top of each other.
    let kept = SPACING.get();
    (kept > 0.0).then_some(kept)
}

/// Where the bottom edge of a button goes, in the window's own coordinates,
/// so that the button is centred in the title bar the window draws itself.
fn below_the_top(window_height: f64, button_height: f64) -> f64 {
    let above = ((TITLE_BAR - button_height) / 2.0).max(0.0);
    window_height - above - button_height
}

/// Moves one button, and only when it is not already there.
fn settle(button: &NSButton, wanted: NSPoint) {
    let at = button.frame().origin;
    if (at.x - wanted.x).abs() < SAME_PLACE && (at.y - wanted.y).abs() < SAME_PLACE {
        return;
    }
    button.setFrameOrigin(wanted);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two windows, one label. The first one's teardown must not take the
    /// second one's observers with it.
    ///
    /// This is the whole of the bug an auto-lock used to have: the runtime
    /// delivers a window's `Destroyed` listeners after the callback that built
    /// its replacement, so a map keyed by label loses the replacement's
    /// corrections on the first relock, silently. Nothing about AppKit is
    /// needed to state it - only that two windows are told apart.
    #[test]
    fn a_window_that_goes_takes_only_its_own_observers() {
        let first = NEXT.fetch_add(1, Ordering::Relaxed);
        let second = NEXT.fetch_add(1, Ordering::Relaxed);
        assert_ne!(first, second);

        WATCHED.with(|watched| {
            let mut watched = watched.borrow_mut();
            watched.insert(first, Watch { tokens: Vec::new() });
            watched.insert(second, Watch { tokens: Vec::new() });

            watched.remove(&first);
            assert!(!watched.contains_key(&first));
            assert!(
                watched.contains_key(&second),
                "the window that went took its replacement's observers with it"
            );

            watched.remove(&second);
        });
    }

    /// The row is anchored to the top of the window, which is where the title
    /// bar that has to hold it is anchored. Reading it off the view the buttons
    /// live in is what used to move them on a zoom: that view's height is
    /// AppKit's, and a taller window put the row somewhere else.
    #[test]
    fn the_row_sits_the_same_distance_below_the_top_whatever_the_window_is_doing() {
        let button = 14.0;
        let mut seen = Vec::new();

        for window in [560.0, 720.0, 1329.5, 2160.0] {
            let bottom = below_the_top(window, button);
            seen.push(window - bottom - button);
        }

        for below in &seen {
            assert!(
                (below - seen[0]).abs() < SAME_PLACE,
                "the row moved when the window was resized: {seen:?}"
            );
        }

        let centre = seen[0] + button / 2.0;
        assert!(
            (centre - TITLE_BAR / 2.0).abs() < SAME_PLACE,
            "the row is not centred in the title bar: {centre} of {TITLE_BAR}"
        );
    }

    /// A button AppKit is taller than the bar Coffer draws would take the row
    /// off the top of the window if the padding above it were allowed to go
    /// negative.
    #[test]
    fn a_button_taller_than_the_bar_is_still_inside_the_window() {
        let window = 720.0;
        let bottom = below_the_top(window, TITLE_BAR + 10.0);
        assert!((window - bottom - (TITLE_BAR + 10.0)).abs() < SAME_PLACE);
    }

    /// The measurement that used to jitter. AppKit lays the row out while a pass
    /// is half way through it, so the gap between the first two buttons is
    /// Coffer's and the gap between the last two is AppKit's - and a row laid
    /// out on the difference puts the rightmost button two of those errors away
    /// from where it belongs.
    #[test]
    fn a_row_that_is_half_moved_is_not_believed() {
        SPACING.set(0.0);
        assert_eq!(
            even_gap(20.0, 20.0),
            Some(20.0),
            "an even row is the answer"
        );

        assert_eq!(
            even_gap(4.0, 20.0),
            Some(20.0),
            "a half moved row was measured, and the buttons crowded"
        );
        assert_eq!(
            even_gap(20.0, 36.0),
            Some(20.0),
            "a half moved row was measured, and the buttons spread"
        );

        assert_eq!(
            even_gap(24.0, 24.0),
            Some(24.0),
            "a row that is even again is the new answer"
        );
    }

    /// Before any row has been seen there is no gap to lay one out on, and
    /// guessing puts all three buttons in the same place.
    #[test]
    fn nothing_is_moved_until_a_whole_row_has_been_seen() {
        SPACING.set(0.0);
        assert_eq!(even_gap(0.0, 0.0), None, "a row of nothing was believed");
        assert_eq!(even_gap(-20.0, -20.0), None, "a backwards row was believed");
    }

    /// Moving a button is itself a frame change, so a pass is always reached
    /// from inside another one. Dropping those was what left the last button of
    /// the row wherever AppKit had just put it, with nothing left to notice.
    #[test]
    fn an_ask_that_arrives_mid_pass_is_answered_once_the_pass_is_over() {
        CORRECTING.set(false);
        AGAIN.set(false);

        let mut passes = 0;
        one_pass_at_a_time(|| {
            passes += 1;
            // What moving a button does: it asks for a pass from inside one.
            // Answering it there would be two passes fighting over one row.
            if passes == 1 {
                one_pass_at_a_time(|| unreachable!("a pass ran inside another one"));
            }
        });

        assert_eq!(passes, 2, "the ask that arrived mid-pass was dropped");
    }

    /// And it stops there. A pass that asks for another every time is AppKit and
    /// Coffer moving the row against each other, and spinning until they agree
    /// is a hang where the complaint was a flicker.
    #[test]
    fn a_row_that_never_settles_is_left_rather_than_spun_on() {
        CORRECTING.set(false);
        AGAIN.set(false);

        let mut passes = 0;
        one_pass_at_a_time(|| {
            passes += 1;
            assert!(passes < 10, "the second pass asked for a third, and got it");
            one_pass_at_a_time(|| unreachable!("a pass ran inside another one"));
        });

        assert_eq!(passes, 2, "a row that never settles was not left alone");
        assert!(!CORRECTING.get(), "the guard was left standing");
        assert!(!AGAIN.get(), "an ask was left for the next pass to answer");
    }
}
