//! The vault-core suite.
//!
//! One test binary rather than one per subject, so that a helper used by a
//! single test is not dead code in every other test target.

#![deny(dead_code, unused_imports, unused_variables, unused_mut)]

mod adversarial;
mod create;
mod edit;
mod generate;
mod history;
mod normalise;
mod open;
mod portable;
mod property;
mod round_trip;
mod save;
mod scan;
mod storage;
mod support;
mod url;
mod wipe;
