//! The one menu under the pointer the window has open: building it on the
//! thread AppKit draws on, putting it under the pointer, and turning the item
//! chosen back into what it is about.
//!
//! AppKit tracks a menu under the pointer inside the call that shows it, on
//! the thread the window is drawn on, and nothing else queued for that thread
//! runs until the menu closes: tao handles what Tauri posts there only between
//! its own callbacks (`app_state.rs` in tao 0.35.3), and the menu is shown from
//! inside one. The item chosen is a `MenuEvent` that waits its turn like the
//! rest and is handled once the menu has gone, and it carries the item's id
//! and nothing else. So the id says which menu and which line, and what that
//! line is about is kept here until the event comes.
//!
//! Nothing here reaches the session. The vault was read before the menu was
//! asked for, and the thread the menu is drawn on never waits for a save.

use std::sync::{Mutex, MutexGuard};

use tauri::menu::{IsMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Runtime, Wry};

use super::{Item, Pick, chosen};
use crate::dto::{Chosen, Point, Subject};
use crate::error::Failure;
use crate::{main_thread, window};

/// What every one of these menus' item ids begins with. The menu bar's never
/// do (`menu::Command`).
const PREFIX: &str = "context:";

/// The menus under the pointer, managed by the application. One small lock,
/// never held across anything that waits.
#[derive(Default)]
pub struct Menus {
    asked: Mutex<Asked>,
}

/// What the window asked for last, and the menu that is, or was last, on the
/// screen.
#[derive(Default)]
struct Asked {
    /// The label of the window that asked, which a lock clears and the
    /// window it builds changes: nothing chosen in the menu of a window that
    /// has gone reaches the next one.
    window: String,
    /// The number of the menu it asked for last. Only that one is drawn.
    newest: u64,
    /// What the menu drawn last is about, until an item of it is chosen.
    open: Option<Open>,
    /// The menu drawn last, kept until the next replaces it: AppKit hands the
    /// choice to the item while it tracks the menu, and the item must still
    /// be there when it does.
    kept: Option<Menu<Wry>>,
}

/// What a menu on the screen is about: the number the window gave it, the
/// subject, and what each of its items does, by the number in the item's id.
struct Open {
    serial: u64,
    subject: Subject,
    picks: Vec<Option<Pick>>,
}

/// A line of a menu as AppKit is to draw it: the item's id instead of what
/// the item does, which stays here.
enum Line {
    Choice {
        id: String,
        label: String,
        enabled: bool,
    },
    Separator,
    Menu {
        label: String,
        enabled: bool,
        lines: Vec<Line>,
    },
}

impl Menus {
    fn asked(&self) -> MutexGuard<'_, Asked> {
        // Nothing here is left half written by a panic, and a lock that
        // stopped answering would be a menu that never opens again.
        self.asked.lock().unwrap_or_else(|it| it.into_inner())
    }

    /// The window `window` asked for menu `serial`, which is now the one to
    /// draw. A page counts its menus from one, and a page that loaded again
    /// counts from one again, so the last asked is the newest whatever its
    /// number.
    fn ask(&self, window: &str, serial: u64) {
        let mut asked = self.asked();
        if asked.window != window {
            asked.window = window.to_owned();
            asked.open = None;
        }
        asked.newest = serial;
    }

    /// Whether menu `serial` of `window` is still the one to draw.
    fn newest(&self, window: &str, serial: u64) -> bool {
        let asked = self.asked();
        asked.window == window && asked.newest == serial
    }

    /// The menu `open` is about is the one on the screen.
    fn opened(&self, window: &str, open: Open) {
        let mut asked = self.asked();
        if asked.window == window {
            asked.open = Some(open);
        }
    }

    /// Keeps `menu`, and hands back the one it replaces, to be let go of
    /// outside the lock.
    fn keep(&self, menu: Menu<Wry>) -> Option<Menu<Wry>> {
        self.asked().kept.replace(menu)
    }

    /// The item `index` of menu `serial`, chosen in `window`, with what it is
    /// about: only from the menu drawn last, of the window that asked for it,
    /// and once.
    fn take(&self, window: &str, serial: u64, index: usize) -> Option<Chosen> {
        let open = {
            let mut asked = self.asked();
            if asked.window != window {
                return None;
            }
            asked.open.take_if(|open| open.serial == serial)?
        };
        let pick = open.picks.get(index)?.as_ref()?;
        chosen(&open.subject, pick)
    }

    /// Lets go of what the menu was about, and hands back the menu, to be let
    /// go of outside the lock.
    ///
    /// The window that asked is let go of too. A menu asked for before the
    /// lock and not drawn yet - its vault read, its turn on the thread that
    /// draws still to come - is then no longer the newest of any window: it
    /// is not drawn over the window the lock takes down, out of names the
    /// lock has wiped, and nothing is kept for it to answer.
    fn forget(&self) -> Option<Menu<Wry>> {
        let mut asked = self.asked();
        asked.window.clear();
        asked.open = None;
        asked.kept.take()
    }
}

/// The window `window` asked for menu `serial`: a menu it asked for before
/// and that has not been drawn yet - waiting behind a save for the vault - is
/// not drawn now.
pub fn ask<R: Runtime>(app: &AppHandle<R>, window: &str, serial: u64) {
    if let Some(menus) = app.try_state::<Menus>() {
        menus.ask(window, serial);
    }
}

/// Puts `items`, a menu about `subject`, under the pointer in the window
/// `window`, at `at`, and answers once it has closed. The item chosen arrives
/// on its own, as a `MenuEvent` for [`picked`], before or after this answers.
pub async fn pop(
    app: &AppHandle,
    window: String,
    serial: u64,
    subject: Subject,
    items: Vec<Item>,
    at: Point,
) -> Result<(), Failure> {
    let mut picks = Vec::new();
    let lines = numbered(serial, items, &mut picks);
    let open = Open {
        serial,
        subject,
        picks,
    };
    let handle = app.clone();
    main_thread::answer(app, move |_| show(&handle, &window, open, &lines, at)).await?
}

/// On the thread AppKit draws on: builds the menu and tracks it until it
/// closes.
fn show(
    app: &AppHandle,
    label: &str,
    open: Open,
    lines: &[Line],
    at: Point,
) -> Result<(), Failure> {
    let Some(menus) = app.try_state::<Menus>() else {
        return Ok(());
    };
    if !menus.newest(label, open.serial) {
        return Ok(());
    }
    // muda shows the menu in the window's view, and panics on a view that is
    // in no window: the window is looked for here, right before.
    let Some(window) = app.get_webview_window(label) else {
        return Ok(());
    };
    let menu = built(app, lines).map_err(|_| unshown())?;
    menus.opened(label, open);
    drop(menus.keep(menu.clone()));
    window
        .popup_menu_at(&menu, at.logical())
        .map_err(|_| unshown())
}

fn unshown() -> Failure {
    Failure::internal("the menu could not be shown")
}

/// Gives every item an id - [`PREFIX`], the menu's number, the item's number -
/// and puts what the item does at that number in `picks`. A greyed item has a
/// number too, holding nothing. Every title is written the way muda reads it
/// ([`drawn`]).
fn numbered(serial: u64, items: Vec<Item>, picks: &mut Vec<Option<Pick>>) -> Vec<Line> {
    items
        .into_iter()
        .map(|item| match item {
            Item::Choice { label, pick } => {
                let id = format!("{PREFIX}{serial}:{}", picks.len());
                let enabled = pick.is_some();
                picks.push(pick);
                Line::Choice {
                    id,
                    label: drawn(&label),
                    enabled,
                }
            }
            Item::Separator => Line::Separator,
            Item::Menu {
                label,
                enabled,
                items,
            } => Line::Menu {
                label: drawn(&label),
                enabled,
                lines: numbered(serial, items, picks),
            },
        })
        .collect()
}

/// A title as muda is to be handed it, so that AppKit draws what it says.
///
/// AppKit draws a title on one line, so a control character or a line or
/// paragraph separator - a folder's name may hold any of them - is a space.
/// And muda reads `&` in every title as the mark of a key and drops it, `&&`
/// as one `&`, and the text `[~~]` as `&` (`strip_mnemonic` in muda 0.19.3's
/// `platform_impl/macos/util.rs`), so a folder called "R&D" would be offered
/// as "RD". A word joiner, which draws nothing, keeps `[~~]` from being read.
/// Every title goes through here, the menus' own words as well as names: the
/// rule is about what muda does to a title, whoever wrote it.
fn drawn(label: &str) -> String {
    label
        .chars()
        .map(|character| {
            if character.is_control() || matches!(character, '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .replace("[~~]", "[~\u{2060}~]")
        .replace('&', "&&")
}

/// The menu, on the thread AppKit draws on, where each of Tauri's builders
/// runs at once rather than waiting for a turn of its own.
///
/// A separator is the only one of AppKit's own items these menus use: a
/// predefined Copy would be the ordinary pasteboard write `guard.ts` exists
/// to stop.
fn built(app: &AppHandle, lines: &[Line]) -> tauri::Result<Menu<Wry>> {
    let items = kinds(app, lines)?;
    Menu::with_items(app, &references(&items))
}

fn kinds(app: &AppHandle, lines: &[Line]) -> tauri::Result<Vec<MenuItemKind<Wry>>> {
    lines
        .iter()
        .map(|line| {
            Ok(match line {
                Line::Choice { id, label, enabled } => MenuItemKind::MenuItem(MenuItem::with_id(
                    app,
                    id.as_str(),
                    label,
                    *enabled,
                    None::<&str>,
                )?),
                Line::Separator => MenuItemKind::Predefined(PredefinedMenuItem::separator(app)?),
                Line::Menu {
                    label,
                    enabled,
                    lines,
                } => {
                    let inside = kinds(app, lines)?;
                    MenuItemKind::Submenu(Submenu::with_items(
                        app,
                        label,
                        *enabled,
                        &references(&inside),
                    )?)
                }
            })
        })
        .collect()
}

fn references(items: &[MenuItemKind<Wry>]) -> Vec<&dyn IsMenuItem<Wry>> {
    items
        .iter()
        .map(|item| item as &dyn IsMenuItem<Wry>)
        .collect()
}

/// The menu and the item an id names, or nothing for an id that is not one
/// of these menus'.
fn parse(id: &str) -> Option<(u64, usize)> {
    let (serial, index) = id.strip_prefix(PREFIX)?.split_once(':')?;
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    if !digits(serial) || !digits(index) {
        return None;
    }
    Some((serial.parse().ok()?, index.parse().ok()?))
}

/// An item of a menu under the pointer was chosen, with what it is about: the
/// menu's number and the ids, for the page. Nothing for any other menu's
/// item. On the main thread, where every menu event arrives, and nothing here
/// waits for more than the one small lock above.
pub fn picked<R: Runtime>(app: &AppHandle<R>, id: &str) -> Option<(u64, Chosen)> {
    let (serial, index) = parse(id)?;
    let menus = app.try_state::<Menus>()?;
    let chosen = menus.take(&window::label(), serial, index)?;
    Some((serial, chosen))
}

/// The vault locked: what the menu was about is names and ids out of the
/// vault, and they go with it. A menu still on the screen stays until it is
/// dismissed - the window's destroy waits for it - and nothing chosen in it
/// reaches anything. A menu asked for and not drawn yet is not drawn.
pub fn forget<R: Runtime>(app: &AppHandle<R>) {
    if let Some(menus) = app.try_state::<Menus>() {
        // Let go of here, outside the lock: Tauri takes the menu down on the
        // thread AppKit draws on.
        drop(menus.forget());
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::context::Item;
    use crate::source::{every_file, shipped};

    fn subject() -> Subject {
        serde_json::from_value(json!({
            "kind": "field", "entry": "an entry", "field": "PIN", "shown": false
        }))
        .expect("the subject reads")
    }

    /// A menu of three lines, the middle one greyed out, about `subject`.
    fn open(serial: u64) -> Open {
        Open {
            serial,
            subject: subject(),
            picks: vec![Some(Pick::Show), None, Some(Pick::Remove)],
        }
    }

    fn item(chosen: Option<Chosen>) -> Option<serde_json::Value> {
        chosen.map(|chosen| serde_json::to_value(chosen).expect("it serialises"))
    }

    /// A choice is the menu's on the screen only, of the window that asked for
    /// it, and is made once: AppKit draws a greyed line it does not hand on,
    /// and an id a page or another menu made up names nothing.
    #[test]
    fn an_item_is_taken_once_and_only_from_the_menu_on_the_screen() {
        let menus = Menus::default();
        menus.ask("main-3", 4);
        menus.opened("main-3", open(4));

        assert!(menus.take("main-3", 3, 0).is_none(), "another menu's");
        assert!(menus.take("main-2", 4, 0).is_none(), "another window's");
        assert!(menus.take("main-3", 4, 1).is_none(), "a greyed line");
        assert!(
            menus.take("main-3", 4, 0).is_none(),
            "taken by the greyed one"
        );

        menus.opened("main-3", open(4));
        assert!(menus.take("main-3", 4, 3).is_none(), "past the last line");
        menus.opened("main-3", open(4));
        assert_eq!(
            item(menus.take("main-3", 4, 2)),
            Some(json!({ "item": "removeField", "entry": "an entry", "field": "PIN" }))
        );
        assert!(menus.take("main-3", 4, 2).is_none(), "chosen twice");
        assert!(menus.take("main-3", 4, 0).is_none());

        menus.ask("main-3", 5);
        menus.opened("main-4", open(5));
        assert!(
            menus.take("main-4", 5, 0).is_none(),
            "a window that never asked"
        );
    }

    #[test]
    fn an_id_that_is_not_one_of_these_menus_is_nobodys() {
        assert_eq!(parse("context:12:3"), Some((12, 3)));
        assert_eq!(parse("context:0:0"), Some((0, 0)));
        for id in [
            "",
            "context:",
            "context:1",
            "context:1:",
            "context::1",
            "context:x:1",
            "context:1:-1",
            "context:+1:1",
            "context: 1:1",
            "context:18446744073709551616:0",
            "context:1:1:1",
            "context:1:99999999999999999999999",
            "Context:1:1",
            "lock",
            "newEntry",
            "1",
        ] {
            assert_eq!(parse(id), None, "{id:?}");
        }
    }

    /// The menu asked for last is the one drawn: one that waited behind a
    /// save while the reader right-clicked again is not drawn after it. A page
    /// that loaded again counts from one, and its first menu is drawn.
    #[test]
    fn only_the_newest_right_click_draws_a_menu() {
        let menus = Menus::default();
        menus.ask("main", 1);
        menus.ask("main", 2);
        assert!(!menus.newest("main", 1));
        assert!(menus.newest("main", 2));

        menus.ask("main", 1);
        assert!(menus.newest("main", 1), "a page loaded again");

        menus.ask("main-1", 1);
        assert!(!menus.newest("main", 1));
        assert!(menus.newest("main-1", 1));
    }

    /// A lock wipes the tree, and with it what the menu was about.
    #[test]
    fn a_lock_forgets_what_the_menu_was_about() {
        let menus = Menus::default();
        menus.ask("main", 1);
        menus.opened("main", open(1));

        assert!(menus.forget().is_none(), "no menu was drawn in a test");
        assert!(menus.take("main", 1, 0).is_none());
    }

    /// A right-click read the vault, and the lock came before the menu's turn
    /// on the thread that draws. That menu is no longer the one to draw, and
    /// were it shown anyway nothing in it would answer. The next right-click
    /// - of the window the lock left, or of the one it brought back - is.
    #[test]
    fn a_menu_asked_for_before_a_lock_is_not_drawn_after_it() {
        let menus = Menus::default();
        menus.ask("main", 1);
        drop(menus.forget());

        assert!(!menus.newest("main", 1), "drawn after the lock");
        menus.opened("main", open(1));
        assert!(
            menus.take("main", 1, 0).is_none(),
            "answered after the lock"
        );

        menus.ask("main-1", 1);
        assert!(menus.newest("main-1", 1));
        menus.opened("main-1", open(1));
        assert!(menus.take("main-1", 1, 0).is_some());
    }

    /// Every line can be told by its id, and the id names what the line does:
    /// something for a line that can be chosen, nothing for a greyed one,
    /// inside a submenu as at the top.
    #[test]
    fn every_line_is_numbered_for_what_it_does() {
        let items = vec![
            Item::Choice {
                label: "Show".to_owned(),
                pick: Some(Pick::Show),
            },
            Item::Separator,
            Item::Menu {
                label: "Move to".to_owned(),
                enabled: true,
                items: vec![
                    Item::Choice {
                        label: "Work".to_owned(),
                        pick: None,
                    },
                    Item::Choice {
                        label: "Home".to_owned(),
                        pick: Some(Pick::MoveInto("home".to_owned())),
                    },
                ],
            },
            Item::Choice {
                label: "Remove".to_owned(),
                pick: Some(Pick::Remove),
            },
        ];
        let mut picks = Vec::new();
        let lines = numbered(7, items, &mut picks);
        assert_eq!(picks.len(), 4);

        let mut seen = Vec::new();
        let mut pending: Vec<&Line> = lines.iter().collect();
        while let Some(line) = pending.pop() {
            match line {
                Line::Choice { id, label, enabled } => {
                    let (serial, index) = parse(id).expect("the id is one of these menus'");
                    assert_eq!(serial, 7);
                    assert_eq!(
                        picks.get(index).map(Option::is_some),
                        Some(*enabled),
                        "{label}"
                    );
                    seen.push(index);
                }
                Line::Menu { lines, .. } => pending.extend(lines),
                Line::Separator => {}
            }
        }
        seen.sort_unstable();
        assert_eq!(seen, [0, 1, 2, 3], "two lines share a number");
    }

    /// muda reads `&` and `[~~]` in a title; a folder's name may hold either,
    /// and control characters and separators that would break the line. After
    /// muda's reading - `strip_mnemonic` in muda 0.19.3's
    /// `platform_impl/macos/util.rs`, copied below - the title says what it
    /// was given, with a space where it would have broken: a submenu's
    /// heading and a line inside it as much as a line at the top.
    #[test]
    fn a_name_cannot_rewrite_the_item_it_is_in() {
        fn strip_mnemonic(string: &str) -> String {
            string
                .replace("&&", "[~~]")
                .replace('&', "")
                .replace("[~~]", "&")
        }
        let shown = |title: &str| strip_mnemonic(title).replace('\u{2060}', "");

        for (name, read) in [
            ("R&D", "R&D"),
            ("&&", "&&"),
            ("&", "&"),
            ("[~~]", "[~~]"),
            ("a[~~]&b", "a[~~]&b"),
            ("[~&~]", "[~&~]"),
            ("[[~~]~]", "[[~~]~]"),
            ("\u{202e}xyz", "\u{202e}xyz"),
            ("\u{2068}open\u{2069}", "\u{2068}open\u{2069}"),
            ("one\ntwo", "one two"),
            ("one\u{2029}two\u{2028}three", "one two three"),
            ("nul\u{0}here", "nul here"),
            ("tab\there", "tab here"),
            ("\u{301}\u{301}", "\u{301}\u{301}"),
            ("   ", "   "),
            ("", ""),
        ] {
            assert_eq!(shown(&drawn(name)), read, "{name:?}");
        }
        let megabyte = "&[~~]".repeat(1 << 18);
        assert_eq!(shown(&drawn(&megabyte)), megabyte);

        let lines = numbered(
            1,
            vec![
                Item::Menu {
                    label: "R&D".to_owned(),
                    enabled: true,
                    items: vec![Item::Choice {
                        label: "a[~~]&b".to_owned(),
                        pick: Some(Pick::Show),
                    }],
                },
                Item::Choice {
                    label: "one\ntwo & three".to_owned(),
                    pick: None,
                },
            ],
            &mut Vec::new(),
        );
        let mut titles = Vec::new();
        let mut pending: Vec<&Line> = lines.iter().rev().collect();
        while let Some(line) = pending.pop() {
            match line {
                Line::Choice { label, .. } => titles.push(shown(label)),
                Line::Menu { label, lines, .. } => {
                    titles.push(shown(label));
                    pending.extend(lines.iter().rev());
                }
                Line::Separator => {}
            }
        }
        assert_eq!(titles, ["R&D", "a[~~]&b", "one two & three"]);
    }

    /// The lock forgets the menu once it knows it is the one locking, and
    /// before anything else it does: the tree has just gone, and the names the
    /// menu was about go with it.
    #[test]
    fn a_lock_forgets_the_menu_as_soon_as_the_tree_is_gone() {
        let lock = shipped(include_str!("../lock.rs"));
        let at = |call: &str| {
            lock.find(call)
                .unwrap_or_else(|| panic!("the lock no longer calls {call}"))
        };
        assert!(at("session.lock(reason)") < at("context::forget(app)"));
        assert!(at("context::forget(app)") < at("clipboard::revoke_pending()"));
    }

    /// AppKit's own Copy, Cut, Paste and Services are the ordinary pasteboard
    /// and other applications. These menus are built from Coffer's items and
    /// separators and nothing else.
    #[test]
    fn these_menus_offer_nothing_of_the_systems_own() {
        let mut read = 0;
        for (path, file) in every_file() {
            if !path.components().any(|part| part.as_os_str() == "context") {
                continue;
            }
            read += 1;
            let source = shipped(&file);
            for (at, _) in source.match_indices("PredefinedMenuItem::") {
                assert!(
                    source[at..].starts_with("PredefinedMenuItem::separator("),
                    "{} builds one of AppKit's items",
                    path.display()
                );
            }
        }
        assert_eq!(read, 2, "the menus are in two files");
    }
}
