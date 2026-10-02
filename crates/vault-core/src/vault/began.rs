//! When a vault began, as its own file says.
//!
//! What a reminder to keep a copy elsewhere counts from when no copy was ever
//! made. The date is read and never written: nothing Coffer keeps about copies
//! goes into the file (spec section 12).

use chrono::NaiveDateTime;

use super::Vault;

impl Vault {
    /// When the vault was made, as its top group says.
    ///
    /// KeePass, KeePassXC and Coffer all date a new database's top group when
    /// they make it, and no client moves that date afterwards, so it is the one
    /// record of when a vault began that every file keeps. Nothing when the
    /// file does not say. Handed back as the file holds it, whatever year that
    /// is: what a date in 1600 or 3000 means is the caller's to decide.
    pub fn made(&self) -> Option<NaiveDateTime> {
        self.database.root().times.creation
    }
}
