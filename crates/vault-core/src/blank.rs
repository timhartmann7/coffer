//! What is in a database nobody has put anything in yet.
//!
//! The library's own empty database is nearly empty in the file as well: every
//! `Meta` field is skipped when it has no value, so the `<Meta>` block a bare
//! new database writes carries a generator and nothing else. Other clients fill
//! the gaps in with their own defaults on load, which means the file says one
//! thing to Coffer and another to KeePassXC, and neither of them says it out
//! loud.
//!
//! So a database Coffer makes is written down in full. It is also the one place
//! that decides what a new vault's history limits are; every other rule about
//! history reads them back out of the file.

use keepass::Database;
use keepass::config::DatabaseConfig;
use keepass::db::{MemoryProtection, Meta, Times};

use crate::error::VaultError;
use crate::history;
use crate::kdf::Work;
use crate::text;

/// The built-in icon a new vault's top group gets. Forty-eight is the folder
/// every KeePass client draws for a root group.
const FOLDER_ICON: usize = 48;

/// How long a client should go on offering to tidy a database up. KeePassXC's
/// own default, written down rather than left out so that the file answers the
/// question instead of the reader.
const MAINTENANCE_DAYS: usize = 365;

/// Neither recommended nor forced. Written as the format's "never" rather than
/// left absent, so that no client nags the reader to change the password on a
/// schedule. They change it in Coffer's settings when they choose to.
const NEVER: isize = -1;

/// A database with nothing in it, named and described the way a real one is.
pub(crate) fn database(name: &str, work: Work) -> Result<Database, VaultError> {
    text::writable(name)?;

    // `DatabaseConfig` is marked non-exhaustive, so it cannot be written as a
    // literal from here and neither can `..Default::default()`. The default is
    // also Argon2**d** at one megabyte, which is not what the spec asks for and
    // is spelt almost the same way, so the derivation is assigned rather than
    // inherited.
    let mut config = DatabaseConfig::default();
    config.kdf_config = work.config();

    let mut database = Database::with_config(config);
    let now = Times::now();

    let mut root = database.root_mut();
    root.name = name.to_owned();
    root.set_icon_builtin(FOLDER_ICON);
    root.times.expiry = Some(now);

    database.meta = Meta {
        database_name: Some(name.to_owned()),
        database_name_changed: Some(now),
        master_key_changed: Some(now),
        master_key_change_rec: Some(NEVER),
        master_key_change_force: Some(NEVER),
        maintenance_history_days: Some(MAINTENANCE_DAYS),
        // Every "changed" date is written, even for a field that has never been
        // set. A date a file leaves out is one every client puts its own value
        // in, which makes two readings of the same file disagree.
        database_description_changed: Some(now),
        default_username_changed: Some(now),
        recyclebin_changed: Some(now),
        entry_templates_group_changed: Some(now),
        memory_protection: Some(MemoryProtection::default()),
        // A recycle bin is asked for and not made. The group itself appears the
        // first time something is deleted; naming a group here that does not
        // exist is how a database ends up with two of them.
        recyclebin_enabled: Some(true),
        history_max_items: Some(history::DEFAULT_MAX_ITEMS),
        history_max_size: Some(history::DEFAULT_MAX_SIZE),
        settings_changed: Some(now),
        ..Meta::default()
    };

    Ok(database)
}
