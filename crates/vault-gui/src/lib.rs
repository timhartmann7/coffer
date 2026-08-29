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
mod window;

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

            if let Some(main) = app.get_webview_window("main") {
                window::centre_buttons(&main);
            }

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
            commands::create_entry,
            commands::delete_entry,
            commands::create_group,
            commands::rename_group,
            commands::delete_group,
            commands::empty_recycle_bin,
            commands::set_field,
            commands::remove_field,
            commands::set_tags,
            commands::add_attachment,
            commands::export_attachment,
            commands::remove_attachment,
            commands::versions,
            commands::version,
            commands::reveal_version,
            commands::restore_version,
            commands::delete_version,
            commands::clear_history,
            commands::generate_password,
            commands::save,
            commands::save_over,
            commands::save_copy,
            commands::reload,
            commands::rival,
        ])
        .build(tauri::generate_context!());

    match application {
        Ok(application) => application.run(|_, event| {
            // A copied secret is taken off the clipboard by a thread that
            // sleeps until its minute is up, and a sleeping thread does not
            // survive the process. Quitting inside that minute would otherwise
            // leave the password on the clipboard for good.
            if matches!(event, tauri::RunEvent::Exit) {
                clipboard::revoke_pending();
            }
        }),
        // Nothing has been unlocked when the window fails to open, so there is
        // nothing here that could name a secret.
        Err(error) => {
            eprintln!("Coffer could not open its window: {error}");
            std::process::exit(1);
        }
    }
}
