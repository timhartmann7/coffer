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
    // A JSON body means the webview's IPC fell back to `postMessage`, where the
    // password would have travelled through a JavaScript string and a JSON
    // document. Refusing is the only safe answer: accepting it would make a
    // broken Content-Security-Policy invisible.
    let InvokeBody::Raw(password) = request.body() else {
        return Err(Failure::refused(
            "the master password must be sent as bytes",
        ));
    };

    let password = Zeroizing::new(password.clone());
    let session = Arc::clone(&session);

    // Key derivation is a second of work by design. It happens on a thread that
    // is allowed to block, so the window goes on drawing while it runs.
    tauri::async_runtime::spawn_blocking(move || session.unlock(password))
        .await
        .map_err(|_| Failure::internal("the database could not be opened"))?
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
    secret
        .expose_str()
        .map(Revealed::new)
        .ok_or_else(|| Failure::refused("that value is not text"))
}

/// Copies one field's value to the clipboard. Nothing comes back but the number
/// of seconds until Coffer takes it off again.
#[tauri::command]
pub fn copy(entry: String, field: String, session: Held<'_>) -> Result<u64, Failure> {
    let secret = session.reveal(dto::entry_id(&entry)?, &field)?;
    let value = secret
        .expose_str()
        .ok_or_else(|| Failure::refused("that value is not text"))?;

    clipboard::copy(value, clipboard::CLEAR_AFTER);
    Ok(clipboard::CLEAR_AFTER.as_secs())
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
    let database = session.database().ok_or_else(Failure::no_vault)?;
    let path = snapshot::slot(&database, index).map_err(Failure::io)?;

    if !path.is_file() {
        return Err(Failure::gone());
    }

    session.choose(path.clone());
    Ok(Database::of(&path))
}
