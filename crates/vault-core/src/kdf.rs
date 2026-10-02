//! How much work a new database asks for before it will open.
//!
//! Argon2id, sixty-four megabytes, parallelism of four. Only the number of
//! passes is decided here, and it is decided by measurement rather than by a
//! constant: Coffer ships for two architectures, and a count tuned on Apple
//! silicon is five seconds of waiting on an older Intel machine.
//!
//! Nothing on this path touches the reader's password. The measurement derives
//! a key from a constant, because the library that does the deriving zeroizes
//! nothing at all and a calibration run is five to eleven derivations.

use std::time::Duration;

use keepass::config::{DatabaseConfig, KdfConfig};
use keepass::{Database, DatabaseKey};

use crate::preflight;

/// The memory every database Coffer creates asks for. Fixed by the spec and not
/// a dial on any screen: it is the parameter that decides how expensive a
/// guess is, and a database whose owner turned it down is a weaker database
/// with nothing to show for it.
pub const MEMORY_BYTES: u64 = 64 * 1024 * 1024;

/// How many lanes the derivation runs in.
///
/// Fixed, and modest on purpose. The number goes into the file and is a real
/// thread count on every machine that opens it afterwards, so calibrating with
/// whatever core count the creating machine happens to have is a bill the next
/// machine pays.
pub const PARALLELISM: u32 = 4;

/// What a derivation is asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Work {
    pub iterations: u64,
    pub memory: u64,
    pub parallelism: u32,
}

impl Work {
    /// The work a database Coffer creates asks for, at this many passes.
    pub fn at(iterations: u64) -> Work {
        Work {
            iterations,
            memory: MEMORY_BYTES,
            parallelism: PARALLELISM,
        }
    }

    /// The key derivation the file's header will describe.
    pub(crate) fn config(self) -> KdfConfig {
        KdfConfig::Argon2id {
            iterations: self.iterations,
            memory: self.memory,
            parallelism: self.parallelism,
            version: argon2::Version::Version13,
        }
    }
}

/// What the calibration settled on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calibration {
    pub work: Work,
    /// What that work measured, on this machine, just now.
    pub took: Duration,
}

/// One measured derivation.
///
/// A trait because the search is the part worth testing and a real derivation
/// is the part that cannot be: a suite that waits a second a step tells you
/// about the machine it ran on and nothing about the arithmetic.
pub trait Stopwatch {
    fn time(&self, work: Work) -> Duration;
}

/// The real one.
///
/// It times a whole save of an empty database rather than calling Argon2
/// directly. The library builds a nine-field configuration of its own around
/// every derivation - the variant, the version, the lane count, and the
/// megabytes-to-kibibytes conversion the format needs - and restating that here
/// would be a second copy of a rule that could drift silently in either
/// direction. Everything a save does apart from the derivation is under a
/// millisecond against a target of a second.
pub struct Machine;

/// What the measurement derives from. Never the reader's password: `rust-argon2`
/// zeroizes nothing, so every derivation leaves its working memory and its
/// answer in freed heap, and a calibration is a handful of them.
const NOBODYS_PASSWORD: &str = "coffer calibration, not anybody's secret";

impl Stopwatch for Machine {
    fn time(&self, work: Work) -> Duration {
        let mut config = DatabaseConfig::default();
        config.kdf_config = work.config();
        let database = Database::with_config(config);

        let started = std::time::Instant::now();
        let outcome = database.save(
            &mut std::io::sink(),
            DatabaseKey::new().with_password(NOBODYS_PASSWORD),
        );
        let took = started.elapsed();

        // A derivation the library refuses took no time and taught nothing. The
        // search reads that as "immeasurably fast" and walks up from it, and
        // the clamp at the end is what stops it writing something absurd.
        match outcome {
            Ok(()) => took,
            Err(_) => Duration::ZERO,
        }
    }
}

/// What key derivation should take on the machine that made the database,
/// which is what one wrong password costs at the unlock screen.
///
/// Public because the one other place a password is checked - the current one,
/// when the master password is changed - is checked without a derivation, and
/// a wrong guess there is made to cost this instead.
pub const TARGET: Duration = Duration::from_millis(1000);
/// The band the answer has to land in, which is the one in the spec.
const FLOOR: Duration = Duration::from_millis(800);
const CEILING: Duration = Duration::from_millis(1200);

/// A measurement small enough that the fixed cost of setting Argon2 up is a
/// visible part of it, and extrapolating from it lands low.
const TOO_SMALL: Duration = Duration::from_millis(40);

/// Where the probe starts. Never at one pass: measured there, the per-pass cost
/// is a fifth higher than it settles at, because the arena is allocated and the
/// prehash computed whether there is one pass or a hundred.
const FIRST_PROBE: u64 = 4;

/// How many times the answer is measured and corrected before it is accepted.
const ROUNDS: usize = 3;

/// Tunes the number of passes so that opening the database takes about a second
/// on this machine.
///
/// Never returns work the pre-flight would refuse: a database Coffer creates
/// and then will not open is the worst thing this file could produce.
pub fn calibrate(clock: &dyn Stopwatch) -> Calibration {
    let most = preflight::max_iterations(MEMORY_BYTES);

    // Thrown away. The first derivation in a process costs about half as much
    // again as the ones after it - the sixty-four megabytes are touched for the
    // first time, and the cores have not come up to speed - and a count
    // extrapolated from it is short by the same margin.
    let _ = clock.time(Work::at(2));

    let mut probe = FIRST_PROBE.min(most);
    let mut took = measure(clock, probe);
    while took < TOO_SMALL && probe.saturating_mul(4) <= most {
        probe = probe.saturating_mul(4);
        took = measure(clock, probe);
    }

    let mut iterations = extrapolate(probe, took, most);
    for _ in 0..ROUNDS {
        let measured = measure(clock, iterations);
        if (FLOOR..=CEILING).contains(&measured) {
            return Calibration {
                work: Work::at(iterations),
                took: measured,
            };
        }

        let next = extrapolate(iterations, measured, most);
        if next == iterations {
            // Either the answer is as close as this machine can be asked to
            // get, or it is pinned against a ceiling. Measuring the same number
            // again would say the same thing.
            return Calibration {
                work: Work::at(iterations),
                took: measured,
            };
        }
        iterations = next;
    }

    Calibration {
        work: Work::at(iterations),
        took: measure(clock, iterations),
    }
}

/// The lowest of two runs.
///
/// The lowest and not the average: interference makes a derivation slower and
/// never faster, so the smallest measurement is the one closest to what the
/// machine can actually do, and erring that way asks for more passes rather
/// than fewer.
fn measure(clock: &dyn Stopwatch, iterations: u64) -> Duration {
    let work = Work::at(iterations);
    clock.time(work).min(clock.time(work))
}

/// How many passes that measurement says a second is worth.
fn extrapolate(iterations: u64, took: Duration, most: u64) -> u64 {
    if took.is_zero() {
        // Immeasurably fast, or a derivation that did not happen. Either way
        // there is nothing to scale, and the most this machine will accept is
        // the honest answer.
        return most;
    }

    let wanted = u128::from(iterations) * TARGET.as_nanos() / took.as_nanos();
    u64::try_from(wanted).unwrap_or(most).clamp(1, most)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    /// A machine whose speed the test decides, so that the search can be driven
    /// through every shape it has to survive without waiting for a single real
    /// derivation.
    struct Fake {
        per_pass: Duration,
        /// How much slower the first few answers are than the truth, and for
        /// how many of them. A real machine warms up.
        inflated: Cell<usize>,
    }

    impl Fake {
        fn at(per_pass: Duration) -> Fake {
            Fake {
                per_pass,
                inflated: Cell::new(0),
            }
        }

        fn warming(per_pass: Duration, answers: usize) -> Fake {
            Fake {
                per_pass,
                inflated: Cell::new(answers),
            }
        }
    }

    impl Stopwatch for Fake {
        fn time(&self, work: Work) -> Duration {
            let held = self.inflated.get();
            let factor = if held > 0 {
                self.inflated.set(held - 1);
                3
            } else {
                1
            };
            self.per_pass * factor * u32::try_from(work.iterations).unwrap_or(u32::MAX)
        }
    }

    /// Whatever the machine, the answer has to be one the file can be opened
    /// with afterwards.
    fn acceptable(calibration: Calibration) {
        assert!(calibration.work.iterations >= 1);
        assert!(preflight::acceptable(calibration.work).is_ok());
    }

    #[test]
    fn an_ordinary_machine_is_asked_for_about_a_second() {
        let found = calibrate(&Fake::at(Duration::from_millis(9)));
        acceptable(found);
        assert!(
            (FLOOR..=CEILING).contains(&found.took),
            "{:?} passes measured {:?}",
            found.work.iterations,
            found.took
        );
    }

    /// A machine fast enough to want more passes than the file may carry. One
    /// past the ceiling is a database Coffer writes and then refuses to open.
    #[test]
    fn a_machine_faster_than_the_format_allows_is_held_at_the_ceiling() {
        let found = calibrate(&Fake::at(Duration::from_micros(1)));
        acceptable(found);
        assert_eq!(
            found.work.iterations,
            preflight::max_iterations(MEMORY_BYTES)
        );
    }

    /// A machine so slow that even one pass is over the target. Zero is not an
    /// answer: the pre-flight refuses it and so does Argon2 itself.
    #[test]
    fn a_machine_slower_than_one_pass_still_gets_one() {
        let found = calibrate(&Fake::at(Duration::from_secs(2)));
        acceptable(found);
        assert_eq!(found.work.iterations, 1);
    }

    /// The warm-up. Without the discarded first run, three inflated answers
    /// walk the count down by the same factor and it never recovers.
    #[test]
    fn a_machine_that_starts_slow_still_converges() {
        let found = calibrate(&Fake::warming(Duration::from_millis(9), 3));
        acceptable(found);
        assert!(
            (FLOOR..=CEILING).contains(&found.took),
            "{:?} passes measured {:?}",
            found.work.iterations,
            found.took
        );
    }

    /// A machine whose answers alternate. The search has to stop rather than
    /// chase the two of them.
    #[test]
    fn a_machine_that_cannot_make_up_its_mind_still_stops() {
        struct Alternating(Cell<bool>);

        impl Stopwatch for Alternating {
            fn time(&self, work: Work) -> Duration {
                let fast = self.0.get();
                self.0.set(!fast);
                let per_pass = if fast { 1 } else { 40 };
                Duration::from_millis(per_pass * work.iterations)
            }
        }

        acceptable(calibrate(&Alternating(Cell::new(true))));
    }

    /// A stopwatch that answers instantly, which is what a refused derivation
    /// looks like. Nothing here may divide by it.
    #[test]
    fn a_machine_that_answers_in_no_time_is_not_divided_by() {
        struct Instant;

        impl Stopwatch for Instant {
            fn time(&self, _: Work) -> Duration {
                Duration::ZERO
            }
        }

        let found = calibrate(&Instant);
        acceptable(found);
        assert_eq!(
            found.work.iterations,
            preflight::max_iterations(MEMORY_BYTES)
        );
    }

    /// An answer so small that scaling it to a second would not fit in the
    /// count the format carries.
    #[test]
    fn an_extrapolation_that_would_overflow_lands_on_the_ceiling() {
        let most = preflight::max_iterations(MEMORY_BYTES);
        assert_eq!(extrapolate(u64::MAX, Duration::from_nanos(1), most), most);
        assert_eq!(extrapolate(1, Duration::ZERO, most), most);
        assert_eq!(extrapolate(1, Duration::from_secs(3600), most), 1);
    }

    /// The one test that touches a real derivation. It asserts nothing about
    /// how long anything took: the suite runs tests in parallel, and build
    /// machines differ by an order of magnitude.
    #[test]
    fn the_real_machine_answers_at_all() {
        assert!(Machine.time(Work::at(1)) > Duration::ZERO);
    }

    /// What the calibration writes has to be what the file says, down to the
    /// variant. Argon2 and Argon2id are both spelt the same way in the type,
    /// and only one of them is what the spec asks for.
    #[test]
    fn the_work_written_into_a_file_is_argon2id_at_the_fixed_memory() {
        assert_eq!(
            Work::at(37).config(),
            KdfConfig::Argon2id {
                iterations: 37,
                memory: 64 * 1024 * 1024,
                parallelism: 4,
                version: argon2::Version::Version13,
            }
        );
    }
}
