//! The Coffer vault engine: reading, writing and guarding a KDBX 4.1 file.
//!
//! Nothing here knows about Tauri, a window or a webview. It opens a file,
//! unlocks it, hands back the tree, hands back one value at a time, applies a
//! change, and saves with a snapshot behind it.
//!
//! Two rules shape every module. A field present in a database Coffer opened is
//! present in the database Coffer saves, and no secret is ever written to a log
//! line, an error message or a `Debug` implementation.

#![deny(dead_code, unused_imports, unused_variables, unused_mut)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

pub mod error;
pub mod generate;
pub mod kdf;
pub mod key;
pub mod kind;
pub mod model;
pub mod scrub;
pub mod secret;
pub mod storage;
pub mod url;

mod attachment;
mod bin;
mod blank;
mod clash;
mod content;
mod history;
mod preflight;
mod templates;
mod text;
mod vault;
mod wipe;

pub use crate::clash::Clash;
pub use crate::error::VaultError;
pub use crate::key::MasterKey;
pub use crate::secret::SecretValue;
pub use crate::vault::{
    Adopted, Attached, LockPolicy, MAX_ATTACHMENT_BYTES, NewValue, ReadOnly, Recipe, Rescue, Rival,
    Typing, Vault, Written,
};
