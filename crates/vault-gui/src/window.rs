//! Putting the window's own buttons where the title bar draws them.
//!
//! The frame is hidden and the title bar is drawn in HTML, so macOS keeps the
//! close, minimise and zoom buttons where a standard title bar would have put
//! them: near the top, in a band half the height of Coffer's. Tauri's
//! `trafficLightPosition` cannot fix it, because the inset it stores is applied
//! when the native view redraws and the webview covers that view for good.
//!
//! So the three buttons are moved here instead, on the same arithmetic, at
//! every moment the system may have put them back.

use objc2::rc::Retained;
use objc2_app_kit::{NSWindow, NSWindowButton};
use objc2_foundation::NSPoint;
use tauri::{Runtime, WebviewWindow, WindowEvent};

/// The height of the title bar in `frontend/src/lib/components/Titlebar.svelte`,
/// which is `h-11` in the mockup. The buttons are centred in it.
const TITLE_BAR: f64 = 44.0;

/// How far the leftmost button sits from the left edge. The bar's own padding
/// is `px-4`, and the buttons line up with it.
const FROM_LEFT: f64 = 16.0;

/// Centres the window buttons, and keeps them centred.
///
/// Every path that gives the window a new size hands the buttons back to
/// AppKit, which puts them where a standard title bar would want them, so the
/// same arithmetic runs again on every resize.
pub fn centre_buttons<R: Runtime>(window: &WebviewWindow<R>) {
    balance(window);

    let handle = window.clone();
    window.on_window_event(move |event| {
        if matches!(
            event,
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }
        ) {
            balance(&handle);
        }
    });
}

fn balance<R: Runtime>(window: &WebviewWindow<R>) {
    let Ok(handle) = window.ns_window() else {
        return;
    };

    // SAFETY: `ns_window` hands back the `NSWindow` this webview lives in, and
    // AppKit owns it for as long as the window is open. The pointer is only
    // borrowed for the length of this call.
    let native: Option<Retained<NSWindow>> = unsafe { Retained::retain(handle.cast::<NSWindow>()) };
    let Some(native) = native else {
        return;
    };

    // Everything below has to happen on the thread that draws.
    let native = SendWindow(native);
    let _ = window.run_on_main_thread(move || unsafe { lay_out(&native) });
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

unsafe fn lay_out(window: &SendWindow) {
    // SAFETY: the caller is the main thread, and the window is held by the
    // value that carried it here.
    let window = &*window.0;

    let (Some(close), Some(miniaturise), Some(zoom)) = (
        window.standardWindowButton(NSWindowButton::CloseButton),
        window.standardWindowButton(NSWindowButton::MiniaturizeButton),
        window.standardWindowButton(NSWindowButton::ZoomButton),
    ) else {
        return;
    };

    // The buttons sit in the view the system keeps for the title bar it is not
    // drawing, which is anchored to the top of the window and stays the height
    // a standard title bar would be. Resizing it does not hold: AppKit puts it
    // back at the next layout, and the buttons stay where they were put. So the
    // buttons are placed inside it, and nothing else is touched.
    // SAFETY: reading the view hierarchy of a live window on the main thread.
    let Some(inside) = (unsafe { close.superview() }) else {
        return;
    };

    // AppKit counts from the bottom, and the bottom of that view is where a
    // standard title bar would end rather than where Coffer's does.
    let height = close.frame().size.height;
    let above = ((TITLE_BAR - height) / 2.0).max(0.0);
    let bottom = (inside.frame().size.height - above - height).max(0.0);
    let spacing = miniaturise.frame().origin.x - close.frame().origin.x;

    for (index, each) in [close, miniaturise, zoom].into_iter().enumerate() {
        each.setFrameOrigin(NSPoint::new(FROM_LEFT + index as f64 * spacing, bottom));
    }
}
