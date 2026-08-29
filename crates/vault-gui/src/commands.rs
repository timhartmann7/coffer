//! What the webview may ask for.
//!
//! Every command here is thin. It turns what the screen sent into what
//! `vault-core` understands, asks the session, and turns the answer into a DTO.
//! No decision about the database is made in this file.
//!
//! Two of them are `async` on purpose. A plain `#[tauri::command]` runs inline
//! on the thread that handles IPC, which on macOS is the thread that draws the
//! window: key derivation there would freeze the window for a second, and a
//! file dialog there deadlocks, because the panel needs the run loop that the
//! call is blocking.

use std::sync::Arc;

use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use vault_core::storage::snapshot;
use zeroize::Zeroizing;

use crate::dto::{self, Database, Entry, Group, Revealed, Snapshot, Status};
use crate::error::Failure;
use crate::session::Session;
use crate::{clipboard, opener, recent};

/// The session is behind an `Arc` so that a command can take it onto a blocking
/// thread, which is where key derivation belongs.
type Held<'a> = State<'a, Arc<Session>>;

#[tauri::command]
pub fn status(session: Held<'_>) -> Status {
    Status {
        database: session.database().as_deref().map(Database::of),
        unlocked: session.is_unlocked(),
    }
}

/// Asks for a database with the system's own file dialog.
///
/// The path never comes from the webview: the webview asks for a picker, the
/// user picks, and Coffer keeps the answer. Nothing the frontend sends can
/// point Coffer at a file.
#[tauri::command]
pub async fn choose_database(
    app: AppHandle,
    session: Held<'_>,
) -> Result<Option<Database>, Failure> {
    let mut picker = app
        .dialog()
        .file()
        .set_title("Open a KeePass database")
        .add_filter("KeePass database", &["kdbx"]);

    if let Some(directory) = session
        .database()
        .as_deref()
        .and_then(std::path::Path::parent)
    {
        picker = picker.set_directory(directory);
    }

    if session.is_unlocked() {
        return Err(Failure::refused("lock the vault before opening another"));
    }

    let Some(chosen) = picker.blocking_pick_file() else {
        return Ok(None);
    };

    let path = chosen
        .into_path()
        .map_err(|_| Failure::refused("that file has no path Coffer can open"))?;

    session.choose(path.clone());

    // A database Coffer fails to remember is one the user picks again next
    // launch. That is not a reason to refuse to open it now.
    if let Ok(directory) = app.path().app_config_dir() {
        let _ = recent::remember(&directory, &path);
    }

    Ok(Some(Database::of(&path)))
}

/// Opens the chosen database with the master password.
///
/// The password arrives as the whole body of the message, as bytes. It is never
/// a JSON string, never a field of an object, and never a value anything else
/// could have written down on the way.
#[tauri::command]
pub async fn unlock(request: Request<'_>, session: Held<'_>) -> Result<(), Failure> {
    let password = password_of(request.body())?;
    let session = Arc::clone(&session);

    // Key derivation is a second of work by design. It happens on a thread that
    // is allowed to block, so the window goes on drawing while it runs.
    tauri::async_runtime::spawn_blocking(move || session.unlock(password))
        .await
        .map_err(|_| Failure::internal("the database could not be opened"))?
}

/// The master password out of a message, and only out of a message that
/// carried it as bytes.
///
/// A JSON body means the webview's IPC fell back to `postMessage`, where the
/// password would have travelled through a JavaScript string and a JSON
/// document. Refusing is the only safe answer: accepting it would make a broken
/// Content-Security-Policy invisible, and a broken one is silent.
fn password_of(body: &InvokeBody) -> Result<Zeroizing<Vec<u8>>, Failure> {
    match body {
        InvokeBody::Raw(password) => Ok(Zeroizing::new(password.clone())),
        InvokeBody::Json(_) => Err(Failure::refused(
            "the master password must be sent as bytes",
        )),
    }
}

/// Wipes the decrypted database out of memory.
#[tauri::command]
pub fn lock(session: Held<'_>) {
    session.lock();
}

#[tauri::command(async)]
pub fn tree(session: Held<'_>) -> Result<Group, Failure> {
    Ok(Group::of(&session.tree()?))
}

#[tauri::command]
pub fn entry(id: String, session: Held<'_>) -> Result<Entry, Failure> {
    Ok(Entry::of(&session.entry(dto::entry_id(&id)?)?))
}

/// Hands one field's value to the screen. This is the only command that returns
/// a secret, and it returns one field of one entry, once.
#[tauri::command]
pub fn reveal(entry: String, field: String, session: Held<'_>) -> Result<Revealed, Failure> {
    let secret = session.reveal(dto::entry_id(&entry)?, &field)?;
    Ok(Revealed::new(text(&secret)?))
}

/// Copies one field's value to the clipboard. Nothing comes back but the number
/// of seconds until Coffer takes it off again.
#[tauri::command]
pub fn copy(entry: String, field: String, session: Held<'_>) -> Result<u64, Failure> {
    let secret = session.reveal(dto::entry_id(&entry)?, &field)?;

    clipboard::copy(text(&secret)?, clipboard::CLEAR_AFTER);
    Ok(clipboard::CLEAR_AFTER.as_secs())
}

/// A revealed value as text.
///
/// Every field value in a KeePass file is text, so the refusal is for the day
/// something that is not - an attachment - comes through the same door.
fn text(secret: &vault_core::SecretValue) -> Result<&str, Failure> {
    secret
        .expose_str()
        .ok_or_else(|| Failure::refused("that value is not text"))
}

/// Opens an entry's address.
///
/// The address is read out of the database here rather than sent by the screen,
/// so that the only thing the webview can ask Coffer to open is an entry it can
/// already see.
#[tauri::command]
pub fn open_url(entry: String, session: Held<'_>) -> Result<(), Failure> {
    let entry = session.entry(dto::entry_id(&entry)?)?;

    match opener::open(entry.url()) {
        opener::Opened::Yes => Ok(()),
        opener::Opened::Refused => Err(Failure::refused(
            "Coffer does not open addresses of that kind",
        )),
        // The scheme is one Coffer opens and the system still did nothing with
        // it, which is a different thing to say.
        opener::Opened::NoHandler => Err(Failure::refused(
            "this Mac has nothing registered to open that address",
        )),
    }
}

/// The snapshots beside the chosen database, most recent first. The unlock
/// screen offers them when the database itself will not open.
#[tauri::command]
pub fn snapshots(session: Held<'_>) -> Result<Vec<Snapshot>, Failure> {
    let database = session.database().ok_or_else(Failure::no_vault)?;
    let taken = snapshot::taken(&database).map_err(Failure::io)?;
    Ok(taken.iter().map(Snapshot::of).collect())
}

/// Points the session at one of those snapshots.
///
/// The index is all the webview sends; the path is built here from the database
/// the user chose, so no message from the screen can name a file.
#[tauri::command]
pub fn choose_snapshot(index: u32, session: Held<'_>) -> Result<Database, Failure> {
    if session.is_unlocked() {
        return Err(Failure::refused("lock the vault before opening another"));
    }

    let database = session.database().ok_or_else(Failure::no_vault)?;
    let path = snapshot::slot(&database, index).map_err(Failure::io)?;

    if !path.is_file() {
        return Err(Failure::gone());
    }

    session.choose(path.clone());
    Ok(Database::of(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the raw body. A number array is what Tauri produces
    /// when the custom-protocol IPC is blocked and it falls back to
    /// `postMessage`, and taking one would mean the password had already been a
    /// JavaScript string and a JSON document by the time it arrived.
    #[test]
    fn a_password_that_did_not_arrive_as_bytes_is_refused() {
        let numbers = serde_json::json!([104, 117, 110, 116, 101, 114, 50]);
        let failure = password_of(&InvokeBody::Json(numbers)).expect_err("a JSON body is refused");
        assert!(format!("{failure:?}").contains("must be sent as bytes"));

        let text = serde_json::json!("hunter2");
        assert!(password_of(&InvokeBody::Json(text)).is_err());

        let object = serde_json::json!({ "password": "hunter2" });
        assert!(password_of(&InvokeBody::Json(object)).is_err());
    }

    #[test]
    fn a_password_that_arrived_as_bytes_is_taken_whole() {
        for password in [
            b"".to_vec(),
            b"correct horse battery staple".to_vec(),
            vec![0xff, 0xfe, 0x00, 0x41],
            vec![b'a'; 10 * 1024 * 1024],
        ] {
            let taken = password_of(&InvokeBody::Raw(password.clone())).expect("bytes are taken");
            assert_eq!(taken.as_slice(), password.as_slice());
        }
    }
}
