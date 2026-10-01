//! The way from a choice made outside the page to the page.
//!
//! Coffer's items in the menu bar are presses the page answers with the
//! function its button runs, so there is one implementation of each. Rust
//! hears the choice as a `MenuEvent` on the main thread and sends it through
//! the one `Channel` the page handed over with `listen`.
//!
//! A channel and not a Tauri event. An event needs two capabilities the window
//! does not have, and Tauri never clears a destroyed window's listeners (see
//! docs/ipc.md, Locking); a channel needs none, and it is let go of when its
//! window goes.

use std::sync::{Mutex, MutexGuard};

use tauri::ipc::Channel;
use tauri::menu::MenuEvent;
use tauri::{AppHandle, Manager, Runtime};

use crate::dto::Action;
use crate::menu::Command;
use crate::window;

/// The page that is listening, if one is, and what was chosen while none was.
#[derive(Default)]
pub struct Route {
    held: Mutex<Ear>,
}

#[derive(Default)]
struct Ear {
    /// The label of the window the page is in, and its way in. The label
    /// tells one window's page from the next one's: a message the last page
    /// sent can arrive after the next page is listening.
    page: Option<(String, Channel<Action>)>,
    /// A choice made with no page to answer it, for the next page.
    waiting: Option<Action>,
}

impl Route {
    fn ear(&self) -> MutexGuard<'_, Ear> {
        // Nothing here is left half-written by a panic, and a route that
        // stopped answering would be a menu bar that does nothing.
        self.held.lock().unwrap_or_else(|it| it.into_inner())
    }

    /// The page in `window` is listening. Its way in replaces the last page's,
    /// and what was chosen while no page was there is handed over, once.
    pub fn listen(&self, window: &str, channel: Channel<Action>) {
        let (gone, waiting) = {
            let mut ear = self.ear();
            let gone = ear.page.replace((window.to_owned(), channel.clone()));
            (gone, ear.waiting.take())
        };
        // Outside the lock: sending evaluates script in the page, and letting
        // the last channel go tells its page so.
        drop(gone);
        if let Some(action) = waiting {
            let _ = channel.send(action);
        }
    }

    /// The window went, and its page with it. A choice made now waits for the
    /// next page. A window that is not the one listening changes nothing, and
    /// the answer says whether this one was.
    pub fn forget(&self, window: &str) -> bool {
        let gone = {
            let mut ear = self.ear();
            if ear.page.as_ref().is_some_and(|(label, _)| label == window) {
                ear.page.take()
            } else {
                None
            }
        };
        // Let go of here, outside the lock: the last clone of a page's channel
        // tells the page so as it goes, which evaluates script in it.
        gone.is_some()
    }

    /// Whether the page in `window` is the one listening.
    pub fn hears(&self, window: &str) -> bool {
        self.ear()
            .page
            .as_ref()
            .is_some_and(|(label, _)| label == window)
    }

    /// Tells the page, and answers whether there was a page to tell. With
    /// none, nothing is kept: the close button is about the window that had
    /// it, and is not a choice for the next one.
    pub fn tell(&self, action: Action) -> bool {
        self.reach(action, false)
    }

    /// Tells the page, or with no page keeps the choice for the next one, and
    /// answers whether a page was told. Only the last choice is kept: two
    /// before a window is back are one reader changing their mind.
    pub fn deliver(&self, action: Action) -> bool {
        self.reach(action, true)
    }

    /// Whether there is a page and keeping the choice for the next are decided
    /// under one lock. Decided apart, a page could start listening between
    /// the two, and the choice would wait for the window after it - the next
    /// lock's, hours later - and happen there by itself. A send that fails is
    /// not kept either: it was a page's, and the page was there.
    fn reach(&self, action: Action, keep: bool) -> bool {
        let channel = {
            let mut ear = self.ear();
            match &ear.page {
                Some((_, channel)) => channel.clone(),
                None => {
                    if keep {
                        ear.waiting = Some(action);
                    }
                    return false;
                }
            }
        };
        // Outside the lock: sending evaluates script in the page.
        channel.send(action).is_ok()
    }
}

/// A Coffer item in the menu bar was chosen; registered with
/// `Builder::on_menu_event`. On the main thread, where every menu event
/// arrives (`app.rs` in tauri 2.11.5). AppKit's own items carry ids Coffer
/// does not know and are left alone.
pub fn chosen<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let Some(command) = Command::named(event.id().as_ref()) else {
        return;
    };
    let Some(route) = app.try_state::<Route>() else {
        return;
    };
    let told = route.deliver(Action::Command { command });
    // What was chosen happens in the window, so the window comes forward for
    // it. A minimised one still has its page listening and the bar as that
    // page left it, and AppKit hands the bar its keys with no window in front:
    // Move to Recycle Bin would run out of sight, and its offer to undo run
    // out unread. With no page - the reader closed the window, or one is still
    // loading - the choice waits for the page that is coming, and the window
    // is asked for. Lock Vault alone is left where it is: it takes the window
    // down, and the one it builds comes forward asking for the password.
    if told && command == Command::Lock {
        return;
    }
    window::bring_back(app);
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use tauri::ipc::InvokeResponseBody;

    use super::*;

    type Heard = Arc<Mutex<Vec<String>>>;

    /// A way in that writes down every message it carries, the way the page
    /// would read them. The record is shared with the channel and nothing
    /// else, so it has one owner left exactly when the route let go of the
    /// channel.
    fn page() -> (Channel<Action>, Heard) {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let writing = Arc::clone(&heard);
        let channel = Channel::new(move |body| {
            if let InvokeResponseBody::Json(message) = body {
                writing.lock().expect("the record is there").push(message);
            }
            Ok(())
        });
        (channel, heard)
    }

    /// A way in to a webview that has gone: every send fails, and is counted.
    fn dead() -> (Channel<Action>, Arc<Mutex<usize>>) {
        let tried = Arc::new(Mutex::new(0));
        let counting = Arc::clone(&tried);
        let channel = Channel::new(move |_| {
            *counting.lock().expect("the count is there") += 1;
            Err(tauri::Error::WebviewNotFound)
        });
        (channel, tried)
    }

    fn chose(command: Command) -> Action {
        Action::Command { command }
    }

    fn said(heard: &Heard) -> Vec<String> {
        heard.lock().expect("the record is there").clone()
    }

    const NEW_ENTRY: &str = r#"{"action":"command","command":"newEntry"}"#;
    const LOCK: &str = r#"{"action":"command","command":"lock"}"#;
    const CLOSING: &str = r#"{"action":"closing"}"#;

    /// A page replaced by the next window's must not go on hearing choices:
    /// it is a destroyed webview, and the choice would be lost in it.
    #[test]
    fn a_choice_reaches_the_page_listening_now_and_no_earlier_one() {
        let route = Route::default();
        let (first, earlier) = page();
        let (second, now) = page();

        route.listen("main", first);
        route.listen("main-1", second);
        assert!(route.deliver(chose(Command::NewEntry)));

        assert!(said(&earlier).is_empty(), "the window that went heard it");
        assert_eq!(said(&now), [NEW_ENTRY]);
        assert!(route.hears("main-1"));
        assert!(!route.hears("main"));
        assert_eq!(Arc::strong_count(&earlier), 1, "the replaced page is held");
    }

    #[test]
    fn nothing_is_told_once_the_page_went_with_its_window() {
        let route = Route::default();
        let (channel, heard) = page();
        route.listen("main", channel);

        assert!(route.forget("main"));

        assert!(!route.tell(Action::Closing));
        assert!(said(&heard).is_empty());
        assert!(!route.hears("main"));
        assert_eq!(Arc::strong_count(&heard), 1, "the page that went is held");
    }

    /// The window that goes is not always the one listening: the last page's
    /// `Destroyed` can be handled after the next page has listened. Forgetting
    /// it must not deafen the page that is there, and must say that it did
    /// not, so the bar is not put back to what no page offers.
    #[test]
    fn a_window_that_went_late_does_not_take_the_next_page_with_it() {
        let route = Route::default();
        let (old, _) = page();
        let (new, heard) = page();
        route.listen("main", old);
        route.listen("main-1", new);

        assert!(
            !route.forget("main"),
            "a page that had already been replaced"
        );

        assert!(route.hears("main-1"));
        assert!(route.deliver(chose(Command::Lock)));
        assert_eq!(said(&heard), [LOCK]);
    }

    /// The reader closed the window and chose Open Vault… from the bar. The
    /// choice is what the new window is for, and it is made once.
    #[test]
    fn a_choice_made_with_no_page_is_delivered_once_to_the_next_page() {
        let route = Route::default();
        assert!(!route.deliver(chose(Command::NewEntry)));

        let (first, heard) = page();
        route.listen("main-1", first);
        assert_eq!(said(&heard), [NEW_ENTRY]);

        let (second, again) = page();
        route.listen("main-1", second);
        assert!(said(&again).is_empty(), "the choice was made twice");
        assert_eq!(said(&heard), [NEW_ENTRY]);
    }

    #[test]
    fn only_the_last_choice_made_with_no_page_waits() {
        let route = Route::default();
        route.deliver(chose(Command::NewEntry));
        route.deliver(chose(Command::Lock));

        let (channel, heard) = page();
        route.listen("main-1", channel);

        assert_eq!(said(&heard), [LOCK]);
    }

    /// The close button belongs to the window it was pressed in. Told to no
    /// page, it is not kept for the next window, which would close itself as
    /// soon as it listened.
    #[test]
    fn a_close_with_no_page_is_not_kept_for_the_next() {
        let route = Route::default();
        assert!(!route.tell(Action::Closing));

        let (channel, heard) = page();
        route.listen("main-1", channel);
        assert!(said(&heard).is_empty(), "the next window was told to close");

        assert!(route.tell(Action::Closing));
        assert_eq!(said(&heard), [CLOSING]);
    }

    /// A choice told to the page there is done with: when that window goes
    /// and the next one listens, it is not made a second time there.
    #[test]
    fn a_choice_made_while_a_page_listens_is_never_kept_for_the_next() {
        let route = Route::default();
        let (first, heard) = page();
        route.listen("main", first);

        assert!(route.deliver(chose(Command::NewEntry)));
        assert!(route.forget("main"));

        let (second, next) = page();
        route.listen("main-1", second);
        assert_eq!(said(&heard), [NEW_ENTRY]);
        assert!(
            said(&next).is_empty(),
            "the next window made the choice again"
        );
    }

    /// A page whose webview has gone and whose `Destroyed` has not been
    /// handled yet: the send fails. The choice was that page's, and keeping it
    /// would make it happen by itself in the next window.
    #[test]
    fn a_send_that_fails_is_not_kept_for_a_later_page() {
        let route = Route::default();
        let (channel, tried) = dead();
        route.listen("main", channel);

        assert!(!route.deliver(chose(Command::NewEntry)));
        assert_eq!(*tried.lock().expect("the count is there"), 1);

        assert!(route.forget("main"));
        let (next, heard) = page();
        route.listen("main-1", next);
        assert!(
            said(&heard).is_empty(),
            "a failed choice waited for a later page"
        );
    }

    /// A page that loaded again hands over a new way in under the same label.
    /// The old one is let go of and hears nothing more; the new one hears each
    /// choice once.
    #[test]
    fn a_page_that_listens_again_hears_each_choice_once() {
        let route = Route::default();
        let (first, before) = page();
        let (second, after) = page();
        route.listen("main", first);
        route.listen("main", second);

        assert!(route.deliver(chose(Command::NewEntry)));
        assert!(
            said(&before).is_empty(),
            "the page that loaded again heard it"
        );
        assert_eq!(said(&after), [NEW_ENTRY]);
        assert_eq!(Arc::strong_count(&before), 1, "the first way in is held");
    }

    /// `reach` decides whether there is a page and keeps the choice for the
    /// next one under one lock. Decided apart, a choice made just as a page
    /// starts listening could find no page, then be kept after the page had
    /// taken what was waiting: the page never hears it, and the window after
    /// it - the next lock's, hours later - makes it by itself.
    #[test]
    fn a_choice_made_as_a_page_starts_listening_reaches_that_page_and_nothing_waits() {
        for round in 0..1000 {
            let route = Arc::new(Route::default());
            let (channel, heard) = page();
            let start = Arc::new(Barrier::new(2));
            let listening = {
                let (route, start) = (Arc::clone(&route), Arc::clone(&start));
                std::thread::spawn(move || {
                    start.wait();
                    route.listen("main", channel);
                })
            };
            start.wait();
            route.deliver(chose(Command::NewEntry));
            listening.join().expect("the page listens");

            assert_eq!(said(&heard), [NEW_ENTRY], "round {round}");
            let (next, later) = page();
            route.listen("main-1", next);
            assert!(
                said(&later).is_empty(),
                "round {round}: the choice waited for the window after"
            );
        }
    }

    /// Lock, unlock, lock again: every window that goes takes its page with
    /// it - the route holds nothing of it afterwards - a choice made while no
    /// page listens reaches exactly the next one, and at the end exactly the
    /// last page is listening with nothing left over for it.
    #[test]
    fn a_hundred_locks_leave_one_page_listening_and_nothing_waiting() {
        let route = Route::default();
        let mut pages = Vec::new();
        for round in 0..100 {
            let label = format!("main-{round}");
            let (channel, heard) = page();
            route.listen(&label, channel);
            let waited = round % 3 == 0 && round > 0;
            if waited {
                assert_eq!(said(&heard), [NEW_ENTRY], "round {round}");
            } else {
                assert!(said(&heard).is_empty(), "round {round}");
            }
            assert!(route.deliver(chose(Command::Lock)));
            assert!(route.forget(&label));
            assert!(!route.tell(Action::Closing), "round {round}");
            if round % 3 == 2 {
                assert!(!route.deliver(chose(Command::NewEntry)));
            }
            assert_eq!(Arc::strong_count(&heard), 1, "round {round} is held");
            pages.push(heard);
        }

        let (last, heard) = page();
        route.listen("main-100", last);
        assert!(said(&heard).is_empty(), "a choice was left waiting");
        assert!(route.deliver(chose(Command::NewEntry)));

        for (round, earlier) in pages.iter().enumerate() {
            let told = if round % 3 == 0 && round > 0 {
                vec![NEW_ENTRY, LOCK]
            } else {
                vec![LOCK]
            };
            assert_eq!(said(earlier), told, "round {round}");
        }
        assert_eq!(said(&heard), [NEW_ENTRY]);
        assert!(route.hears("main-100"));
        assert!((0..100).all(|round| !route.hears(&format!("main-{round}"))));
    }
}
