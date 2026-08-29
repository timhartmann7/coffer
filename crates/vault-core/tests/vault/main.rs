//! The vault-core suite.
//!
//! One test binary rather than one per subject, so that a helper used by a
//! single test is not dead code in every other test target.

#![deny(dead_code, unused_imports, unused_variables, unused_mut)]

mod adversarial;
mod edit;
mod generate;
mod history;
mod normalise;
mod open;
mod probe;
mod property;
mod round_trip;
mod save;
mod storage;
mod support;
mod url;
