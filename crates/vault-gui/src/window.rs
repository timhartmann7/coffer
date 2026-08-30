//! Opening the window, and opening it again.
//!
//! Locking destroys the window rather than hiding it, so this runs on every
//! unlock as well as at startup. The two are the same path on purpose: a
//! rebuilt window that differed from the one Coffer starts with would differ in
//! a way nobody tested.

use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::utils::config::WindowConfig;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime, WindowEvent};

use crate::{buttons, settings};

/// The window's label, which is also its key in `tauri.conf.json`.
pub const MAIN: &str = "main";

/// Whether the window that is going is going in order to come back.
///
/// A lock destroys the window, and so does the reader pressing the close
/// button. The two arrive as the same event, so without this the close button
/// would build the window again and Coffer could never be shut.
static REBUILDING: AtomicBool = AtomicBool::new(false);

/// Says that the destroy about to be queued is a lock rather than a close.
pub fn rebuilding() {
    REBUILDING.store(true, Ordering::Release);
}

/// Whether the window that has just gone asked to come back. Answers once: a
/// second destroy is a close until another lock says otherwise.
pub fn wanted_again() -> bool {
    REBUILDING.swap(false, Ordering::AcqRel)
}

thread_local! {
    /// Where the reader last left the window.
    ///
    /// Recorded from the window's own events, which arrive on the main thread,
    /// rather than read off the window when it is about to be destroyed:
    /// reading geometry is a round trip to the thread that draws, and a lock
    /// that arrives while that thread is inside a file panel would wait for the
    /// reader to close it.
    static PLACED: Cell<Option<Frame>> = const { Cell::new(None) };
}

/// Where a window is, in the units the configuration file uses.
#[derive(Clone, Copy)]
struct Frame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Builds the window described in `tauri.conf.json`, where the reader last put
/// it.
///
/// From the configuration rather than by hand. A hand-written builder would
/// restate the title, the size, the theme and the title-bar style, which are
/// already written down once; and its URL would have to be chosen, where the
/// configuration's own default resolves to the dev server under `tauri dev` and
/// to the bundled files in a real build. Getting that wrong shows up as a blank
/// window in development and nowhere else.
pub fn open<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let Some(mut config) = described(app) else {
        return Err(tauri::Error::WebviewNotFound);
    };

    // A window that came back at the centre of the screen after every idle lock
    // would be Coffer tidying the reader's desk for them.
    if let Some(frame) = PLACED.get() {
        config.center = false;
        config.x = Some(frame.x);
        config.y = Some(frame.y);
        config.width = frame.width;
        config.height = frame.height;
    }

    // What the window is born wearing, rather than what it is told afterwards.
    // The title bar, the three buttons and the native file panel are drawn once,
    // and a window that opened dark and turned light would do it where the
    // reader is looking. It is also what `prefers-color-scheme` reports inside
    // the webview, which is the only answer the screen has until the settings
    // have crossed.
    config.theme = appearance(look(app));

    let window = tauri::WebviewWindowBuilder::from_config(app, &config)?.build()?;
    buttons::centre_buttons(&window);

    let held = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Moved(at) => place(&held, Some(*at), None),
        WindowEvent::Resized(size) => place(&held, None, Some(*size)),
        _ => {}
    });

    Ok(())
}

/// The look the reader chose, or the default on a Mac with nowhere to keep one.
fn look<R: Runtime>(app: &AppHandle<R>) -> settings::Theme {
    app.try_state::<Arc<settings::Preferences>>()
        .map(|held| held.get().theme)
        .unwrap_or_default()
}

/// What AppKit is told. Nothing means the Mac's own, which is what `System` is
/// asking for and what Tauri reads an absent theme as.
fn appearance(theme: settings::Theme) -> Option<tauri::Theme> {
    match theme {
        settings::Theme::System => None,
        settings::Theme::Dark => Some(tauri::Theme::Dark),
        settings::Theme::Light => Some(tauri::Theme::Light),
    }
}

/// Retunes the window that is already open.
///
/// A look is not worth a lock, and building the window again is a lock: it wipes
/// the tree and asks for the password back. macOS keeps the appearance on the
/// application rather than on one window, so this is the application's own
/// setter, which is also why it works when nothing is unlocked.
pub fn retune<R: Runtime>(app: &AppHandle<R>, theme: settings::Theme) {
    app.set_theme(appearance(theme));
}

/// The entry for the main window in the configuration file.
fn described<R: Runtime>(app: &AppHandle<R>) -> Option<WindowConfig> {
    app.config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN)
        .cloned()
}

/// Records where the window is now.
///
/// Both halves come from events rather than from the window, except the one
/// that did not change: a `Moved` says nothing about the size and a `Resized`
/// says nothing about the position, and asking the window for the other one is
/// a read on the thread this closure is already running on.
fn place<R: Runtime>(
    window: &tauri::WebviewWindow<R>,
    at: Option<PhysicalPosition<i32>>,
    size: Option<PhysicalSize<u32>>,
) {
    // Asked for now rather than remembered from when the window was built. A
    // window dragged to a display of another scale keeps reporting physical
    // pixels, and dividing those by the scale of the display it started on
    // stores a frame off by the ratio between the two.
    let scale = window.scale_factor().unwrap_or(1.0);

    let Some(at) = at.or_else(|| window.outer_position().ok()) else {
        return;
    };
    let Some(size) = size.or_else(|| window.inner_size().ok()) else {
        return;
    };

    // A minimised window reports nothing worth remembering, and putting it back
    // at that size would open a window nobody could see.
    if size.width == 0 || size.height == 0 {
        return;
    }

    PLACED.set(Some(Frame {
        x: f64::from(at.x) / scale,
        y: f64::from(at.y) / scale,
        width: f64::from(size.width) / scale,
        height: f64::from(size.height) / scale,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The inversion this would ship is a window forced dark for a reader who
    /// asked for the Mac's own, and it looks like nothing being wrong until
    /// somebody's Mac is light.
    #[test]
    fn only_the_macs_own_look_leaves_the_window_to_the_mac() {
        for theme in settings::THEME_CHOICES {
            assert_eq!(
                appearance(theme).is_none(),
                theme == settings::Theme::System,
                "{theme:?} was handed the wrong appearance"
            );
        }

        assert_eq!(
            appearance(settings::Theme::Dark),
            Some(tauri::Theme::Dark),
            "the dark look asked AppKit for something else"
        );
        assert_eq!(
            appearance(settings::Theme::Light),
            Some(tauri::Theme::Light),
            "the light look asked AppKit for something else"
        );
    }

    /// Locking and closing arrive as the same event, and only one of them may
    /// build the window again.
    #[test]
    fn only_a_lock_asks_for_the_window_back() {
        assert!(!wanted_again(), "nothing has asked yet");

        rebuilding();
        assert!(wanted_again(), "a lock asked for it back");
        assert!(
            !wanted_again(),
            "the answer stood a second time, so a close would reopen the window"
        );
    }
}
