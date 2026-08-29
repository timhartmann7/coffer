//! The one thread that keeps the deadline.
//!
//! One thread for the life of the process, parked on a condition variable. The
//! clipboard's idiom - a thread per copy, sleeping - is safe there because a
//! copy happens once and a late timer is neutralised by the change count. An
//! idle deadline is reset on every keystroke, and the same shape would be a
//! thread per keystroke.
//!
//! There is no timer in Tauri's async runtime to fall back on: it enables tokio
//! with `rt`, `rt-multi-thread`, `sync`, `fs` and `io-util`, and not `time`, so
//! `sleep`, `interval` and `timeout` are not compiled at all.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use super::{Deadline, Decision, Event, Moment, Reason};

/// The deadline, and the thread watching it.
pub struct Timer {
    shared: Arc<Shared>,
    watcher: Option<JoinHandle<()>>,
}

/// What the watcher and the window both reach.
///
/// Held apart from [`Timer`] on purpose. If the thread held the timer itself,
/// the timer would be keeping its own watcher alive and the destructor that
/// stops it could never run.
struct Shared {
    held: Mutex<Held>,
    /// What a decision to lock reaches. A closure rather than a one-method
    /// trait: the only two implementations such a trait could have are the real
    /// one and a test recorder, and `Fn` already is that trait.
    fire: Box<dyn Fn(Reason) + Send + Sync>,
    woken: Condvar,
    stopping: AtomicBool,
}

struct Held {
    deadline: Deadline,
    /// How long the watcher should wait, or nothing when there is nothing to
    /// wait for.
    waiting: Option<Duration>,
}

impl Timer {
    pub fn start(deadline: Deadline, fire: impl Fn(Reason) + Send + Sync + 'static) -> Timer {
        let shared = Arc::new(Shared {
            held: Mutex::new(Held {
                deadline,
                waiting: None,
            }),
            fire: Box::new(fire),
            woken: Condvar::new(),
            stopping: AtomicBool::new(false),
        });

        let watching = Arc::clone(&shared);
        Timer {
            shared,
            watcher: Some(std::thread::spawn(move || watching.watch())),
        }
    }

    /// Tells the deadline what happened, and locks if that is the answer.
    pub fn post(&self, event: Event) {
        self.shared.post(event);
    }

    /// How long the open vault has left, for the bar in the status line.
    pub fn left(&self) -> Option<Duration> {
        self.shared.state().deadline.left(Moment::now())
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::Release);
        self.shared.woken.notify_all();
        if let Some(watcher) = self.watcher.take() {
            let _ = watcher.join();
        }
    }
}

impl Shared {
    /// The lock runs on the calling thread, with the state released.
    ///
    /// That is load-bearing in both directions. A notification block posting
    /// `Machine(Sleeping)` needs the tree wiped before the Mac suspends rather
    /// than whenever a watcher thread gets round to it; and a lock that says the
    /// reader is there, or locks again, must not find a mutex its own caller is
    /// holding.
    fn post(&self, event: Event) {
        let decision = {
            let mut held = self.state();
            let decision = held.deadline.on(event, Moment::now());
            held.waiting = match decision {
                Decision::WaitFor(left) => Some(left),
                Decision::Nothing | Decision::Lock(_) => None,
            };
            decision
        };

        self.woken.notify_one();
        if let Decision::Lock(reason) = decision {
            (self.fire)(reason);
        }
    }

    fn state(&self) -> MutexGuard<'_, Held> {
        // A poisoned lock means a panic happened while the deadline was
        // borrowed. Nothing here leaves it half-written, and the alternative is
        // a vault that stops locking itself.
        self.held.lock().unwrap_or_else(|it| it.into_inner())
    }

    fn watch(&self) {
        loop {
            // The state is taken before the stop flag is read and is still held
            // when the wait begins, so a message that arrives in between cannot
            // be signalled into a thread that has not started listening yet.
            let held = self.state();
            if self.stopping.load(Ordering::Acquire) {
                return;
            }

            let elapsed = match held.waiting {
                // Nothing is open. Park until something happens, which costs
                // nothing and wakes on the first unlock.
                None => {
                    let held = self
                        .woken
                        .wait(held)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    drop(held);
                    false
                }
                Some(left) => {
                    let (held, outcome) = self
                        .woken
                        .wait_timeout(held, left)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    drop(held);
                    outcome.timed_out()
                }
            };

            // Whether the wait really ran out is not this thread's to decide. A
            // condition variable wakes up on its own, and the deadline may have
            // moved while it slept, so the question is asked rather than
            // assumed - which is also what makes a wait left over from a vault
            // that has already gone do nothing at all.
            if elapsed {
                self.post(Event::Elapsed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    /// What the timer locked, and how often.
    #[derive(Default)]
    struct Locks {
        count: AtomicUsize,
        last: Mutex<Option<Reason>>,
    }

    impl Locks {
        fn record(self: &Arc<Locks>) -> impl Fn(Reason) + Send + Sync + 'static {
            let held = Arc::clone(self);
            move |reason| {
                *held.last.lock().unwrap_or_else(|it| it.into_inner()) = Some(reason);
                held.count.fetch_add(1, Ordering::Release);
            }
        }

        fn count(&self) -> usize {
            self.count.load(Ordering::Acquire)
        }

        fn last(&self) -> Option<Reason> {
            *self.last.lock().unwrap_or_else(|it| it.into_inner())
        }
    }

    /// Every wait in this file is milliseconds. The suite's leak timeout is half
    /// a second, so a test that waited for a real five-minute default would be
    /// reported as leaking rather than as slow.
    const SOON: Duration = Duration::from_millis(20);

    fn wait_for(what: &str, condition: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !condition() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(condition(), "{what}");
    }

    /// How many threads this process is running, so that a hundred timers can be
    /// shown to leave none behind.
    fn threads() -> usize {
        // SAFETY: asking the kernel for a count of this task's threads. The
        // structure is filled in by the kernel and read once.
        unsafe {
            let mut info: libc::proc_taskinfo = std::mem::zeroed();
            let size = size_of::<libc::proc_taskinfo>() as i32;
            let read = libc::proc_pidinfo(
                libc::getpid(),
                libc::PROC_PIDTASKINFO,
                0,
                std::ptr::from_mut(&mut info).cast(),
                size,
            );
            if read == size {
                info.pti_threadnum as usize
            } else {
                0
            }
        }
    }

    #[test]
    fn a_deadline_that_runs_out_locks_once_and_only_once() {
        let locks = Arc::new(Locks::default());
        let timer = Timer::start(Deadline::new(SOON), locks.record());

        timer.post(Event::Unlocked);
        wait_for("the deadline never ran out", || locks.count() == 1);
        assert_eq!(locks.last(), Some(Reason::Idle));

        // The watcher goes back to waiting on nothing rather than locking again.
        std::thread::sleep(SOON * 5);
        assert_eq!(locks.count(), 1);
        assert_eq!(timer.left(), None);
    }

    /// The reader is there, from every thread at once. One deadline moves, and
    /// nothing locks.
    #[test]
    fn many_threads_saying_the_reader_is_there_move_one_deadline() {
        let locks = Arc::new(Locks::default());
        let timer = Timer::start(Deadline::new(Duration::from_secs(60)), locks.record());
        timer.post(Event::Unlocked);

        std::thread::scope(|scope| {
            for _ in 0..10 {
                scope.spawn(|| {
                    for _ in 0..200 {
                        timer.post(Event::Stirred);
                    }
                });
            }
        });

        assert_eq!(locks.count(), 0);
        assert!(
            timer
                .left()
                .is_some_and(|left| left > Duration::from_secs(59))
        );
    }

    /// A hundred vaults opened and locked. The process ends with the threads it
    /// started with, which is what fails without the condition variable and the
    /// join.
    #[test]
    fn a_hundred_timers_leave_no_threads_behind() {
        let before = threads();
        assert!(before > 0, "this platform does not count its own threads");

        let locks = Arc::new(Locks::default());
        for _ in 0..100 {
            let timer = Timer::start(Deadline::new(Duration::from_secs(60)), locks.record());
            timer.post(Event::Unlocked);
            timer.post(Event::Locking(Reason::ScreenLocked));
        }

        assert_eq!(locks.count(), 100);
        wait_for("a hundred timers left threads behind", || {
            threads() <= before
        });
    }

    /// A timer dropped with a deadline still pending never fires afterwards.
    #[test]
    fn a_timer_that_is_dropped_stops_locking() {
        let locks = Arc::new(Locks::default());
        {
            let timer = Timer::start(Deadline::new(SOON * 10), locks.record());
            timer.post(Event::Unlocked);
        }

        std::thread::sleep(SOON * 20);
        assert_eq!(locks.count(), 0);
    }

    /// The lock runs with the state released, so a slow one does not stop the
    /// window telling the timer that the reader is still there.
    #[test]
    fn a_slow_lock_does_not_hold_up_the_next_message() {
        let timer = Arc::new(Timer::start(Deadline::new(Duration::from_secs(60)), |_| {
            std::thread::sleep(SOON * 5)
        }));
        timer.post(Event::Unlocked);

        let held = Arc::clone(&timer);
        let locking = std::thread::spawn(move || held.post(Event::Locking(Reason::Sleeping)));
        wait_for("the slow lock never started", || {
            timer.shared.state().waiting.is_none()
        });

        // While that one is inside its slow lock, this one has to get through.
        let started = std::time::Instant::now();
        timer.post(Event::Stirred);
        assert!(started.elapsed() < SOON * 4);

        locking.join().expect("the locking thread finishes");
    }

    /// Locking is allowed to say the reader is there, or to lock again. Neither
    /// may deadlock, because the state is not held while the lock runs.
    #[test]
    fn a_lock_that_talks_back_to_the_timer_does_not_deadlock() {
        let locks = Arc::new(Locks::default());
        let talkative = Arc::new(Mutex::new(None::<Arc<Timer>>));

        let recording = locks.record();
        let held = Arc::clone(&talkative);
        let timer = Arc::new(Timer::start(
            Deadline::new(Duration::from_secs(60)),
            move |reason| {
                recording(reason);
                if let Some(timer) = held.lock().unwrap_or_else(|it| it.into_inner()).as_ref() {
                    timer.post(Event::Stirred);
                }
            },
        ));
        *talkative.lock().unwrap_or_else(|it| it.into_inner()) = Some(Arc::clone(&timer));

        timer.post(Event::Unlocked);
        timer.post(Event::Locking(Reason::Sleeping));
        assert_eq!(locks.count(), 1);

        // The cycle the test made has to go, or the timer outlives the test and
        // nextest reports it as a leak.
        *talkative.lock().unwrap_or_else(|it| it.into_inner()) = None;
    }
}
