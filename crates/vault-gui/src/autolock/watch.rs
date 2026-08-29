//! Hearing the Mac say the reader has gone.
//!
//! Two notification centres. The workspace's own carries sleep, power-off and
//! fast user switching, and delivers them inside this process. The distributed
//! one carries the screen lock, which is posted by `loginwindow` and arrives
//! from another process entirely.
//!
//! Neither of them is observed for waking up. A moment is counted on both of
//! this machine's clocks, so the time a laptop spent shut is already in the
//! arithmetic, and re-basing the deadline on wake would hand a full fresh
//! timeout to a Mac that had been closed all night.
//!
//! Display sleep is not observed either. It is a couple of minutes on a default
//! Mac, and a vault that locked on it would be obeying a timer the reader set in
//! System Settings rather than the one they set here.

use std::cell::RefCell;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{
    NSWorkspace, NSWorkspaceSessionDidResignActiveNotification,
    NSWorkspaceWillPowerOffNotification, NSWorkspaceWillSleepNotification,
};
use objc2_foundation::{NSDistributedNotificationCenter, NSNotification, NSNotificationCenter};

use super::Reason;

/// What `loginwindow` posts when the screen locks. There is no constant for it
/// in any framework header: it is a name, published by Apple's own
/// documentation and used by every application that needs to know.
const SCREEN_LOCKED: &str = "com.apple.screenIsLocked";

thread_local! {
    /// What this process is listening to.
    ///
    /// On the main thread and nowhere else, which is where it is made and where
    /// it is dropped. An observer token is an Objective-C reference and is not a
    /// thing to send between threads.
    static LISTENING: RefCell<Option<Watch>> = const { RefCell::new(None) };
}

/// The observers, taken off the notification centres when this is dropped.
///
/// Dropping the token `addObserverForName:object:queue:usingBlock:` hands back
/// does **not** deregister the observer. A token that was merely let go is an
/// observer holding a copy of the block for the life of the process, and
/// registering a second time means the next screen lock fires twice.
pub struct Watch {
    workspace: Retained<NSNotificationCenter>,
    distributed: Retained<NSDistributedNotificationCenter>,
    tokens: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        for token in self.tokens.drain(..) {
            let observer: &AnyObject = (*token).as_ref();
            // SAFETY: the token came from one of these two centres and is
            // removed once. Removing it from the other is harmless: a centre
            // asked to forget an observer it never had does nothing.
            unsafe {
                self.workspace.removeObserver(observer);
                self.distributed.removeObserver(observer);
            }
        }
    }
}

/// Starts listening, and keeps listening until [`unwatch`] is called.
///
/// `say` runs on whichever thread the notification arrived on, which for the
/// workspace centre is the thread that posted it and for the distributed centre
/// is the main run loop. It must therefore be cheap and must not care where it
/// is: what it does is wipe the tree, which is neither.
pub fn watch(say: impl Fn(Reason) + 'static) {
    let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
    let distributed = NSDistributedNotificationCenter::defaultCenter();

    let say = std::rc::Rc::new(say);
    let mut tokens = Vec::new();

    for (name, reason) in [
        // SAFETY: reading AppKit constants declared in an `extern "C"` block.
        (
            unsafe { NSWorkspaceWillSleepNotification },
            Reason::Sleeping,
        ),
        (
            unsafe { NSWorkspaceSessionDidResignActiveNotification },
            Reason::SessionSwitched,
        ),
        (
            unsafe { NSWorkspaceWillPowerOffNotification },
            Reason::Quitting,
        ),
    ] {
        tokens.push(listen(&workspace, name, &say, reason));
    }

    let locked = objc2_foundation::NSString::from_str(SCREEN_LOCKED);
    tokens.push(listen(&distributed, &locked, &say, Reason::ScreenLocked));

    LISTENING.with(|listening| {
        *listening.borrow_mut() = Some(Watch {
            workspace,
            distributed,
            tokens,
        })
    });
}

/// Stops listening. Nothing calls this in the window today; it is what the
/// destructor above is for, and it is how the suite proves an observer really
/// goes.
#[cfg(test)]
pub fn unwatch() {
    LISTENING.with(|listening| *listening.borrow_mut() = None);
}

/// One observer, on either centre.
///
/// The distributed centre is a subclass of the ordinary one, so one function
/// covers both. `object` is always `None`: on the distributed centre that
/// argument is a poster's *name* rather than an object, and passing anything
/// else would be a type confusion across a process boundary.
fn listen(
    centre: &NSNotificationCenter,
    name: &objc2_foundation::NSString,
    say: &std::rc::Rc<impl Fn(Reason) + 'static>,
    reason: Reason,
) -> Retained<ProtocolObject<dyn NSObjectProtocol>> {
    let say = std::rc::Rc::clone(say);
    let block = RcBlock::new(move |_: std::ptr::NonNull<NSNotification>| say(reason));

    // SAFETY: the block outlives the observer - the token owns it - and the
    // token is removed when the `Watch` holding it is dropped.
    unsafe { centre.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block) }
}

/// The suite here registers real observers on the centre this whole process
/// shares, so it needs a process of its own per test. `cargo nextest` gives one;
/// `cargo test` does not, and under it these tests hear each other.
#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use objc2_foundation::NSString;

    use super::*;

    /// Posts a notification into this process, the way the system does.
    ///
    /// The workspace centre delivers synchronously, on the thread that posted,
    /// so this drives the registration, the block and the mapping for real -
    /// with no run loop, no main thread and no display.
    fn post(name: &objc2_foundation::NSNotificationName) {
        let centre = NSWorkspace::sharedWorkspace().notificationCenter();
        // SAFETY: posting a name AppKit declares, with no object and no user
        // information, into the centre this process shares.
        unsafe { centre.postNotificationName_object(name, None) };
    }

    fn heard() -> (Rc<Cell<usize>>, Rc<RefCell<Vec<Reason>>>) {
        (Rc::new(Cell::new(0)), Rc::new(RefCell::new(Vec::new())))
    }

    /// Every notification the workspace centre carries, mapped to the reason it
    /// stands for. A name silently changed upstream shows up here as a lock
    /// that never happens.
    #[test]
    fn the_machine_saying_the_reader_has_gone_says_why() {
        let (count, reasons) = heard();
        let counting = Rc::clone(&count);
        let recording = Rc::clone(&reasons);
        watch(move |reason| {
            counting.set(counting.get() + 1);
            recording.borrow_mut().push(reason);
        });

        for name in [
            // SAFETY: reading AppKit constants declared in an `extern "C"`
            // block.
            unsafe { NSWorkspaceWillSleepNotification },
            unsafe { NSWorkspaceSessionDidResignActiveNotification },
            unsafe { NSWorkspaceWillPowerOffNotification },
        ] {
            post(name);
        }

        assert_eq!(count.get(), 3);
        assert_eq!(
            *reasons.borrow(),
            vec![Reason::Sleeping, Reason::SessionSwitched, Reason::Quitting]
        );

        unwatch();
    }

    /// The observer really goes. Without the removal in `Drop`, letting the
    /// token fall out of scope leaves the block registered for the life of the
    /// process.
    #[test]
    fn a_watch_that_has_been_let_go_hears_nothing() {
        let (count, _) = heard();
        let counting = Rc::clone(&count);
        watch(move |_| counting.set(counting.get() + 1));
        unwatch();

        // SAFETY: as above.
        post(unsafe { NSWorkspaceWillSleepNotification });
        assert_eq!(count.get(), 0);
    }

    /// A hundred locks and unlocks, each of which registers and deregisters.
    /// One notification afterwards has to reach the last one and only the last
    /// one - not a hundred stale blocks as well.
    #[test]
    fn a_hundred_registrations_leave_one_listener() {
        for _ in 0..100 {
            watch(|_| panic!("a watch that was replaced still heard something"));
            unwatch();
        }

        let (count, _) = heard();
        let counting = Rc::clone(&count);
        watch(move |_| counting.set(counting.get() + 1));

        // SAFETY: as above.
        post(unsafe { NSWorkspaceWillSleepNotification });
        assert_eq!(count.get(), 1);

        unwatch();
    }

    /// The screen-lock notification cannot be exercised here: the distributed
    /// centre delivers on a run loop that a test binary does not run, and giving
    /// it a queue to make it testable would also move the real delivery off the
    /// main thread. What can be checked is the name, which is the half that is
    /// easy to get wrong and impossible to notice.
    #[test]
    fn the_screen_lock_is_the_name_loginwindow_posts() {
        assert_eq!(SCREEN_LOCKED, "com.apple.screenIsLocked");

        let centre = NSDistributedNotificationCenter::defaultCenter();
        let name = NSString::from_str(SCREEN_LOCKED);
        let token = listen(
            &centre,
            &name,
            &Rc::new(|_: Reason| {}),
            Reason::ScreenLocked,
        );

        // SAFETY: the token came from this centre and is removed once.
        unsafe { centre.removeObserver((*token).as_ref()) };
    }
}
