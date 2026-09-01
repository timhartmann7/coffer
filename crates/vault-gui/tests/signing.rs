//! The signature the bundle carries.
//!
//! Without an identity in the configuration the bundler signs nothing, and what
//! ships is the ad-hoc signature the linker puts on the executable alone: no
//! sealed resources, no bound `Info.plist`, and the crate's name where the
//! bundle identifier belongs. `codesign --verify` rejects that, and a rejected
//! signature is not the same thing as an absent one - macOS calls the first
//! damaged and offers no way past it, while the second is the ordinary
//! unidentified-developer dialog the README sends the reader to.
//!
//! CI never builds a bundle, so nothing else in the project would notice this
//! line being taken out again.

#![deny(dead_code, unused_imports, unused_variables, unused_mut)]

use std::path::PathBuf;

fn configuration() -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let text = std::fs::read_to_string(path).expect("tauri.conf.json is beside the crate");
    serde_json::from_str(&text).expect("tauri.conf.json is JSON")
}

#[test]
fn the_bundle_names_an_identity_to_sign_itself_with() {
    let configured = configuration();
    let identity = configured["bundle"]["macOS"]["signingIdentity"]
        .as_str()
        .expect("the macOS bundle names a signing identity");

    assert!(
        !identity.is_empty(),
        "an empty identity signs nothing, which is the state this test exists to prevent"
    );
}
