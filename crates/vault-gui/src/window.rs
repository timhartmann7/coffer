//! Opening the window, and opening it again.
//!
//! Locking destroys the window rather than hiding it, so this runs on every
//! unlock as well as at startup. The two are the same path on purpose: a
//! rebuilt window that differed from the one Coffer starts with would differ in
//! a way nobody tested.

use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tauri::utils::config::WindowConfig;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime, WindowEvent};

use crate::{buttons, settings};

/// What every one of Coffer's window labels begins with, and the key of the one
/// in `tauri.conf.json`.
pub const MAIN: &str = "main";

/// How many times the window has been built again.
///
/// The label carries it, because a label cannot be used twice. Building a window
/// with the label of one that was just destroyed leaves a webview that is made,
/// registered, sized and visible, and that never loads its page: no script runs
/// in it and no command ever arrives. On screen it is an empty window the reader
/// cannot get back into, and nothing anywhere reports an error. Counting up
/// costs a string and sidesteps it entirely.
///
/// `capabilities/main.json` matches `main*` for the same reason.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// The label the window carries now.
pub fn label() -> String {
    match GENERATION.load(Ordering::Acquire) {
        0 => MAIN.to_owned(),
        generation => format!("{MAIN}-{generation}"),
    }
}

/// Moves on to the next label, so that the window about to be built does not
/// take the name of the one that just went.
fn next_generation() {
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

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
/// restate the title, the size and the title-bar style, which are already
/// written down once; and its URL would have to be chosen, where the
/// configuration's own default resolves to the dev server under `tauri dev` and
/// to the bundled files in a real build. Getting that wrong shows up as a blank
/// window in development and nowhere else.
///
/// Two things the configuration cannot say are filled in below, because both
/// belong to the reader rather than to the build: where the window was last
/// left, and which look it wears.
///
/// Private, so that nothing outside builds a window without choosing between
/// [`first`] and [`again`]: a build under the label of one that went is an
/// empty window for good (see `GENERATION`).
fn open<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
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
    config.label = label();

    // Shown when the page has drawn, rather than as the window is built. A
    // webview is white until its first frame, whatever the window around it is
    // wearing and whatever the document says about its colour scheme, and a
    // lock shows that as a flash between the vault and the password screen.
    // There is nothing to look at in the meantime: the window is a rectangle of
    // the reader's desktop until the screen it carries exists.
    config.visible = false;

    let window = tauri::WebviewWindowBuilder::from_config(app, &config)?
        .on_page_load(|window, payload| {
            if payload.event() != tauri::webview::PageLoadEvent::Finished {
                return;
            }
            let _ = window.show();
            // Asked for as well as shown: the window that was destroyed took
            // the application's focus with it, and the password field is what
            // the next keystroke is for.
            let _ = window.set_focus();
        })
        .build()?;
    buttons::centre_buttons(&window);

    let held = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Moved(at) => place(&held, Some(*at), None),
        WindowEvent::Resized(size) => place(&held, None, Some(*size)),
        _ => {}
    });

    Ok(())
}

/// The window Coffer starts with, under the first label. Built once, when the
/// application is set up; every window after it is built by [`again`].
pub fn first<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    open(app)
}

/// Builds the window under a label no window has carried (see `GENERATION`).
/// Every build after the first goes through here: after a lock, and after the
/// reader closed the window and asked for it back.
pub fn again<R: Runtime>(app: &AppHandle<R>) {
    next_generation();
    said(open(app));
}

/// The Dock icon was clicked, or something was chosen in the menu bar.
///
/// A window that is there is brought forward, and out of the Dock when it was
/// minimised, which AppKit does not do here on its own: tao answers a click on
/// the Dock icon with "do nothing" whenever no window is visible. One still
/// loading is left to show itself when its page has drawn, rather than shown
/// white. A window the reader closed is built again, where it was left.
pub fn bring_back<R: Runtime>(app: &AppHandle<R>) {
    match app.get_webview_window(&label()) {
        Some(window) => said(window.unminimize().and_then(|()| window.set_focus())),
        None => again(app),
    }
}

/// A window that would not come back is a line on standard error, written
/// here once for every way it is asked back. Nothing has been unlocked when
/// it is asked for - a lock wiped the tree before its window went, and a
/// close locked - so there is nothing in the error that could name a secret.
fn said(built: tauri::Result<()>) {
    if let Err(error) = built {
        eprintln!("Coffer could not open its window again: {error}");
    }
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

    /// A window may not take the label of one that was just destroyed: the
    /// replacement is made, registered and shown, and never loads its page.
    /// There is no error anywhere; the reader locks the vault and gets an empty
    /// window back. So every label after the first one is a new one.
    #[test]
    fn a_window_never_takes_the_label_of_the_one_before_it() {
        let mut seen = vec![label()];
        for _ in 0..4 {
            next_generation();
            let now = label();
            assert!(
                !seen.contains(&now),
                "the window was built again as {now}, which has been used"
            );
            assert!(
                now.starts_with(MAIN),
                "{now} does not begin with the label the configuration file uses"
            );
            seen.push(now);
        }
    }

    /// A window built under the label of one just destroyed is empty for good,
    /// so every build after the first takes a new label first. `open` is
    /// private, so the compiler holds every other file to `first` and `again`;
    /// this holds `first` to the one build at startup, in every file of the
    /// crate, and this file - which the compiler does not hold to anything -
    /// to building only through those two. A Dock click after Cmd+W moves no
    /// generation on by itself (a close is not a lock), so a `bring_back` that
    /// built through `open` or `first` would build under the closed window's
    /// label. Read from the source, because building a window needs the main
    /// thread a test does not have.
    #[test]
    fn every_window_after_the_first_is_built_under_a_new_label() {
        use crate::source::{every_file, functions, shipped};

        /// A function without its comments, which name calls they do not make.
        fn code(body: &str) -> String {
            body.lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n")
        }

        let here: Vec<String> = functions(shipped(include_str!("window.rs")))
            .iter()
            .map(|body| code(body))
            .collect();
        let building: Vec<&String> = here.iter().filter(|body| body.contains("open(")).collect();
        assert_eq!(building.len(), 2, "{building:#?}");
        assert!(
            building
                .iter()
                .all(|body| body.contains("pub fn first<") || body.contains("pub fn again<")),
            "a window is built in this file other than through `first` and `again`: {building:#?}"
        );
        assert!(
            here.iter().all(|body| !body.contains("first(")),
            "the first window's build is called again in this file"
        );
        let back = here
            .iter()
            .find(|body| body.contains("pub fn bring_back<"))
            .expect("there is a way to bring the window back");
        assert!(
            back.contains("again(") && !back.contains("open("),
            "a closed window is brought back under the label it went with:\n{back}"
        );

        let mut first = Vec::new();
        for (path, file) in every_file() {
            let calls = shipped(&file).matches("window::first(").count();
            first.extend(std::iter::repeat_n(path.display().to_string(), calls));
        }
        assert_eq!(
            first.len(),
            1,
            "the first window is built more than once, without a new label: {first:?}"
        );
        assert!(
            first.iter().all(|path| path.ends_with("lib.rs")),
            "the first window is built somewhere other than at startup: {first:?}"
        );
        assert!(
            shipped(include_str!("lib.rs")).contains("window::again("),
            "a lock no longer builds the window again"
        );

        let source = shipped(include_str!("window.rs"));
        let again = functions(source)
            .into_iter()
            .find(|body| body.contains("pub fn again<"))
            .expect("there is a way to build the window again");
        let renamed = again
            .find("next_generation()")
            .expect("it takes a new label");
        let built = again.find("open(app)").expect("it builds the window");
        assert!(
            renamed < built,
            "the window is built before its label moves on"
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
