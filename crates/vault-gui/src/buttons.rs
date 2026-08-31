//! Putting the window's own buttons where the title bar draws them.
//!
//! The frame is hidden and the title bar is drawn in HTML, so macOS keeps the
//! close, minimise and zoom buttons where a standard title bar would have put
//! them: near the top, in a band half the height of Coffer's. Tauri's
//! `trafficLightPosition` cannot fix it, because the inset it stores is applied
//! when the native view redraws and the webview covers that view for good.
//!
//! Moving the buttons afterwards cannot fix it either, and that is the whole
//! lesson of this module. Measured on macOS 26:
//!
//! - AppKit lays the row out again on every pass, and it writes the same two
//!   numbers every time: an offset from the left of the view the buttons are
//!   in, and an offset up from its bottom. It does not ask where the buttons
//!   are, so nothing that moves them is remembered.
//! - A drag lays the row out on every frame the display draws. A correction
//!   that answers a notification, an event or a timer is a correction that
//!   sometimes lands after the frame it belonged to, and each one of those is a
//!   frame where the row is drawn where AppKit put it. That is the flicker.
//!
//! So the row is not moved. The view it sits in is replaced instead: the three
//! buttons are taken into a view of Coffer's own, positioned so that AppKit's
//! own two numbers land in the middle of Coffer's title bar. AppKit goes on
//! placing the row exactly as it likes, as often as it likes, and every one of
//! those placements is already right. There is nothing to correct, so there is
//! no moment at which the correction can be late.
//!
//! While the window is dragged, the view follows the top of the window on its
//! autoresizing mask, which AppKit applies inside the same call that resizes the
//! window. Nothing about the row is read a second time, so nothing about it can
//! disagree with itself mid-drag.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSButton, NSView, NSWindow, NSWindowButton, NSWindowStyleMask,
};
use objc2_foundation::{
    NSArray, NSObjectProtocol, NSPoint, NSRect, NSRunLoop, NSRunLoopCommonModes, NSSize,
};
use tauri::{Runtime, WebviewWindow, WindowEvent};

/// The height of the title bar in `frontend/src/lib/components/Titlebar.svelte`,
/// which is `h-11` in the mockup. The buttons are centred in it.
const TITLE_BAR: f64 = 44.0;

/// How far the leftmost button sits from the left edge. The bar's own padding
/// is `px-4`, and the buttons line up with it.
const FROM_LEFT: f64 = 16.0;

define_class!(
    /// The view the three buttons are moved into.
    ///
    /// It draws nothing and holds nothing of its own. Its frame is the whole
    /// mechanism: AppKit writes the row's place in the coordinates of whichever
    /// view the buttons are in, so where that view is decides where the row is.
    ///
    /// SAFETY:
    /// - `NSView` has no subclassing requirement beyond the main thread, which
    ///   `MainThreadOnly` states.
    /// - The class holds no instance variables and implements no `Drop`, so
    ///   nothing of Rust's outlives the view AppKit owns.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "CofferWindowButtons"]
    struct Row;

    unsafe impl NSObjectProtocol for Row {}

    impl Row {
        /// Only the buttons answer a click; the gaps between them are the
        /// window's.
        ///
        /// A view of this size laid over the title bar would otherwise swallow
        /// everything that lands beside a button, and beside the buttons is
        /// where the reader drags the window.
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, at: NSPoint) -> Option<Retained<NSView>> {
            // The point arrives in the coordinates of the view this one sits
            // in, and a button wants it in this one's.
            let inside =
                unsafe { self.convertPoint_fromView(at, self.superview().as_deref()) };
            // In the order they were added, which is the order they were made:
            // three buttons in a row, none of them over another.
            self.subviews()
                .iter()
                .find_map(|button| button.hitTest(inside))
        }

        /// AppKit taking one of the buttons back.
        ///
        /// It does that for a full screen, and it does it in the setup that
        /// follows a window being built - which is how a lock used to end with
        /// the row where macOS wanted it. There is no notification for a view
        /// changing hands, and none is needed: the view it is leaving is asked
        /// first, and this is that view.
        ///
        /// Asked again rather than pulled back here: AppKit is in the middle of
        /// the move, and the answer is only worth anything once it has finished.
        #[unsafe(method(willRemoveSubview:))]
        fn will_remove(&self, button: &NSView) {
            let _: () = unsafe { msg_send![super(self), willRemoveSubview: button] };
            if let Some(window) = self.window() {
                later(&window);
            }
        }
    }
);

/// Centres the window buttons, and keeps them centred.
pub fn centre_buttons<R: Runtime>(window: &WebviewWindow<R>) {
    on_the_thread_that_draws(window, |native, mtm| {
        hold(native, mtm);
        // And once more when the window has finished being built. Coffer is
        // handed the window before AppKit has done with it, and the setup that
        // follows hands the row back to the title bar it came from.
        later(native);
    });

    let handle = window.clone();
    window.on_window_event(move |event| match event {
        // Three things that mean AppKit has had the window in its hands.
        // Returning from a full screen leaves the row in a title bar of
        // AppKit's own and tao reports it as a resize; a display of another
        // scale and a window coming forward are the two others, and the second
        // of them is when a reader would see a row that had gone astray.
        //
        // Answering all three costs nothing: a check that finds the row in
        // place does not touch it, which is what makes this safe to answer
        // while a window is being dragged.
        WindowEvent::Resized(_)
        | WindowEvent::ScaleFactorChanged { .. }
        | WindowEvent::Focused(_) => on_the_thread_that_draws(&handle, hold),
        _ => {}
    });
}

/// Runs the work against the window AppKit knows, on the thread AppKit allows.
fn on_the_thread_that_draws<R, W>(window: &WebviewWindow<R>, work: W)
where
    R: Runtime,
    W: FnOnce(&NSWindow, MainThreadMarker) + Send + 'static,
{
    let Some(native) = native(window) else {
        return;
    };

    if let Some(mtm) = MainThreadMarker::new() {
        work(&native, mtm);
        return;
    }

    let native = SendWindow(native);
    let _ = window.run_on_main_thread(move || {
        // The whole carrier is moved across, not the reference inside it: that
        // reference is the half that is not `Send`.
        let carried = &native;
        if let Some(mtm) = MainThreadMarker::new() {
            work(&carried.0, mtm);
        }
    });
}

/// Asks for the row again on the next turn of the run loop.
///
/// Once, and from the two places that know the answer is about to change: a
/// window that has just been built, and a row AppKit has just taken. Neither
/// asks again from inside the answer, so nothing here can spin.
fn later(window: &NSWindow) {
    let carried = window.retain();
    let block = RcBlock::new(move || {
        if let Some(mtm) = MainThreadMarker::new() {
            hold(&carried, mtm);
        }
    });

    // The common modes, so that a row taken while the window is being dragged
    // comes back during the drag rather than after it.
    //
    // SAFETY: the block only touches AppKit, and the main run loop only runs it
    // on the main thread.
    unsafe {
        NSRunLoop::mainRunLoop()
            .performInModes_block(&NSArray::from_slice(&[NSRunLoopCommonModes]), &block);
    }
}

fn native<R: Runtime>(window: &WebviewWindow<R>) -> Option<Retained<NSWindow>> {
    let handle = window.ns_window().ok()?;

    // SAFETY: `ns_window` hands back the `NSWindow` this webview lives in, and
    // AppKit owns it for as long as the window is open.
    unsafe { Retained::retain(handle.cast::<NSWindow>()) }
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

/// Moves the row into Coffer's view, and does nothing at all if it is already
/// there.
///
/// Doing nothing is the common case by far: this is reached from every resize,
/// and the row leaves only for full screen.
fn hold(window: &NSWindow, mtm: MainThreadMarker) {
    // A full screen hangs the title bar in a window of AppKit's own, where the
    // row is revealed by a mouse at the top edge rather than drawn by anybody's
    // title bar. Coffer's title bar is not up there to centre anything in.
    if window.styleMask().contains(NSWindowStyleMask::FullScreen) {
        return;
    }

    let Some(row) = row(window) else {
        return;
    };
    let Some(content) = window.contentView() else {
        return;
    };
    // The view the whole window is drawn in, which is the one the title bar and
    // the webview are both inside. SAFETY: reading the view hierarchy of a live
    // window on the main thread.
    let Some(host) = (unsafe { content.superview() }) else {
        return;
    };

    let holder = match ours(&host) {
        Some(holder) if row.iter().all(|button| sits_in(button, &holder)) => return,
        Some(holder) => holder,
        None => make(&host, mtm),
    };

    let [close, _, zoom] = &row;
    holder.setFrame(place(close.frame(), zoom.frame(), host.frame().size));
    for button in &row {
        // The row leaves whichever view it was in: a view has one superview,
        // and this is how it is moved between two of them.
        holder.addSubview(button);
    }
}

/// The three buttons, or nothing if this window is missing one of them.
///
/// All three or none: the row is laid out as a row, and a holder sized from a
/// row with a hole in it is a holder sized from the wrong button.
fn row(window: &NSWindow) -> Option<[Retained<NSView>; 3]> {
    let of = |kind| {
        window
            .standardWindowButton(kind)
            .map(|button: Retained<NSButton>| Retained::into_super(Retained::into_super(button)))
    };

    Some([
        of(NSWindowButton::CloseButton)?,
        of(NSWindowButton::MiniaturizeButton)?,
        of(NSWindowButton::ZoomButton)?,
    ])
}

/// Coffer's view, if this window has been given one already.
fn ours(host: &NSView) -> Option<Retained<NSView>> {
    let ours = <Row as ClassType>::class();
    host.subviews().iter().find(|view| view.isKindOfClass(ours))
}

fn make(host: &NSView, mtm: MainThreadMarker) -> Retained<NSView> {
    let holder = Row::alloc(mtm).set_ivars(());
    let holder: Retained<Row> = unsafe { msg_send![super(holder), init] };

    // The one thing that keeps the row in place while the window is dragged.
    // A flexible bottom margin is a view that keeps its height and its distance
    // from the top, and AppKit applies it inside the call that resizes the
    // window rather than in answer to it.
    holder.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);

    // Last in the list is above the webview, which is where the buttons have
    // to be drawn.
    host.addSubview(&holder);
    Retained::into_super(holder)
}

fn sits_in(button: &NSView, holder: &NSView) -> bool {
    // SAFETY: reading the view hierarchy on the main thread.
    unsafe { button.superview() }
        .is_some_and(|at| std::ptr::eq(&*at as *const NSView, holder as *const NSView))
}

/// Where the holder goes, so that AppKit's own placement inside it is the
/// placement Coffer's title bar wants.
///
/// AppKit writes the row at a fixed offset from the left of the holder and a
/// fixed offset up from its bottom, and it writes those two numbers whatever
/// the holder is doing. So the holder's left edge carries the row to
/// [`FROM_LEFT`], and the holder's height - measured from the top of the
/// window, where its own autoresizing keeps it - carries the row to the middle
/// of [`TITLE_BAR`].
///
/// The holder is never shorter than the row is tall. A button that AppKit made
/// taller than Coffer's title bar would otherwise be carried up past the top of
/// the window, where half of it cannot be drawn or clicked.
fn place(first: NSRect, last: NSRect, host: NSSize) -> NSRect {
    let height = (TITLE_BAR / 2.0 + first.origin.y + first.size.height / 2.0)
        .max(first.origin.y + first.size.height);

    NSRect::new(
        NSPoint::new(FROM_LEFT - first.origin.x, host.height - height),
        NSSize::new((last.origin.x + last.size.width).max(0.0), height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What AppKit does with the row once it is in the holder: it writes the
    /// same offsets it always writes, in the holder's own coordinates.
    fn as_appkit_would(holder: NSRect, button: NSRect, host: NSSize) -> (f64, f64) {
        let left = holder.origin.x + button.origin.x;
        let centre = host.height - (holder.origin.y + button.origin.y + button.size.height / 2.0);
        (left, centre)
    }

    /// The whole of the fix: whatever AppKit chooses, the choice lands in the
    /// middle of Coffer's title bar. Nothing corrects it afterwards, so this is
    /// the only thing that can put the row in the right place.
    #[test]
    fn appkits_own_placement_lands_in_the_middle_of_the_title_bar() {
        // Measured on macOS 26, and the same window with a toolbar, which puts
        // the row somewhere else entirely.
        for button in [(9.0, 9.0, 14.0), (12.0, 13.0, 14.0), (19.0, 33.0, 14.0)] {
            let first = NSRect::new(
                NSPoint::new(button.0, button.1),
                NSSize::new(button.2, button.2),
            );
            let last = NSRect::new(
                NSPoint::new(button.0 + 46.0, button.1),
                NSSize::new(button.2, button.2),
            );

            for height in [560.0, 720.0, 1329.5, 2160.0] {
                let host = NSSize::new(1080.0, height);
                let (left, centre) = as_appkit_would(place(first, last, host), first, host);

                assert!(
                    (left - FROM_LEFT).abs() < f64::EPSILON,
                    "the row was left at {left} rather than {FROM_LEFT}"
                );
                assert!(
                    (centre - TITLE_BAR / 2.0).abs() < f64::EPSILON,
                    "the row's middle was {centre} below the top of a {TITLE_BAR} bar"
                );
            }
        }
    }

    /// The holder is anchored to the top of the window, which is what its
    /// autoresizing mask keeps it at while the window is dragged. A holder
    /// placed any other way would need putting back, and putting it back is the
    /// thing that used to arrive a frame late.
    #[test]
    fn the_holder_reaches_the_top_of_the_window_at_every_height() {
        let first = NSRect::new(NSPoint::new(9.0, 9.0), NSSize::new(14.0, 14.0));
        let last = NSRect::new(NSPoint::new(55.0, 9.0), NSSize::new(14.0, 14.0));

        for height in [560.0, 561.0, 720.0, 2160.0] {
            let holder = place(first, last, NSSize::new(1080.0, height));
            assert!(
                (holder.origin.y + holder.size.height - height).abs() < f64::EPSILON,
                "the holder's top was {} below the window's at height {height}",
                height - holder.origin.y - holder.size.height
            );
        }
    }

    /// The holder has to reach the last button, or a click on the zoom button
    /// lands on the window behind it.
    #[test]
    fn the_holder_reaches_the_far_side_of_the_last_button() {
        let first = NSRect::new(NSPoint::new(9.0, 9.0), NSSize::new(14.0, 14.0));
        let last = NSRect::new(NSPoint::new(55.0, 9.0), NSSize::new(14.0, 14.0));
        let holder = place(first, last, NSSize::new(1080.0, 720.0));

        assert!(
            holder.size.width >= last.origin.x + last.size.width,
            "the holder stops at {} and the row ends at {}",
            holder.size.width,
            last.origin.x + last.size.width
        );
    }

    /// A button AppKit made taller than Coffer's title bar would be carried off
    /// the top of the window by a holder sized to centre it, and the half of it
    /// that is outside cannot be drawn or clicked.
    #[test]
    fn a_button_taller_than_the_title_bar_stays_inside_the_window() {
        let tall = NSSize::new(14.0, TITLE_BAR + 20.0);
        let first = NSRect::new(NSPoint::new(9.0, 9.0), tall);
        let host = NSSize::new(1080.0, 720.0);
        let holder = place(first, first, host);

        let top = holder.origin.y + first.origin.y + first.size.height;
        assert!(
            top <= host.height,
            "the row reached {top} of a window {} tall",
            host.height
        );
    }

    /// A row AppKit has not laid out yet answers with nothing rather than with
    /// a rectangle AppKit will not accept.
    #[test]
    fn a_row_of_nothing_does_not_make_a_backwards_holder() {
        let nothing = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
        let holder = place(nothing, nothing, NSSize::new(1080.0, 720.0));

        assert!(holder.size.width >= 0.0, "the holder was {holder:?}");
        assert!(holder.size.height >= 0.0, "the holder was {holder:?}");
    }
}
