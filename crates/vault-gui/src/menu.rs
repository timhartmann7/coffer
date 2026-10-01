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
//!
//! Coffer's own items sit among AppKit's. Each is a press the page answers
//! with the function its on-screen button runs (see `route.rs`); here is only
//! what each is called, the key that chooses it, and whether it can be chosen
//! now.

use serde::{Deserialize, Serialize};
use tauri::menu::{
    AboutMetadata, HELP_SUBMENU_ID, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu,
    WINDOW_SUBMENU_ID,
};
use tauri::{AppHandle, Manager, Runtime};

/// An item of Coffer's own in the bar. The word is the item's id in the bar
/// and its name in every message to and from the page, so a choice and the
/// page's answer to it can never be about two different items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    Settings,
    Lock,
    NewEntry,
    NewFolder,
    OpenVault,
    Find,
    CopyLogin,
    CopyPassword,
    MoveToBin,
    Shortcuts,
}

impl Command {
    /// Every item, in the order the bar draws them.
    pub const ALL: [Command; 10] = [
        Command::Settings,
        Command::Lock,
        Command::NewEntry,
        Command::NewFolder,
        Command::OpenVault,
        Command::Find,
        Command::CopyLogin,
        Command::CopyPassword,
        Command::MoveToBin,
        Command::Shortcuts,
    ];

    /// The id the item is built with, which is the word the page knows it by.
    fn id(self) -> &'static str {
        match self {
            Command::Settings => "settings",
            Command::Lock => "lock",
            Command::NewEntry => "newEntry",
            Command::NewFolder => "newFolder",
            Command::OpenVault => "openVault",
            Command::Find => "find",
            Command::CopyLogin => "copyLogin",
            Command::CopyPassword => "copyPassword",
            Command::MoveToBin => "moveToBin",
            Command::Shortcuts => "shortcuts",
        }
    }

    /// The item an id names, or nothing for one of AppKit's own.
    pub fn named(id: &str) -> Option<Command> {
        Command::ALL.into_iter().find(|command| command.id() == id)
    }

    fn title(self) -> &'static str {
        match self {
            Command::Settings => "Settings…",
            Command::Lock => "Lock Vault",
            Command::NewEntry => "New Entry",
            Command::NewFolder => "New Folder",
            Command::OpenVault => "Open Vault…",
            Command::Find => "Find…",
            Command::CopyLogin => "Copy Login",
            Command::CopyPassword => "Copy Password",
            Command::MoveToBin => "Move to Recycle Bin",
            Command::Shortcuts => "Keyboard Shortcuts",
        }
    }

    /// The key that chooses the item, in the words muda reads (`parse_modifier`
    /// and `parse_code` in muda 0.19.3's `accelerator.rs`). A string it cannot
    /// read becomes no key at all, and says nothing about it: Tauri parses it
    /// with `.ok()` (`menu/normal.rs` in 2.11.5). `shortcuts.test.ts` reads
    /// these lines to draw the same keys in the window.
    ///
    /// Copy Password is on Shift+Cmd+C because Cmd+C is Edit ▸ Copy, which is
    /// how every text field in the window copies; the page answers Cmd+C itself
    /// before AppKit looks here, so the mockup's Cmd+C still copies a password.
    fn accelerator(self) -> Option<&'static str> {
        match self {
            Command::Settings => Some("CmdOrCtrl+,"),
            Command::Lock => Some("CmdOrCtrl+L"),
            Command::NewEntry => Some("CmdOrCtrl+N"),
            Command::NewFolder => Some("Shift+CmdOrCtrl+N"),
            Command::OpenVault => Some("CmdOrCtrl+O"),
            Command::Find => Some("CmdOrCtrl+F"),
            Command::CopyLogin => Some("CmdOrCtrl+B"),
            Command::CopyPassword => Some("Shift+CmdOrCtrl+C"),
            Command::MoveToBin => Some("CmdOrCtrl+Backspace"),
            Command::Shortcuts => None,
        }
    }

    /// Whether the session lets the item be chosen at all, whatever the page
    /// says: there is nothing to lock with no vault open, and Rust refuses to
    /// point the session at another file while one is.
    pub fn allowed(self, unlocked: bool) -> bool {
        match self {
            Command::Lock => unlocked,
            Command::OpenVault => !unlocked,
            _ => true,
        }
    }
}

/// One of AppKit's own actions, with the title, key equivalent and enabling
/// rules the system gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Native {
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
}

/// An item of the bar, by what it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Native(Native),
    Coffer(Command),
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

/// The bar, left to right. Show All is the one item of AppKit's here that
/// Tauri's own bar leaves out, and it belongs beside Hide Others on every Mac.
const BAR: &[Column] = &[
    (
        Title::Application,
        None,
        &[
            Item::Native(Native::About),
            Item::Separator,
            Item::Coffer(Command::Settings),
            Item::Separator,
            Item::Coffer(Command::Lock),
            Item::Separator,
            Item::Native(Native::Hide),
            Item::Native(Native::HideOthers),
            Item::Native(Native::ShowAll),
            Item::Separator,
            Item::Native(Native::Quit),
        ],
    ),
    (
        Title::Named("File"),
        None,
        &[
            Item::Coffer(Command::NewEntry),
            Item::Coffer(Command::NewFolder),
            Item::Separator,
            Item::Coffer(Command::OpenVault),
            Item::Separator,
            Item::Native(Native::CloseWindow),
        ],
    ),
    (
        Title::Named("Edit"),
        None,
        &[
            Item::Native(Native::Undo),
            Item::Native(Native::Redo),
            Item::Separator,
            Item::Native(Native::Cut),
            Item::Native(Native::Copy),
            Item::Native(Native::Paste),
            Item::Native(Native::SelectAll),
            Item::Separator,
            Item::Coffer(Command::Find),
            Item::Coffer(Command::CopyLogin),
            Item::Coffer(Command::CopyPassword),
            Item::Separator,
            Item::Coffer(Command::MoveToBin),
        ],
    ),
    (
        Title::Named("View"),
        None,
        &[Item::Native(Native::Fullscreen)],
    ),
    // AppKit lists the open windows under this one.
    (
        Title::Named("Window"),
        Some(WINDOW_SUBMENU_ID),
        &[
            Item::Native(Native::Minimize),
            Item::Native(Native::Maximize),
            Item::Separator,
            Item::Native(Native::CloseWindow),
        ],
    ),
    // AppKit puts the search field for the menus at the top of this one.
    (
        Title::Named("Help"),
        Some(HELP_SUBMENU_ID),
        &[Item::Coffer(Command::Shortcuts)],
    ),
];

/// Coffer's items in the bar, kept so that they can be greyed out. Nothing
/// else holds them: the bar is built once, before `setup`, and Tauri's
/// `Menu::get` looks one level deep.
struct Bar<R: Runtime> {
    items: Vec<(Command, MenuItem<R>)>,
}

/// Whether an item can be chosen. With no page listening (`None`) only the two
/// that build the window again and then hand the choice to the page that
/// arrives: the reader closed the window and went to the menu bar for it.
fn enabled(command: Command, said: Option<&[Command]>) -> bool {
    match said {
        None => matches!(command, Command::OpenVault | Command::Settings),
        Some(said) => said.contains(&command),
    }
}

/// Greys out every item of Coffer's own the page did not say applies, or, with
/// no page, all but the two that bring one back.
///
/// On the main thread. Each change is AppKit's, and Tauri makes one from any
/// other thread a round trip that waits for this one (`run_item_main_thread`
/// in 2.11.5's `menu/mod.rs`).
pub fn enable<R: Runtime>(app: &AppHandle<R>, said: Option<&[Command]>) {
    let Some(bar) = app.try_state::<Bar<R>>() else {
        return;
    };
    for (command, item) in &bar.items {
        let _ = item.set_enabled(enabled(*command, said));
    }
}

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
    let mut coffer = Vec::new();
    for (title, id, items) in BAR {
        let title = match title {
            Title::Application => package.name.as_str(),
            Title::Named(title) => title,
        };
        let mut built: Vec<Box<dyn IsMenuItem<R>>> = Vec::new();
        for item in *items {
            built.push(match *item {
                Item::Native(native) => Box::new(predefined(app, native, &about)?),
                Item::Separator => Box::new(PredefinedMenuItem::separator(app)?),
                Item::Coffer(command) => {
                    let made = MenuItem::with_id(
                        app,
                        command.id(),
                        command.title(),
                        enabled(command, None),
                        command.accelerator(),
                    )?;
                    coffer.push((command, made.clone()));
                    Box::new(made)
                }
            });
        }
        let items: Vec<&dyn IsMenuItem<R>> = built.iter().map(|item| item.as_ref()).collect();
        let column = match id {
            Some(id) => Submenu::with_id_and_items(app, *id, title, true, &items)?,
            None => Submenu::with_items(app, title, true, &items)?,
        };
        menu.append(&column)?;
    }

    // Managed from here, which runs inside `build` on the main thread with a
    // working handle and before `setup` (`app.rs` in 2.11.5): nothing can ask
    // for these before they are there.
    app.manage(Bar { items: coffer });
    Ok(menu)
}

fn predefined<R: Runtime>(
    app: &AppHandle<R>,
    item: Native,
    about: &AboutMetadata<'_>,
) -> tauri::Result<PredefinedMenuItem<R>> {
    match item {
        Native::About => PredefinedMenuItem::about(app, None, Some(about.clone())),
        Native::Hide => PredefinedMenuItem::hide(app, None),
        Native::HideOthers => PredefinedMenuItem::hide_others(app, None),
        Native::ShowAll => PredefinedMenuItem::show_all(app, None),
        Native::Quit => PredefinedMenuItem::quit(app, None),
        Native::CloseWindow => PredefinedMenuItem::close_window(app, None),
        Native::Undo => PredefinedMenuItem::undo(app, None),
        Native::Redo => PredefinedMenuItem::redo(app, None),
        Native::Cut => PredefinedMenuItem::cut(app, None),
        Native::Copy => PredefinedMenuItem::copy(app, None),
        Native::Paste => PredefinedMenuItem::paste(app, None),
        Native::SelectAll => PredefinedMenuItem::select_all(app, None),
        Native::Fullscreen => PredefinedMenuItem::fullscreen(app, None),
        Native::Minimize => PredefinedMenuItem::minimize(app, None),
        Native::Maximize => PredefinedMenuItem::maximize(app, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{every_file, shipped};

    /// A bar cannot be built in a test: AppKit makes menus on the main thread
    /// only, and the harness runs every test on a thread of its own. So this
    /// is read from the source. The two ways Services comes back are the
    /// predefined item and the default bar, which Tauri builds whenever the
    /// application is given no bar of its own.
    #[test]
    fn nothing_in_the_window_offers_a_selection_to_services() {
        for (path, file) in every_file() {
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
        }
        assert!(
            shipped(include_str!("lib.rs")).contains(".menu(menu::bar)"),
            "the application is built without a bar of its own, so Tauri's is drawn"
        );
    }

    fn natives(items: &[Item]) -> Vec<Native> {
        items
            .iter()
            .filter_map(|item| match item {
                Item::Native(native) => Some(*native),
                _ => None,
            })
            .collect()
    }

    /// Tauri's own bar for macOS (`tauri::menu::Menu::default` in 2.11.5), with
    /// the Services submenu and the separator under it taken out and Show All
    /// put in. Edit is the column that matters: a text field finds Cut, Copy,
    /// Paste, Select All and Undo through it, and an item of Coffer's put in
    /// its place would take the key from every field in the window.
    #[test]
    fn the_bar_keeps_every_item_of_tauris_own_in_order() {
        use Native::*;

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

        let columns: Vec<Vec<Native>> = BAR.iter().map(|(_, _, items)| natives(items)).collect();
        assert_eq!(
            columns,
            [
                vec![About, Hide, HideOthers, ShowAll, Quit],
                vec![CloseWindow],
                vec![Undo, Redo, Cut, Copy, Paste, SelectAll],
                vec![Fullscreen],
                vec![Minimize, Maximize, CloseWindow],
                vec![],
            ]
        );

        let ids: Vec<Option<&str>> = BAR.iter().map(|(_, id, _)| *id).collect();
        assert_eq!(
            ids,
            [
                None,
                None,
                None,
                None,
                Some(WINDOW_SUBMENU_ID),
                Some(HELP_SUBMENU_ID)
            ],
            "AppKit no longer lists the windows, or no longer puts its search in Help"
        );
    }

    /// AppKit draws every separator it is given, so a column that starts or
    /// ends with one, or has two in a row, draws an empty line where an item
    /// was taken out.
    #[test]
    fn no_column_starts_ends_or_doubles_a_separator() {
        for (title, _, items) in BAR {
            assert_ne!(
                items.first(),
                Some(&Item::Separator),
                "{title:?} starts with one"
            );
            assert_ne!(
                items.last(),
                Some(&Item::Separator),
                "{title:?} ends with one"
            );
            for pair in items.windows(2) {
                assert_ne!(
                    pair,
                    [Item::Separator, Item::Separator],
                    "{title:?} has two in a row"
                );
            }
        }
    }

    /// An item missing from the bar is a press the page answers that nothing
    /// can make; one in it twice is two items greyed and chosen as one.
    #[test]
    fn every_item_of_coffers_own_is_in_the_bar_once_in_order() {
        let drawn: Vec<Command> = BAR
            .iter()
            .flat_map(|(_, _, items)| items.iter())
            .filter_map(|item| match item {
                Item::Coffer(command) => Some(*command),
                _ => None,
            })
            .collect();
        assert_eq!(drawn, Command::ALL);
    }

    /// `ALL` is written out by hand beside the enum, and nothing the compiler
    /// checks ties the two: a variant added to the enum and not to `ALL` is an
    /// item the page answers and the sheet of keys lists, which the bar never
    /// draws and `named` never finds. Read from the enum's own declaration.
    #[test]
    fn every_item_declared_is_one_the_bar_knows() {
        let source = shipped(include_str!("menu.rs"));
        let declared: Vec<&str> = source
            .split("pub enum Command {")
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .expect("Command is declared")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("//") && !line.starts_with('#'))
            .map(|line| line.trim_end_matches(','))
            .collect();
        let known: Vec<String> = Command::ALL
            .iter()
            .map(|command| format!("{command:?}"))
            .collect();
        assert!(declared.len() > 5, "{declared:?}");
        assert_eq!(declared, known, "`Command::ALL` is not every item");
    }

    /// The id a choice arrives with and the word the page answers it by are
    /// one word. Written twice - the match and serde's renaming - and held
    /// together here. An id equal to one of the two submenus AppKit fills in
    /// would hand that submenu's events to the page.
    #[test]
    fn the_page_and_the_bar_call_every_item_by_the_same_word() {
        let mut seen = Vec::new();
        for command in Command::ALL {
            let id = command.id();
            assert_eq!(Command::named(id), Some(command), "{id} names nothing");
            assert_eq!(
                serde_json::to_value(command).expect("a command serialises"),
                serde_json::Value::String(id.to_owned()),
                "{command:?} reaches the page under another word"
            );
            assert_ne!(id, HELP_SUBMENU_ID);
            assert_ne!(id, WINDOW_SUBMENU_ID);
            assert!(!seen.contains(&id), "{id} names two items");
            seen.push(id);
        }
    }

    /// The page names what applies by these words, and a near miss is refused
    /// rather than read as the nearest item.
    #[test]
    fn a_word_the_bar_does_not_use_is_refused() {
        for word in [
            "NewEntry",
            "new_entry",
            "newentry",
            "",
            " lock",
            "lock ",
            "moveToTrash",
            "services",
        ] {
            assert!(
                serde_json::from_value::<Command>(serde_json::Value::String(word.to_owned()))
                    .is_err(),
                "{word:?} was read as an item"
            );
            assert_eq!(Command::named(word), None, "{word:?} named an item");
        }
        assert!(serde_json::from_value::<Command>(serde_json::json!(3)).is_err());
        assert!(serde_json::from_value::<Command>(serde_json::json!(null)).is_err());
    }

    /// A key read the way muda reads one: an optional Shift, an optional
    /// Option, then Command and one key. Anything else either names a key
    /// Coffer has not checked AppKit draws and matches, or is silently no key
    /// at all.
    fn readable(accelerator: &str) -> bool {
        let rest = accelerator.strip_prefix("Shift+").unwrap_or(accelerator);
        let rest = rest.strip_prefix("Alt+").unwrap_or(rest);
        let Some(key) = rest.strip_prefix("CmdOrCtrl+") else {
            return false;
        };
        key == ","
            || key == "Backspace"
            || (key.len() == 1 && key.bytes().all(|b| b.is_ascii_uppercase()))
    }

    #[test]
    fn every_key_is_one_muda_reads_and_no_two_items_share_one() {
        let mut taken = Vec::new();
        for command in Command::ALL {
            let Some(accelerator) = command.accelerator() else {
                continue;
            };
            assert!(
                readable(accelerator),
                "{command:?} is on {accelerator:?}, which the bar draws as no key"
            );
            assert!(
                !taken.contains(&accelerator),
                "{accelerator} chooses two items"
            );
            taken.push(accelerator);
        }

        // The reader itself, against the shapes that slip past by a character.
        for wrong in [
            "Cmd+L",
            "CmdOrCtrl+l",
            "CmdOrCtrl+",
            "CmdOrCtrl+LL",
            "Shift+Shift+CmdOrCtrl+N",
            "CmdOrCtrl+Shift+N",
            "Ctrl+N",
            "CmdOrCtrl+Delete",
        ] {
            assert!(!readable(wrong), "{wrong} passed as a key");
        }
    }

    /// The keys AppKit and the text fields already answer. An item on Cmd+C
    /// would take copying away from every field in the window, one on Cmd+W or
    /// Cmd+Q away from the system.
    #[test]
    fn no_item_takes_a_key_a_text_field_or_the_system_needs() {
        let kept = [
            "CmdOrCtrl+C",
            "CmdOrCtrl+V",
            "CmdOrCtrl+X",
            "CmdOrCtrl+A",
            "CmdOrCtrl+Z",
            "Shift+CmdOrCtrl+Z",
            "CmdOrCtrl+W",
            "CmdOrCtrl+Q",
            "CmdOrCtrl+H",
            "Alt+CmdOrCtrl+H",
            "CmdOrCtrl+M",
        ];
        for command in Command::ALL {
            if let Some(accelerator) = command.accelerator() {
                assert!(
                    !kept.contains(&accelerator),
                    "{command:?} takes {accelerator}"
                );
            }
        }
    }

    /// With no page there is nobody to answer most items, and the two that
    /// remain are the two that build the window again.
    #[test]
    fn with_no_page_only_what_builds_the_window_again_is_offered() {
        for command in Command::ALL {
            assert_eq!(
                enabled(command, None),
                matches!(command, Command::OpenVault | Command::Settings),
                "{command:?}"
            );
            assert!(
                !enabled(command, Some(&[])),
                "{command:?} is offered by a page that said nothing applies"
            );
            assert!(enabled(command, Some(&Command::ALL)));
        }
    }

    /// The page says what its screens can do, and the session has the last
    /// word on the two items that are about the session.
    #[test]
    fn locking_and_opening_another_follow_the_session() {
        for command in Command::ALL {
            match command {
                Command::Lock => {
                    assert!(command.allowed(true));
                    assert!(!command.allowed(false), "a lock with nothing open");
                }
                Command::OpenVault => {
                    assert!(!command.allowed(true), "another vault over an open one");
                    assert!(command.allowed(false));
                }
                _ => {
                    assert!(command.allowed(true), "{command:?}");
                    assert!(command.allowed(false), "{command:?}");
                }
            }
        }
    }
}
