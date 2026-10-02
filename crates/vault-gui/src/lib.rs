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

mod autolock;
mod buttons;
mod clipboard;
mod closing;
mod commands;
mod context;
mod drafts;
mod dto;
mod error;
#[cfg(test)]
mod fixtures;
mod generator;
mod home;
mod kept;
mod lock;
mod main_thread;
mod menu;
mod offered;
mod opener;
mod recent;
mod route;
mod session;
mod settings;
#[cfg(test)]
mod source;
mod window;

use std::sync::Arc;

use tauri::Manager;

use crate::autolock::timer::Timer;
use crate::autolock::{Deadline, Event, Reason, watch};
use crate::session::Session;

/// Opens the window.
pub fn run() {
    let application = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Tauri draws a bar of its own for an application that names none, and
        // that one carries Services (see `menu.rs`).
        .menu(menu::bar)
        .on_menu_event(route::chosen)
        .setup(|app| {
            let directory = app.path().app_config_dir().ok();
            let remembered = directory.as_deref().and_then(recent::remembered);

            // Managed before anything that locks is started: the timer and the
            // notification blocks both find the session and the settings by
            // type, and neither may run before they are there.
            let preferences = Arc::new(settings::preferences(directory.clone()));
            app.manage(Arc::new(generator::remembered(directory.clone())));
            app.manage(Arc::new(Session::new(remembered, directory)));
            app.manage(Arc::clone(&preferences));
            app.manage(route::Route::default());
            app.manage(context::Menus::default());

            let locking = app.handle().clone();
            app.manage(Arc::new(Timer::start(
                Deadline::new(preferences.get().idle()),
                move |reason| lock::lock(&locking, reason),
            )));

            // The machine's own triggers, which the reader can turn off one at
            // a time. They are read here rather than in the timer, so that the
            // deadline knows nothing about what a reader chose.
            let ticking = app.handle().clone();
            let chosen = Arc::clone(&preferences);
            watch::watch(move |reason| {
                if !reason.wanted(chosen.get()) {
                    return;
                }
                // Through the timer rather than straight to the lock, so that
                // the deadline it was keeping is cleared by the same message
                // that tears the window down. There is one route in.
                if let Some(timer) = ticking.try_state::<Arc<Timer>>() {
                    timer.post(Event::Locking(reason));
                }
            });

            window::first(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::settings,
            commands::set_settings,
            commands::choose_database,
            commands::choose_found,
            commands::choose_existing,
            commands::choose_new_database,
            commands::default_new_database,
            commands::target,
            commands::calibrate,
            commands::create_database,
            commands::unlock,
            commands::unlock_over,
            commands::choose_key_file,
            commands::forget_key_file,
            commands::lock,
            commands::stirred,
            commands::listen,
            commands::menu_state,
            commands::close_window,
            commands::context_menu,
            commands::tree,
            commands::entry,
            commands::reveal,
            commands::copy,
            commands::open_url,
            commands::snapshots,
            commands::choose_snapshot,
            commands::choose_rescue,
            commands::discard_rescue,
            commands::put_back_rescue,
            commands::promote_rescue,
            commands::leave_rescue,
            commands::kinds,
            commands::create_entry,
            commands::create_from_template,
            commands::duplicate_entry,
            commands::delete_entries,
            commands::create_group,
            commands::rename_group,
            commands::delete_group,
            commands::put_back_entries,
            commands::tag_entries,
            commands::untag_entries,
            commands::put_back_group,
            commands::move_entries,
            commands::move_entries_back,
            commands::move_group,
            commands::move_group_back,
            commands::empty_recycle_bin,
            commands::set_field,
            commands::draft,
            commands::remove_field,
            commands::set_protection,
            commands::rename_field,
            commands::undo_removal,
            commands::set_tags,
            commands::add_attachment,
            commands::keep_both_attachments,
            commands::replace_attachment,
            commands::withdraw_attachment,
            commands::export_attachment,
            commands::remove_attachment,
            commands::remove_attachment_and_versions,
            commands::versions,
            commands::version,
            commands::reveal_version,
            commands::copy_version,
            commands::restore_version,
            commands::delete_version,
            commands::clear_history,
            commands::generator,
            commands::generate_password,
            commands::save,
            commands::save_over,
            commands::save_copy,
            commands::reload,
            commands::rival,
            commands::change_master_password,
            commands::remove_old_snapshots,
        ])
        .build(tauri::generate_context!());

    match application {
        Ok(application) => application.run(|app, event| match event {
            // The close button and Close Window. The page is asked to finish
            // first (`closing.rs`); with no page the window goes and Rust
            // locks.
            tauri::RunEvent::WindowEvent {
                ref label,
                event: tauri::WindowEvent::CloseRequested { ref api, .. },
                ..
            } if label.starts_with(window::MAIN) => {
                if closing::requested(app, label) {
                    api.prevent_close();
                }
            }
            tauri::RunEvent::WindowEvent {
                ref label,
                event: tauri::WindowEvent::Destroyed,
                ..
            } if label.starts_with(window::MAIN) => {
                // The page that was listening went with its window. Until the
                // next one listens, the bar offers only what builds a window
                // again. A window that had already been replaced - its event
                // handled after the next page listened and said what it
                // offers - leaves the bar to that page.
                if app
                    .try_state::<route::Route>()
                    .is_some_and(|route| route.forget(label))
                {
                    menu::enable(app, None);
                }

                // The window is built again exactly here after a lock, and
                // nowhere else. A destroy is a message to the event loop rather
                // than something that has happened by the time the call
                // returns, and the label stays taken until this event is
                // delivered - so destroying and building in one function
                // always fails, and building from a timer thread builds a
                // window AppKit will not let that thread decorate.
                //
                // Only when a lock asked for it. The reader closing the window
                // arrives as the same event, and a window that came back from
                // that would be one Coffer could never put away: the Dock
                // brings that one back.
                if window::wanted_again() {
                    // Posted back to the main thread rather than built here.
                    // The runtime takes the window that just went out of its
                    // own books *after* this callback returns, and the
                    // replacement carries the same label - so a window built
                    // inline is the one that cleanup erases. What is left is a
                    // frame macOS still draws, with no page in it and nothing
                    // to answer a command: the reader locks the vault and
                    // cannot get back in.
                    let handle = app.clone();
                    let _ = app.run_on_main_thread(move || window::again(&handle));
                }
            }
            // The last window going is not Coffer quitting. The reader closed
            // it, which locked the vault, and Coffer waits in the Dock until
            // it is clicked or quit. Quit arrives as `Exit` and never asks
            // first, and an exit with a code is one Coffer asked for itself.
            tauri::RunEvent::ExitRequested {
                code: None,
                ref api,
                ..
            } => api.prevent_exit(),
            // The Dock icon, or the application opened again while it runs.
            tauri::RunEvent::Reopen { .. } => window::bring_back(app),
            // The window toolkit ends its loop by exiting the process, which
            // runs no destructor. Without this, quitting with a vault open
            // leaves the lock file beside the database for good, and a copied
            // password on the clipboard until somebody copies something else.
            tauri::RunEvent::Exit => lock::lock(app, Reason::Quitting),
            _ => {}
        }),
        // Nothing has been unlocked when the window fails to open, so there is
        // nothing here that could name a secret.
        Err(error) => {
            eprintln!("Coffer could not open its window: {error}");
            std::process::exit(1);
        }
    }
}
