//! The Tauri command layer over [`vault_core`].
//!
//! Nothing but error mapping, the shape of the messages and the timers lives
//! here: every decision about the database is made in `vault-core`.

#![deny(dead_code, unused_imports, unused_variables, unused_mut)]
// A panic in shipping code takes the window down with the user's unsaved work.
// A panic in a test is how a test reports a failure, and the suites in this
// workspace are written that way, so the ban is on the code that ships. The
// library target is still compiled without `cfg(test)`, so nothing in it
// escapes the check.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )
)]

#[cfg(not(target_os = "macos"))]
compile_error!("Coffer's window is macOS only. vault-core is the part that is not.");

mod clipboard;
mod commands;
mod dto;
mod error;
mod opener;
mod recent;
mod session;

use std::sync::Arc;

use tauri::Manager;

use crate::session::Session;

/// Opens the window.
pub fn run() {
    let application = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let remembered = app
                .path()
                .app_config_dir()
                .ok()
                .and_then(|directory| recent::remembered(&directory));

            app.manage(Arc::new(Session::new(remembered)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::choose_database,
            commands::unlock,
            commands::lock,
            commands::tree,
            commands::entry,
            commands::reveal,
            commands::copy,
            commands::open_url,
            commands::snapshots,
            commands::choose_snapshot,
        ])
        .run(tauri::generate_context!());

    // Nothing has been unlocked when the window fails to open, so there is
    // nothing here that could name a secret.
    if let Err(error) = application {
        eprintln!("Coffer could not open its window: {error}");
        std::process::exit(1);
    }
}
