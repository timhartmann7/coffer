//! When an open vault is due to lock.
//!
//! Arithmetic, and nothing else: no clock of its own, no thread, no window.
//! Everything that makes locking hard to get right - a timer that fires after
//! the vault has already gone, two triggers arriving at once, a laptop that was
//! shut for eight hours - is answered here, where it can be written as a test
//! rather than as a race somebody hopes not to lose.

pub mod timer;
pub mod watch;

use std::time::{Duration, Instant, SystemTime};

use crate::settings::Settings;

/// Why a vault would be locked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Nobody has touched the window for as long as they said.
    Idle,
    /// The Mac is going to sleep.
    Sleeping,
    /// The screen locked.
    ScreenLocked,
    /// Somebody else logged in on this Mac.
    SessionSwitched,
    /// The reader pressed the button.
    ByHand,
    /// The reader closed the window. Like quitting, the window does not come
    /// back on its own: Coffer waits in the Dock until it is clicked.
    Closed,
    /// Coffer is closing.
    Quitting,
}

impl Reason {
    /// Whether the reader asked for a lock of this kind.
    ///
    /// Quitting is not a preference and neither is the button: the lock file
    /// beside the database has to go on the way out whatever anybody chose, and
    /// a button that did nothing would be worse than no button. Closing the
    /// window is neither: a vault left open by a window that went would be a
    /// decrypted vault in a process nobody can see.
    pub fn wanted(self, settings: Settings) -> bool {
        match self {
            Reason::Sleeping => settings.lock_on_sleep,
            Reason::ScreenLocked | Reason::SessionSwitched => settings.lock_on_screen_lock,
            Reason::Idle | Reason::ByHand | Reason::Closed | Reason::Quitting => true,
        }
    }

    /// What the rebuilt window is told, so that it can say why it is asking for
    /// a password again.
    ///
    /// A lock the reader asked for has nothing to explain, so it is not here.
    pub fn explained(self) -> Option<&'static str> {
        match self {
            Reason::Idle => Some("idle"),
            Reason::Sleeping => Some("sleeping"),
            Reason::ScreenLocked => Some("screenLocked"),
            Reason::SessionSwitched => Some("sessionSwitched"),
            Reason::ByHand | Reason::Closed | Reason::Quitting => None,
        }
    }

    /// Whether the window is built again after this lock, asking for the
    /// password. Not when Coffer is quitting, and not when the reader closed
    /// it: a window that came straight back from its own close button would be
    /// one Coffer could never put away. The Dock brings that one back.
    pub fn comes_back(self) -> bool {
        !matches!(self, Reason::Closed | Reason::Quitting)
    }
}

/// Something that happened, from the reader or from the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A vault was opened. The clock starts here and nowhere else.
    Unlocked,
    /// The reader is there, which starts the clock again - unless it had
    /// already run out.
    ///
    /// A Mac that slept through the deadline wakes with the watcher still
    /// waiting on a clock that stood still while it slept, and the first key
    /// pressed after that gets here before the watcher does. Taken as a reason
    /// to start again, it would hand a fresh timeout to whoever sat down at the
    /// machine; taken for what it is, it is the first thing to find the vault
    /// overdue, and it locks it.
    Stirred,
    /// The wait ran out. Whether that means anything is this module's answer,
    /// not the timer's: a condition variable wakes up on its own.
    Elapsed,
    /// A lock has been asked for: by the machine, or by the reader pressing the
    /// button. Whether the reader wanted a lock of this kind is settled before
    /// the event is posted, so that the deadline knows nothing about settings.
    Locking(Reason),
    /// The reader chose a different timeout.
    TimeoutChanged(Duration),
}

/// What to do about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Nothing is open, so nothing is due.
    Nothing,
    /// Come back in this long, or sooner if something happens.
    WaitFor(Duration),
    Lock(Reason),
}

/// A moment, as both of this machine's clocks saw it.
///
/// `Instant` on Darwin does not advance while the Mac is asleep, so a laptop
/// closed for eight hours wakes up believing four minutes have passed and an
/// idle timer alone would not fire. `SystemTime` does advance, and can also be
/// stepped by anything that sets the clock. Counting with both and believing
/// whichever says more time has gone closes the first without trusting the
/// second, and needs no notification when the machine wakes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moment {
    steady: Instant,
    wall: SystemTime,
}

impl Moment {
    pub fn now() -> Moment {
        Moment {
            steady: Instant::now(),
            wall: SystemTime::now(),
        }
    }

    /// How long has passed since an earlier moment.
    ///
    /// A wall clock that went backwards contributes nothing rather than a
    /// negative, so setting the clock back can never make a vault look younger
    /// than the steady clock says it is.
    pub fn since(self, earlier: Moment) -> Duration {
        let steady = self.steady.saturating_duration_since(earlier.steady);
        let wall = self
            .wall
            .duration_since(earlier.wall)
            .unwrap_or(Duration::ZERO);
        steady.max(wall)
    }
}

/// When the open vault is due to lock.
pub struct Deadline {
    after: Duration,
    /// When the clock was last restarted, and `None` when nothing is open.
    ///
    /// This one field is what makes a stale timer impossible rather than
    /// unlikely. A wait that ends after the vault has already gone finds
    /// nothing to lock, so there is no generation counter anywhere in this
    /// slice and nothing to keep in step with one.
    since: Option<Moment>,
}

impl Deadline {
    pub fn new(after: Duration) -> Deadline {
        Deadline { after, since: None }
    }

    /// What to do about something that happened.
    pub fn on(&mut self, event: Event, now: Moment) -> Decision {
        if let Event::TimeoutChanged(after) = event {
            self.after = after;
        }

        match event {
            Event::Unlocked => {
                self.since = Some(now);
                Decision::WaitFor(self.after)
            }
            Event::Locking(reason) => match self.since {
                None => Decision::Nothing,
                Some(_) => {
                    self.since = None;
                    Decision::Lock(reason)
                }
            },
            // A stir is asked the watcher's own question before it may start
            // the clock again. After a night shut, whichever of the two gets
            // here first is the one that finds the time spent.
            Event::Stirred | Event::Elapsed | Event::TimeoutChanged(_) => match self.left(now) {
                None => Decision::Nothing,
                Some(left) if left.is_zero() => {
                    self.since = None;
                    Decision::Lock(Reason::Idle)
                }
                Some(_) if event == Event::Stirred => {
                    self.since = Some(now);
                    Decision::WaitFor(self.after)
                }
                Some(left) => Decision::WaitFor(left),
            },
        }
    }

    /// How long the open vault has left, and nothing when none is open.
    pub fn left(&self, now: Moment) -> Option<Duration> {
        let since = self.since?;
        Some(self.after.saturating_sub(now.since(since)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    /// A clock the test moves by hand. `Moment` reads two real clocks, so the
    /// only way to sit at a chosen point on both is to take a real reading and
    /// then say how much later this one is.
    fn later(base: Moment, by: Duration) -> Moment {
        Moment {
            steady: base.steady + by,
            wall: base.wall + by,
        }
    }

    fn armed() -> (Deadline, Moment) {
        let mut deadline = Deadline::new(MINUTE);
        let start = Moment::now();
        assert_eq!(
            deadline.on(Event::Unlocked, start),
            Decision::WaitFor(MINUTE)
        );
        (deadline, start)
    }

    #[test]
    fn nothing_is_due_while_nothing_is_open() {
        let mut deadline = Deadline::new(MINUTE);
        let now = Moment::now();

        for event in [
            Event::Stirred,
            Event::Elapsed,
            Event::Locking(Reason::Sleeping),
            Event::TimeoutChanged(MINUTE),
        ] {
            assert_eq!(deadline.on(event, now), Decision::Nothing, "{event:?}");
        }
        assert_eq!(deadline.left(now), None);
    }

    /// The stale timer, said directly. A wait that ends after the vault has
    /// gone has nothing to lock, whatever it was waiting for.
    #[test]
    fn a_wait_that_ends_after_the_lock_locks_nothing() {
        let (mut deadline, start) = armed();

        assert_eq!(
            deadline.on(Event::Locking(Reason::Sleeping), later(start, MINUTE / 2)),
            Decision::Lock(Reason::Sleeping)
        );
        assert_eq!(
            deadline.on(Event::Elapsed, later(start, MINUTE * 2)),
            Decision::Nothing
        );
        assert_eq!(deadline.left(later(start, MINUTE * 2)), None);
    }

    /// A condition variable wakes up on its own, and a wait that came back
    /// early has to go back to waiting rather than lock a vault somebody is
    /// still looking at.
    #[test]
    fn a_wait_that_ends_early_goes_back_to_waiting() {
        let (mut deadline, start) = armed();

        assert_eq!(
            deadline.on(Event::Elapsed, later(start, Duration::from_secs(20))),
            Decision::WaitFor(Duration::from_secs(40))
        );
        assert_eq!(
            deadline.on(Event::Elapsed, later(start, MINUTE)),
            Decision::Lock(Reason::Idle)
        );
    }

    /// Two triggers arriving together give one lock, because the first clears
    /// the deadline the second would have read.
    #[test]
    fn two_triggers_at_once_lock_once() {
        let (mut deadline, start) = armed();
        let now = later(start, MINUTE / 2);

        assert_eq!(
            deadline.on(Event::Locking(Reason::Sleeping), now),
            Decision::Lock(Reason::Sleeping)
        );
        assert_eq!(
            deadline.on(Event::Locking(Reason::ScreenLocked), now),
            Decision::Nothing
        );

        let (mut deadline, start) = armed();
        let due = later(start, MINUTE);
        assert_eq!(
            deadline.on(Event::Elapsed, due),
            Decision::Lock(Reason::Idle)
        );
        assert_eq!(
            deadline.on(Event::Locking(Reason::Sleeping), due),
            Decision::Nothing
        );
    }

    /// The reader pressing keys moves a deadline. It never makes one, and it
    /// never locks one that still has time on it.
    #[test]
    fn ten_thousand_stirs_move_one_deadline_and_lock_nothing() {
        let (mut deadline, start) = armed();

        for step in 0..10_000u32 {
            let now = later(start, Duration::from_millis(u64::from(step)));
            assert_eq!(deadline.on(Event::Stirred, now), Decision::WaitFor(MINUTE));
        }

        let last = later(start, Duration::from_millis(9_999));
        assert_eq!(deadline.left(last), Some(MINUTE));
        assert_eq!(deadline.on(Event::Elapsed, last), Decision::WaitFor(MINUTE));
    }

    /// A hundred locks and unlocks. Each one arms exactly once and locks
    /// exactly once, and the wait left over from each is inert.
    #[test]
    fn a_hundred_cycles_arm_once_and_lock_once_each() {
        let mut deadline = Deadline::new(MINUTE);
        let start = Moment::now();

        for round in 0..100u32 {
            let now = later(start, MINUTE * round);
            assert_eq!(deadline.on(Event::Unlocked, now), Decision::WaitFor(MINUTE));
            assert_eq!(
                deadline.on(Event::Locking(Reason::ScreenLocked), now),
                Decision::Lock(Reason::ScreenLocked)
            );
            assert_eq!(deadline.on(Event::Elapsed, now), Decision::Nothing);
            assert_eq!(deadline.left(now), None);
        }
    }

    /// A laptop shut for eight hours. The steady clock stops while the Mac is
    /// asleep; the wall clock does not, and it is the one that says the reader
    /// has been gone all night.
    #[test]
    fn a_machine_that_slept_locks_on_the_clock_that_kept_running() {
        let mut deadline = Deadline::new(MINUTE);
        let start = Moment::now();
        deadline.on(Event::Unlocked, start);

        let woken = Moment {
            steady: start.steady + Duration::from_secs(4),
            wall: start.wall + Duration::from_secs(8 * 3600),
        };
        assert_eq!(deadline.left(woken), Some(Duration::ZERO));
        assert_eq!(
            deadline.on(Event::Elapsed, woken),
            Decision::Lock(Reason::Idle)
        );
    }

    /// The same night, with the reader's first key getting there before the
    /// watcher does. The watcher's wait is counted on the clock that stood
    /// still, so it has not come back; a stir that restarted the clock here
    /// handed the vault a fresh timeout at the moment somebody sat down at it.
    #[test]
    fn a_stir_that_arrives_first_after_a_long_sleep_locks() {
        let mut deadline = Deadline::new(MINUTE);
        let start = Moment::now();
        deadline.on(Event::Unlocked, start);

        let woken = Moment {
            steady: start.steady + Duration::from_secs(4),
            wall: start.wall + Duration::from_secs(3600),
        };
        assert_eq!(
            deadline.on(Event::Stirred, woken),
            Decision::Lock(Reason::Idle)
        );

        // The watcher coming round afterwards, and the next key after that,
        // find nothing left to lock.
        assert_eq!(deadline.on(Event::Elapsed, woken), Decision::Nothing);
        assert_eq!(deadline.on(Event::Stirred, woken), Decision::Nothing);
        assert_eq!(deadline.left(woken), None);
    }

    /// Exactly on the deadline is due for a stir too, on the same terms as the
    /// watcher's wake-up: one comparison, not two that could disagree.
    #[test]
    fn a_stir_exactly_at_the_deadline_locks() {
        let (mut deadline, start) = armed();

        assert_eq!(
            deadline.on(Event::Stirred, later(start, MINUTE)),
            Decision::Lock(Reason::Idle)
        );
        assert_eq!(deadline.left(later(start, MINUTE)), None);
    }

    /// A millisecond before it, the reader got there in time, and the whole
    /// timeout starts again from their key rather than from the unlock.
    #[test]
    fn a_stir_just_before_the_deadline_starts_it_again() {
        let (mut deadline, start) = armed();
        let just = later(start, MINUTE - Duration::from_millis(1));

        assert_eq!(deadline.on(Event::Stirred, just), Decision::WaitFor(MINUTE));
        assert_eq!(deadline.left(just), Some(MINUTE));

        // Half a minute past the original deadline is half a minute and a
        // millisecond into the new one.
        let after = later(start, MINUTE + MINUTE / 2);
        assert_eq!(
            deadline.on(Event::Elapsed, after),
            Decision::WaitFor(MINUTE / 2 - Duration::from_millis(1))
        );
    }

    /// Only the clock that moved furthest counts, for a stir as for the watcher.
    /// A wall clock stepped back an hour is not a night asleep.
    #[test]
    fn a_stir_after_the_clock_was_set_back_is_only_a_stir() {
        let (mut deadline, start) = armed();
        let stepped = Moment {
            steady: start.steady + Duration::from_secs(30),
            wall: start.wall - Duration::from_secs(3600),
        };

        assert_eq!(
            deadline.on(Event::Stirred, stepped),
            Decision::WaitFor(MINUTE)
        );
    }

    /// A wall clock stepped backwards is not a reason to lock early, and not a
    /// reason to stop counting either.
    #[test]
    fn a_clock_set_backwards_neither_locks_early_nor_stops_the_count() {
        let mut deadline = Deadline::new(MINUTE);
        let start = Moment::now();
        deadline.on(Event::Unlocked, start);

        let stepped = Moment {
            steady: start.steady + Duration::from_secs(30),
            wall: start.wall - Duration::from_secs(3600),
        };
        assert_eq!(deadline.left(stepped), Some(Duration::from_secs(30)));
        assert_eq!(
            deadline.on(Event::Elapsed, stepped),
            Decision::WaitFor(Duration::from_secs(30))
        );
    }

    /// A clock that does not move at all never runs anything out.
    #[test]
    fn a_clock_that_never_advances_never_locks() {
        let (mut deadline, start) = armed();

        for _ in 0..1_000 {
            assert_eq!(
                deadline.on(Event::Elapsed, start),
                Decision::WaitFor(MINUTE)
            );
        }
    }

    /// Exactly on the deadline is due. A strict comparison here would leave a
    /// vault open for one more round of whatever the timer's resolution is.
    #[test]
    fn the_moment_the_time_is_up_is_up() {
        let (mut deadline, start) = armed();
        let due = later(start, MINUTE);

        assert_eq!(deadline.left(due), Some(Duration::ZERO));
        assert_eq!(
            deadline.on(Event::Elapsed, due),
            Decision::Lock(Reason::Idle)
        );
    }

    /// Shortening the timeout below what has already gone locks now. Lengthening
    /// it does not, and neither does changing it while nothing is open - but the
    /// next unlock has to get the new one.
    #[test]
    fn a_changed_timeout_takes_effect_on_the_vault_that_is_open() {
        let (mut deadline, start) = armed();
        let now = later(start, Duration::from_secs(30));

        assert_eq!(
            deadline.on(Event::TimeoutChanged(MINUTE * 5), now),
            Decision::WaitFor(Duration::from_secs(4 * 60 + 30))
        );
        assert_eq!(
            deadline.on(Event::TimeoutChanged(Duration::from_secs(10)), now),
            Decision::Lock(Reason::Idle)
        );

        assert_eq!(
            deadline.on(Event::TimeoutChanged(MINUTE), now),
            Decision::Nothing
        );
        assert_eq!(deadline.on(Event::Unlocked, now), Decision::WaitFor(MINUTE));
    }

    /// How long is left is never more than the whole timeout, whatever the
    /// clocks did.
    #[test]
    fn what_is_left_never_exceeds_the_whole_timeout() {
        let (deadline, start) = armed();

        for step in [0, 1, 30, 59, 60, 61, 10_000] {
            let left = deadline
                .left(later(start, Duration::from_secs(step)))
                .expect("a vault is open");
            assert!(left <= MINUTE, "{step}s left {left:?}");
        }
    }

    #[test]
    fn the_reader_decides_which_machine_triggers_count() {
        let both = Settings::default();
        let neither = Settings {
            lock_on_sleep: false,
            lock_on_screen_lock: false,
            ..Settings::default()
        };

        assert!(Reason::Sleeping.wanted(both));
        assert!(Reason::ScreenLocked.wanted(both));
        assert!(Reason::SessionSwitched.wanted(both));

        assert!(!Reason::Sleeping.wanted(neither));
        assert!(!Reason::ScreenLocked.wanted(neither));
        assert!(!Reason::SessionSwitched.wanted(neither));

        // Never a choice: the button has to work, the lock file beside the
        // database has to go on the way out, and a window that went must not
        // leave its vault open behind it.
        assert!(Reason::Idle.wanted(neither));
        assert!(Reason::ByHand.wanted(neither));
        assert!(Reason::Closed.wanted(neither));
        assert!(Reason::Quitting.wanted(neither));
    }

    /// A lock the reader asked for has nothing to explain, and a screen that
    /// said "you were away" after they pressed the button would be wrong.
    #[test]
    fn only_a_lock_the_reader_did_not_ask_for_has_something_to_say() {
        assert_eq!(Reason::Idle.explained(), Some("idle"));
        assert_eq!(Reason::Sleeping.explained(), Some("sleeping"));
        assert_eq!(Reason::ScreenLocked.explained(), Some("screenLocked"));
        assert_eq!(Reason::SessionSwitched.explained(), Some("sessionSwitched"));
        assert_eq!(Reason::ByHand.explained(), None);
        assert_eq!(Reason::Closed.explained(), None);
        assert_eq!(Reason::Quitting.explained(), None);
    }

    /// The close button that brought its own window straight back is the
    /// window Coffer could never put away; the lock that did not bring it back
    /// is a reader locked out until they relaunch.
    #[test]
    fn a_window_comes_back_after_every_lock_but_a_close_and_a_quit() {
        for reason in [
            Reason::Idle,
            Reason::Sleeping,
            Reason::ScreenLocked,
            Reason::SessionSwitched,
            Reason::ByHand,
        ] {
            assert!(reason.comes_back(), "{reason:?} left no window to unlock");
        }
        assert!(!Reason::Closed.comes_back(), "the closed window came back");
        assert!(
            !Reason::Quitting.comes_back(),
            "a window was built on the way out"
        );
    }
}
