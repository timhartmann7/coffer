//! Closing the window.
//!
//! It locks the vault: a window that went and left its vault open would be a
//! decrypted vault in a process nobody can see. And it does not come back on
//! its own: Coffer waits in the Dock, and a click there builds the window again
//! the way a lock does (`window::bring_back`).
//!
//! The close is asked of the page first, because only the page can send what
//! the reader typed in the last quarter of a second. A page that has not
//! answered within its grace is closed from here all the same, because a close
//! JavaScript can hold up is a lock JavaScript can refuse. The grace runs only
//! while nothing holds the session: a page whose last words are waiting behind
//! a save is answering, only slowly.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

use crate::autolock::timer::Timer;
use crate::autolock::{Event, Reason};
use crate::dto::Action;
use crate::route::Route;
use crate::session::Session;

/// How long a page is given to send what is being typed and ask for the close
/// itself, in pauses: three seconds.
///
/// Counted only while nothing holds the session. A save holds it for a key
/// derivation and the encryption of the whole file, which on a vault carrying
/// documents is seconds, and the page's drafts wait behind it. So would a lock
/// from here - and the session is not handed out in the order it was asked
/// for, so a lock that took it first would write the vault without what the
/// reader typed during the save, which is the loss asking the page exists to
/// prevent.
const GRACE: u32 = 30;

/// One pause of the grace, between two looks at the window and the session.
const PAUSE: Duration = Duration::from_millis(100);

/// What a close does to the application around it. When each happens, and on
/// which thread, is decided here; how is Tauri's, and a test stands in for it,
/// because a test has no window and no main thread to build one on.
pub trait Closer: Clone + Send + 'static {
    /// Tells the page the reader asked to close its window, and answers
    /// whether a page was listening to be told.
    fn tell(&self) -> bool;
    /// Hands the timer a message, the way every other trigger reaches it.
    fn post(&self, event: Event);
    /// Locks straight away, on the thread this is called on.
    fn lock(&self, reason: Reason);
    /// Whether the window `label` is still there.
    fn up(&self, label: &str) -> bool;
    /// Takes the window `label` down.
    fn destroy(&self, label: &str);
    /// Waits out one pause of the page's grace.
    fn pause(&self);
    /// Whether something holds the session now, without waiting for it.
    fn busy(&self) -> bool;
}

impl<R: Runtime> Closer for AppHandle<R> {
    fn tell(&self) -> bool {
        self.try_state::<Route>()
            .is_some_and(|route| route.tell(Action::Closing))
    }

    fn post(&self, event: Event) {
        if let Some(timer) = self.try_state::<Arc<Timer>>() {
            timer.post(event);
        }
    }

    fn lock(&self, reason: Reason) {
        crate::lock::lock(self, reason);
    }

    fn up(&self, label: &str) -> bool {
        self.get_webview_window(label).is_some()
    }

    fn destroy(&self, label: &str) {
        if let Some(window) = self.get_webview_window(label) {
            let _ = window.destroy();
        }
    }

    fn pause(&self) {
        std::thread::sleep(PAUSE);
    }

    fn busy(&self) -> bool {
        self.try_state::<Arc<Session>>()
            .is_some_and(|session| session.busy())
    }
}

/// The reader pressed the close button of the window `label`, or chose Close
/// Window, on the main thread. Answers whether the window stays up for now: it
/// does while a page is listening, which then asks for `close_window`. With no
/// page the window goes at once.
///
/// Nothing here waits. The lock writes the vault out before it wipes it, which
/// is a key derivation, so it runs on a thread of its own and never on the one
/// AppKit draws on.
pub fn requested<C: Closer>(app: &C, label: &str) -> bool {
    let told = app.tell();
    let handle = app.clone();
    if told {
        let label = label.to_owned();
        std::thread::spawn(move || overdue(&handle, &label));
    } else {
        std::thread::spawn(move || lock(&handle));
    }
    told
}

/// Locks for a closed window: whatever is open, and whatever is still opening.
pub fn lock<C: Closer>(app: &C) {
    // Through the timer, so that the deadline it was keeping is cleared by the
    // same message that locks.
    app.post(Event::Locking(Reason::Closed));
    // Then straight to the lock as well. With nothing open the timer has
    // nothing to lock, and this only moves the session's generation on, so
    // that an unlock still deriving its key lands after it and is refused as
    // stale rather than opening a vault behind a window that has gone.
    app.lock(Reason::Closed);
}

/// The page was asked and the window `label` is still there once the grace is
/// over: close it from here. Kept to the label, because a window built since -
/// an idle lock's, or the Dock's - is not the one the reader closed.
///
/// A pause during which something held the session is not counted, and the
/// window gone at any look ends the wait there.
fn overdue<C: Closer>(app: &C, label: &str) {
    let mut left = GRACE;
    while left > 0 {
        app.pause();
        if !app.up(label) {
            return;
        }
        if !app.busy() {
            left -= 1;
        }
    }
    lock(app);
    app.destroy(label);
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::thread::ThreadId;
    use std::time::Instant;

    use vault_core::LockPolicy;
    use vault_core::kdf::Work;
    use zeroize::Zeroizing;

    use super::*;
    use crate::source::shipped;

    /// What the application was asked to do, in order, and on which thread.
    #[derive(Clone, Debug, PartialEq)]
    enum Did {
        Posted(Event),
        Locked(Reason),
        Destroyed(String),
    }

    /// The application, as a close sees it: a page that is or is not
    /// listening, the windows that are up, a session the lock really locks,
    /// and a grace whose pauses the test hands over one by one.
    #[derive(Clone)]
    struct Fake {
        listening: bool,
        windows: Arc<Mutex<Vec<String>>>,
        did: Arc<Mutex<Vec<(Did, ThreadId)>>>,
        session: Arc<Session>,
        /// The grace asking for its next pause. A test waits for the ask
        /// before it hands one over, so the grace never runs ahead of the
        /// test or behind a timer - and a grace that stopped asking fails the
        /// test rather than hanging it.
        asks: Sender<()>,
        asked: Arc<Mutex<Receiver<()>>>,
        /// Each pause, carrying whether something held the session through it.
        pauses: Arc<Mutex<Receiver<bool>>>,
        hand: Sender<bool>,
        /// Whether the session was held through the pause that passed last.
        held: Arc<AtomicBool>,
        /// One for this handle and one for every copy a thread is holding:
        /// back to one when every thread a close started has finished.
        alive: Arc<()>,
    }

    /// A pause during which a save held the session.
    const HELD: bool = true;
    /// A pause during which nothing did.
    const FREE: bool = false;

    impl Fake {
        fn new(listening: bool, windows: &[&str]) -> Fake {
            let (asks, asked) = channel();
            let (hand, pauses) = channel();
            Fake {
                listening,
                windows: Arc::new(Mutex::new(
                    windows.iter().map(|label| (*label).to_owned()).collect(),
                )),
                did: Arc::new(Mutex::new(Vec::new())),
                session: Arc::new(Session::new(None, None)),
                asks,
                asked: Arc::new(Mutex::new(asked)),
                pauses: Arc::new(Mutex::new(pauses)),
                hand,
                held: Arc::new(AtomicBool::new(false)),
                alive: Arc::new(()),
            }
        }

        fn did(&self) -> Vec<Did> {
            self.record().into_iter().map(|(did, _)| did).collect()
        }

        fn record(&self) -> Vec<(Did, ThreadId)> {
            self.did.lock().expect("the record is there").clone()
        }

        fn write(&self, did: Did) {
            self.did
                .lock()
                .expect("the record is there")
                .push((did, std::thread::current().id()));
        }

        /// The window `label` went, the way the page's own `close_window`, a
        /// lock or a rebuild takes one.
        fn gone(&self, label: &str) {
            self.windows
                .lock()
                .expect("the windows are there")
                .retain(|up| up != label);
        }

        /// Waits for the grace to ask for its next pause and hands it one,
        /// `held` saying whether something held the session through it.
        fn pass(&self, held: bool) {
            self.asked
                .lock()
                .expect("the asks are there")
                .recv_timeout(Duration::from_secs(10))
                .unwrap_or_else(|_| {
                    panic!("the grace stopped asking, having done {:?}", self.did())
                });
            self.hand.send(held).expect("the grace is there to take it");
        }

        /// Waits until every thread the close started has finished.
        fn settled(&self) {
            let until = Instant::now() + Duration::from_secs(10);
            while Arc::strong_count(&self.alive) > 1 {
                assert!(Instant::now() < until, "a close never finished");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    impl Closer for Fake {
        fn tell(&self) -> bool {
            self.listening
        }

        fn post(&self, event: Event) {
            // The timer locks only what is open, and a session with nothing
            // open has nothing for it: the case a close has to cover itself.
            self.write(Did::Posted(event));
        }

        fn lock(&self, reason: Reason) {
            self.session.lock(reason);
            self.write(Did::Locked(reason));
        }

        fn up(&self, label: &str) -> bool {
            self.windows
                .lock()
                .expect("the windows are there")
                .iter()
                .any(|up| up == label)
        }

        fn destroy(&self, label: &str) {
            self.gone(label);
            self.write(Did::Destroyed(label.to_owned()));
        }

        fn pause(&self) {
            self.asks.send(()).expect("the test is there to answer");
            let held = self
                .pauses
                .lock()
                .expect("the pauses are there")
                .recv()
                .expect("the test hands over every pause");
            self.held.store(held, Ordering::SeqCst);
        }

        fn busy(&self) -> bool {
            self.held.load(Ordering::SeqCst)
        }
    }

    const CLOSED: [Did; 2] = [
        Did::Posted(Event::Locking(Reason::Closed)),
        Did::Locked(Reason::Closed),
    ];

    fn closed(label: &str) -> Vec<Did> {
        let mut closed = CLOSED.to_vec();
        closed.push(Did::Destroyed(label.to_owned()));
        closed
    }

    /// A page that was told and never asked - a script stuck, a page that
    /// crashed - still has its vault locked and its window taken down, once
    /// the grace is over and not a pause before.
    #[test]
    fn a_page_that_never_answers_is_locked_and_closed_after_the_grace() {
        let app = Fake::new(true, &["main"]);

        assert!(
            requested(&app, "main"),
            "the window went before the page was asked"
        );
        for _ in 1..GRACE {
            app.pass(FREE);
        }
        assert!(
            app.did().is_empty(),
            "the close was made before the page had its grace"
        );
        assert!(app.up("main"));

        app.pass(FREE);
        app.settled();
        assert_eq!(app.did(), closed("main"));
        assert!(!app.up("main"));
    }

    /// The page sent what was typed and asked for the close itself, which
    /// locked and took the window. The grace finds nothing left to do at its
    /// next look, and ends there rather than sitting out the rest.
    #[test]
    fn a_page_that_closed_in_time_is_not_locked_twice() {
        let app = Fake::new(true, &["main"]);

        assert!(requested(&app, "main"));
        app.gone("main");
        app.pass(FREE);

        app.settled();
        assert!(
            app.did().is_empty(),
            "the close was made twice: {:?}",
            app.did()
        );
    }

    /// The reader typed during a long save and closed the window. The page's
    /// drafts wait behind the save for the session, and so would a lock from
    /// here, which could take the session first and write the vault without
    /// them. The page is answering, only slowly: the grace does not run while
    /// the session is held, however long that is, and runs in full once it is
    /// free.
    #[test]
    fn the_grace_does_not_run_while_the_page_waits_behind_a_save() {
        let app = Fake::new(true, &["main"]);

        assert!(requested(&app, "main"));
        for _ in 0..GRACE * 3 {
            app.pass(HELD);
        }
        assert!(
            app.did().is_empty(),
            "the window was closed from here while its page waited behind a save"
        );
        assert!(app.up("main"));

        for _ in 1..GRACE {
            app.pass(FREE);
        }
        assert!(
            app.did().is_empty(),
            "the time behind the save was counted against the page"
        );

        app.pass(FREE);
        app.settled();
        assert_eq!(app.did(), closed("main"));
    }

    /// No page to ask - it never loaded, or it went - so the window goes at
    /// once. The lock writes the vault out first, which is a key derivation,
    /// and AppKit's thread must not be the one that waits for it.
    #[test]
    fn with_no_page_the_close_locks_off_the_calling_thread_and_lets_the_window_go() {
        let app = Fake::new(false, &["main"]);

        assert!(
            !requested(&app, "main"),
            "the window was held up with no page"
        );

        app.settled();
        let here = std::thread::current().id();
        let record = app.record();
        assert_eq!(
            record
                .iter()
                .map(|(did, _)| did.clone())
                .collect::<Vec<_>>(),
            CLOSED
        );
        assert!(
            record.iter().all(|(_, thread)| *thread != here),
            "the lock ran on the thread that asked for the close"
        );
        assert!(
            !record
                .iter()
                .any(|(did, _)| matches!(did, Did::Destroyed(_))),
            "AppKit takes this window down itself"
        );
    }

    /// Cmd+W while the password is still deriving. The timer has nothing open
    /// to lock, so a close that only went through it would leave the unlock to
    /// land a moment later: a decrypted vault behind a window that has gone,
    /// with its lock file beside the database.
    #[test]
    fn a_close_with_nothing_open_still_makes_a_landing_unlock_stale() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let target = directory.path().join("slow.kdbx");
        let password = || Zeroizing::new(b"correct horse battery staple".to_vec());

        let app = Fake::new(false, &[]);
        let session = Arc::clone(&app.session);
        // Heavy enough that the unlock is still deriving when the close
        // arrives, as in `session.rs`.
        session.making(target.clone());
        session.measured(Work::at(40));
        session.create(password()).expect("the vault is made");
        assert!(session.lock(Reason::ByHand));

        let unlocking = {
            let session = Arc::clone(&session);
            std::thread::spawn(move || session.unlock(password(), LockPolicy::Respect))
        };
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            !session.is_unlocked(),
            "the unlock had already finished, so this proved nothing"
        );
        lock(&app);
        let landed = unlocking.join().expect("the unlocking thread finishes");

        assert_eq!(app.did(), CLOSED);
        assert!(
            landed.is_err(),
            "a vault opened behind a window that had gone"
        );
        assert!(!session.is_unlocked());
        assert!(
            vault_core::storage::lock::inspect(&target)
                .expect("the lock file reads")
                .is_none(),
            "the vault nobody is asking about kept its lock file"
        );
    }

    /// The run callback is where the close and the exit arrive, and it cannot
    /// run in a test. Read from the source: the window is held up only for a
    /// page that was asked, and the loop is kept running only when the last
    /// window went - never when Coffer asked to exit with a code, which is how
    /// `app.exit` and a restart end it.
    #[test]
    fn the_run_callback_holds_a_window_up_only_for_a_page_that_was_asked() {
        let run: String = shipped(include_str!("lib.rs"))
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        assert_eq!(run.matches("prevent_close(").count(), 1);
        assert!(
            run.contains("if closing::requested(app, label) { api.prevent_close(); }"),
            "the window is held up whether or not a page was asked"
        );

        assert_eq!(run.matches("ExitRequested").count(), 1);
        assert_eq!(run.matches("prevent_exit(").count(), 1);
        assert!(
            run.contains(
                "tauri::RunEvent::ExitRequested { code: None, ref api, .. } => api.prevent_exit(),"
            ),
            "an exit Coffer asked for with a code is refused"
        );

        assert!(
            run.contains("tauri::RunEvent::Reopen { .. } => window::bring_back(app),"),
            "a click on the Dock icon no longer brings the window back"
        );
    }
}
