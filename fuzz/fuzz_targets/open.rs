//! Opening arbitrary bytes as a database.
//!
//! Everything on this path runs before the file has proved it is genuine: the
//! version block, Coffer's own header pre-flight, and then the library's parser
//! and key derivation. None of it may panic, hang or allocate without bound on
//! bytes somebody sent the user.
//!
//! Run with a nightly toolchain, from this directory:
//!
//!     cargo +nightly fuzz run open -- -max_len=65536
//!
//! Seed the corpus from the fixtures:
//!
//!     mkdir -p corpus/open
//!     cp ../crates/vault-core/tests/fixtures/*.kdbx corpus/open/

#![no_main]

use libfuzzer_sys::fuzz_target;
use vault_core::{LockPolicy, MasterKey, Vault};
use zeroize::Zeroizing;

fuzz_target!(|data: &[u8]| {
    let Ok(directory) = tempfile::tempdir() else {
        return;
    };
    let path = directory.path().join("fuzzed.kdbx");
    if std::fs::write(&path, data).is_err() {
        return;
    }

    let key = MasterKey::from_password(Zeroizing::new(b"coffer-test".to_vec()));
    let _ = Vault::open(&path, key, LockPolicy::Respect);
});
