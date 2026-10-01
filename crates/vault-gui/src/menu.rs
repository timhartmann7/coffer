//! The menu bar.
//!
//! Tauri's own, item for item, without the Services submenu. Services hands
//! whatever is selected in the frontmost window to another application - New
//! Sticky Note, a TextEdit window containing the selection - and a revealed
//! value is text that can be selected. WebKit serves a selection to a service
//! straight off the page, so nothing in the window sees it happen: no copy event
//! fires for `guard.ts` to take, and the value ends up kept in plain text by
//! whichever application the reader picked.
//!
//! Everything else stays, because the reader's typing depends on it. AppKit
//! finds Cut, Copy, Paste, Select All and Undo for a text field through the
//! Edit menu's key equivalents, so a bar without them leaves every field in the
//! window deaf to Cmd+C and Cmd+V.

use tauri::menu::{
    AboutMetadata, HELP_SUBMENU_ID, IsMenuItem, Menu, PredefinedMenuItem, Submenu,
    WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Runtime};

/// An item of the bar, by what it does. Each is one of AppKit's own actions,
/// with the title, key equivalent and enabling rules the system gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    About,
    Hide,
    HideOthers,
    ShowAll,
    Quit,
    CloseWindow,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Fullscreen,
    Minimize,
    Maximize,
    Separator,
}

/// What a submenu is called.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Title {
    /// The application's own name, which is the first submenu's title on a Mac.
    Application,
    Named(&'static str),
}

/// One submenu: its title, the id Tauri hands to AppKit for the two submenus
/// AppKit fills in itself, and its items.
type Column = (Title, Option<&'static str>, &'static [Item]);

/// The bar, left to right. Show All is the one item here that Tauri's own bar
/// leaves out, and it belongs beside Hide Others on every Mac.
const BAR: &[Column] = &[
    (
        Title::Application,
        None,
        &[
            Item::About,
            Item::Separator,
            Item::Hide,
            Item::HideOthers,
            Item::ShowAll,
            Item::Separator,
            Item::Quit,
        ],
    ),
    (Title::Named("File"), None, &[Item::CloseWindow]),
    (
        Title::Named("Edit"),
        None,
        &[
            Item::Undo,
            Item::Redo,
            Item::Separator,
            Item::Cut,
            Item::Copy,
            Item::Paste,
            Item::SelectAll,
        ],
    ),
    (Title::Named("View"), None, &[Item::Fullscreen]),
    // AppKit lists the open windows under this one.
    (
        Title::Named("Window"),
        Some(WINDOW_SUBMENU_ID),
        &[
            Item::Minimize,
            Item::Maximize,
            Item::Separator,
            Item::CloseWindow,
        ],
    ),
    // Empty, and still there: AppKit puts the search field for the menus in it.
    (Title::Named("Help"), Some(HELP_SUBMENU_ID), &[]),
];

/// Builds the bar for the application `app`.
pub fn bar<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let package = app.package_info();
    let config = app.config();
    let about = AboutMetadata {
        name: Some(package.name.clone()),
        version: Some(package.version.to_string()),
        copyright: config.bundle.copyright.clone(),
        authors: config
            .bundle
            .publisher
            .clone()
            .map(|publisher| vec![publisher]),
        ..Default::default()
    };

    let menu = Menu::new(app)?;
    for (title, id, items) in BAR {
        let title = match title {
            Title::Application => package.name.as_str(),
            Title::Named(title) => title,
        };
        let items = items
            .iter()
            .map(|item| predefined(app, *item, &about))
            .collect::<tauri::Result<Vec<_>>>()?;
        let items: Vec<&dyn IsMenuItem<R>> = items
            .iter()
            .map(|item| item as &dyn IsMenuItem<R>)
            .collect();
        let column = match id {
            Some(id) => Submenu::with_id_and_items(app, *id, title, true, &items)?,
            None => Submenu::with_items(app, title, true, &items)?,
        };
        menu.append(&column)?;
    }
    Ok(menu)
}

fn predefined<R: Runtime>(
    app: &AppHandle<R>,
    item: Item,
    about: &AboutMetadata<'_>,
) -> tauri::Result<PredefinedMenuItem<R>> {
    match item {
        Item::About => PredefinedMenuItem::about(app, None, Some(about.clone())),
        Item::Hide => PredefinedMenuItem::hide(app, None),
        Item::HideOthers => PredefinedMenuItem::hide_others(app, None),
        Item::ShowAll => PredefinedMenuItem::show_all(app, None),
        Item::Quit => PredefinedMenuItem::quit(app, None),
        Item::CloseWindow => PredefinedMenuItem::close_window(app, None),
        Item::Undo => PredefinedMenuItem::undo(app, None),
        Item::Redo => PredefinedMenuItem::redo(app, None),
        Item::Cut => PredefinedMenuItem::cut(app, None),
        Item::Copy => PredefinedMenuItem::copy(app, None),
        Item::Paste => PredefinedMenuItem::paste(app, None),
        Item::SelectAll => PredefinedMenuItem::select_all(app, None),
        Item::Fullscreen => PredefinedMenuItem::fullscreen(app, None),
        Item::Minimize => PredefinedMenuItem::minimize(app, None),
        Item::Maximize => PredefinedMenuItem::maximize(app, None),
        Item::Separator => PredefinedMenuItem::separator(app),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::shipped;

    /// A bar cannot be built in a test: AppKit makes menus on the main thread
    /// only, and the harness runs every test on a thread of its own. So this
    /// is read from the source. The two ways Services comes back are the
    /// predefined item and the default bar, which Tauri builds whenever the
    /// application is given no bar of its own.
    #[test]
    fn nothing_in_the_window_offers_a_selection_to_services() {
        let mut directories = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        let mut read = 0;
        while let Some(directory) = directories.pop() {
            for found in std::fs::read_dir(&directory).expect("the source is there") {
                let path = found.expect("the source is readable").path();
                if path.is_dir() {
                    directories.push(path);
                    continue;
                }
                let file = std::fs::read_to_string(&path).expect("the source is text");
                let source = shipped(&file);
                let name = path.display();
                assert!(
                    !source.contains("::services("),
                    "{name} builds a Services item"
                );
                assert!(
                    !source.contains("Menu::default("),
                    "{name} builds Tauri's bar"
                );
                read += 1;
            }
        }
        assert!(read > 10, "only {read} files of this crate were read");
        assert!(
            shipped(include_str!("lib.rs")).contains(".menu(menu::bar)"),
            "the application is built without a bar of its own, so Tauri's is drawn"
        );
    }

    /// Tauri's own bar for macOS (`tauri::menu::Menu::default` in 2.11.5), with
    /// the Services submenu and the separator under it taken out and Show All
    /// put in. Edit is the column that matters: a text field finds Cut, Copy,
    /// Paste, Select All and Undo through it.
    #[test]
    fn the_bar_keeps_every_other_item_of_tauris_own() {
        use Item::*;

        let edit = BAR
            .iter()
            .find(|(title, _, _)| *title == Title::Named("Edit"))
            .map(|(_, _, items)| *items);
        assert_eq!(
            edit,
            Some(&[Undo, Redo, Separator, Cut, Copy, Paste, SelectAll][..])
        );

        let titles: Vec<Title> = BAR.iter().map(|(title, _, _)| *title).collect();
        assert_eq!(
            titles,
            [
                Title::Application,
                Title::Named("File"),
                Title::Named("Edit"),
                Title::Named("View"),
                Title::Named("Window"),
                Title::Named("Help"),
            ]
        );

        let application = BAR.first().map(|(_, _, items)| *items);
        assert_eq!(
            application,
            Some(&[About, Separator, Hide, HideOthers, ShowAll, Separator, Quit][..])
        );
    }
}
