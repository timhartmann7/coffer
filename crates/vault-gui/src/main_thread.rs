//! Work only the thread AppKit draws on may do, asked for by a command that
//! waits for the answer somewhere else.
//!
//! A command answered on that thread holds it for as long as the command
//! takes, and a command waiting there for the answer to work posted there
//! waits for itself. So the work is posted, and the command waits on its
//! own thread for what comes back. Nothing posted here may reach the session:
//! the thread it runs on is the one a save would then hold up.

use objc2::MainThreadMarker;
use tauri::AppHandle;

use crate::error::Failure;

/// Runs `work` on the thread AppKit draws on, handing it the proof that it is
/// there, and answers with what it answered.
///
/// Work that never runs - the event loop ended first, or the thread turned
/// out not to be the main one - is answered with a failure rather than waited
/// for forever.
pub async fn answer<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce(MainThreadMarker) -> T + Send + 'static,
) -> Result<T, Failure> {
    let (sent, mut answered) = tauri::async_runtime::channel(1);
    app.run_on_main_thread(move || {
        if let Some(main) = MainThreadMarker::new() {
            // One message into a channel with room for one: it never waits.
            let _ = sent.try_send(work(main));
        }
    })
    .map_err(|_| unreached())?;
    answered.recv().await.ok_or_else(unreached)
}

fn unreached() -> Failure {
    Failure::internal("the window could not be reached")
}
