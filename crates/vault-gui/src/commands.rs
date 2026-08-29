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

use vault_core::generate::{Alphabet, Recipe};
use vault_core::{NewValue, Vault};

use crate::dto::{self, Database, Entry, Group, Made, Revealed, Rival, Snapshot, Status, Version};
use crate::error::Failure;
use crate::session::Session;
use crate::{clipboard, opener, recent};

/// The session is behind an `Arc` so that a command can take it onto a blocking
/// thread, which is where key derivation belongs.
type Held<'a> = State<'a, Arc<Session>>;

#[tauri::command]
pub fn status(session: Held<'_>) -> Status {
    let (entries, dirty, read_only) = session
        .with(|vault| (vault.count(), vault.is_dirty(), vault.is_read_only()))
        .unwrap_or((0, false, false));

    Status {
        database: session.database().as_deref().map(Database::of),
        unlocked: session.is_unlocked(),
        entries,
        dirty,
        read_only,
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

/// The whole tree, as it is now. Every command that changes the shape of the
/// vault answers with one of these, so the screen never draws from a picture it
/// assembled itself.
fn tree_of(session: &Session) -> Result<Group, Failure> {
    Ok(Group::of(&session.tree()?))
}

fn entry_of(session: &Session, id: &str) -> Result<Entry, Failure> {
    Ok(Entry::of(&session.entry(dto::entry_id(id)?)?))
}

#[tauri::command(async)]
pub fn create_entry(group: String, session: Held<'_>) -> Result<Made, Failure> {
    let group = dto::group_id(&group)?;
    let made = session.with_mut(|vault| vault.create_entry(group))??;

    Ok(Made {
        tree: tree_of(&session)?,
        entry: made.to_string(),
    })
}

#[tauri::command(async)]
pub fn delete_entry(entry: String, session: Held<'_>) -> Result<Group, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.delete_entry(id))??;
    tree_of(&session)
}

#[tauri::command(async)]
pub fn create_group(parent: String, name: String, session: Held<'_>) -> Result<Group, Failure> {
    let parent = dto::group_id(&parent)?;
    session.with_mut(|vault| vault.create_group(parent, &name))??;
    tree_of(&session)
}

#[tauri::command(async)]
pub fn rename_group(group: String, name: String, session: Held<'_>) -> Result<Group, Failure> {
    let group = dto::group_id(&group)?;
    session.with_mut(|vault| vault.rename_group(group, &name))??;
    tree_of(&session)
}

#[tauri::command(async)]
pub fn delete_group(group: String, session: Held<'_>) -> Result<Group, Failure> {
    let group = dto::group_id(&group)?;
    session.with_mut(|vault| vault.delete_group(group))??;
    tree_of(&session)
}

#[tauri::command(async)]
pub fn empty_recycle_bin(session: Held<'_>) -> Result<Group, Failure> {
    session.with_mut(vault_core::Vault::empty_recycle_bin)??;
    tree_of(&session)
}

/// Writes one field of one entry.
///
/// `protect` is what the screen read off the field it is editing, so a value
/// the database keeps protected goes back protected. Getting that wrong would
/// write a password into the file as plain text inside the encrypted body.
#[tauri::command(async)]
pub fn set_field(
    entry: String,
    field: String,
    value: String,
    protect: bool,
    session: Held<'_>,
) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    let written = if protect {
        NewValue::Protected(Zeroizing::new(value))
    } else {
        NewValue::Open(value)
    };

    session.with_mut(|vault| vault.set_field(id, &field, written))??;
    entry_of(&session, &entry)
}

#[tauri::command(async)]
pub fn remove_field(entry: String, field: String, session: Held<'_>) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.remove_field(id, &field))??;
    entry_of(&session, &entry)
}

#[tauri::command(async)]
pub fn set_tags(entry: String, tags: Vec<String>, session: Held<'_>) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.set_tags(id, tags))??;
    entry_of(&session, &entry)
}

/// Puts a file on an entry.
///
/// The file is chosen and read in Rust. Nothing about it crosses the boundary
/// on the way in: the webview asks for a picker and is told what the entry
/// holds afterwards.
#[tauri::command]
pub async fn add_attachment(
    app: AppHandle,
    entry: String,
    session: Held<'_>,
) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;

    let Some(chosen) = app
        .dialog()
        .file()
        .set_title("Add a file to this entry")
        .blocking_pick_file()
    else {
        return entry_of(&session, &entry);
    };
    let path = chosen
        .into_path()
        .map_err(|_| Failure::refused("that file has no path Coffer can read"))?;

    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| Failure::refused("that file has no name"))?;

    // Asked before the file is read rather than after. A twenty gigabyte pick
    // read first and refused second is twenty gigabytes in the memory of a
    // process holding a decrypted vault, and the allocation that fails takes
    // the vault down with it.
    let size = std::fs::metadata(&path).map_err(Failure::io)?.len();
    if size > vault_core::MAX_ATTACHMENT_BYTES as u64 {
        return Err(Failure::from(vault_core::VaultError::AttachmentTooLarge));
    }

    let data = Zeroizing::new(std::fs::read(&path).map_err(Failure::io)?);

    session.with_mut(|vault| vault.add_attachment(id, &name, data))??;
    entry_of(&session, &entry)
}

/// Writes one of an entry's files out to wherever the reader says.
///
/// The bytes never reach the webview and the path never comes from it: the
/// panel is opened here and the file is written here, owner-only and through
/// the same staged write the database gets.
#[tauri::command]
pub async fn export_attachment(
    app: AppHandle,
    entry: String,
    name: String,
    session: Held<'_>,
) -> Result<(), Failure> {
    let id = dto::entry_id(&entry)?;
    let suggested = session
        .with(|vault| vault.entry(id))?
        .ok_or_else(Failure::no_such_entry)?
        .attachments
        .iter()
        .find(|attachment| attachment.name == name)
        .ok_or_else(|| Failure::from(vault_core::VaultError::NoSuchAttachment))?
        .file_name();

    let bytes = session.with(|vault| vault.attachment(id, &name))??;

    let Some(chosen) = app
        .dialog()
        .file()
        .set_title("Write this file out")
        .set_file_name(&suggested)
        .blocking_save_file()
    else {
        return Ok(());
    };
    let path = chosen
        .into_path()
        .map_err(|_| Failure::refused("that place has no path Coffer can write"))?;

    vault_core::storage::atomic::write_atomic::<std::io::Error, _>(
        &path,
        |writer: &mut dyn std::io::Write| writer.write_all(bytes.expose()),
    )
    .map_err(Failure::io)
}

#[tauri::command(async)]
pub fn remove_attachment(entry: String, name: String, session: Held<'_>) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.remove_attachment(id, &name))??;
    entry_of(&session, &entry)
}

#[tauri::command(async)]
pub fn versions(entry: String, session: Held<'_>) -> Result<Vec<Version>, Failure> {
    let id = dto::entry_id(&entry)?;
    let found = session.with(|vault| vault.versions(id))?;
    Ok(found.iter().map(Version::of).collect())
}

#[tauri::command(async)]
pub fn version(entry: String, index: usize, session: Held<'_>) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    let found = session.with(|vault| vault.version(id, index))?;
    found
        .as_ref()
        .map(Entry::of)
        .ok_or_else(|| Failure::from(vault_core::VaultError::NoSuchVersion))
}

/// Hands one field's value out of a previous version, on the same terms as
/// [`reveal`]: one field, once, and only when the screen asked for it.
#[tauri::command]
pub fn reveal_version(
    entry: String,
    index: usize,
    field: String,
    session: Held<'_>,
) -> Result<Revealed, Failure> {
    let id = dto::entry_id(&entry)?;
    let secret = session.with(|vault| vault.reveal_version(id, index, &field))?;
    let secret = secret.ok_or_else(|| Failure::refused("that version has no such field"))?;
    Ok(Revealed::new(text(&secret)?))
}

#[tauri::command(async)]
pub fn restore_version(entry: String, index: usize, session: Held<'_>) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.restore_version(id, index))??;
    entry_of(&session, &entry)
}

#[tauri::command(async)]
pub fn delete_version(
    entry: String,
    index: usize,
    session: Held<'_>,
) -> Result<Vec<Version>, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.delete_version(id, index))??;
    versions(entry, session)
}

#[tauri::command(async)]
pub fn clear_history(entry: String, session: Held<'_>) -> Result<Vec<Version>, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.clear_history(id))??;
    versions(entry, session)
}

/// Makes a password.
///
/// It comes back the way a revealed value does, because that is what it is: the
/// screen shows it, the reader looks at it, and it goes into a field or it goes
/// nowhere.
#[tauri::command]
pub fn generate_password(
    length: usize,
    alphabets: Vec<String>,
    similar: bool,
) -> Result<Revealed, Failure> {
    let chosen: Vec<Alphabet> = alphabets
        .iter()
        .filter_map(|name| match name.as_str() {
            "lower" => Some(Alphabet::Lower),
            "upper" => Some(Alphabet::Upper),
            "digits" => Some(Alphabet::Digits),
            "symbols" => Some(Alphabet::Symbols),
            _ => None,
        })
        .collect();

    let made = vault_core::generate::password(&Recipe {
        length,
        alphabets: chosen,
        similar,
    })?;
    Ok(Revealed::new(&made))
}

/// Writes the database back.
///
/// A save derives the key again and encrypts the whole file, which is the
/// second of work the unlock screen also pays, so it happens on a thread that
/// is allowed to block.
#[tauri::command]
pub async fn save(session: Held<'_>) -> Result<(), Failure> {
    let session = Arc::clone(&session);
    tauri::async_runtime::spawn_blocking(move || -> Result<(), Failure> {
        session.with_mut(Vault::save)??;
        Ok(())
    })
    .await
    .map_err(|_| Failure::internal("the database could not be written"))?
}

/// Writes the database back over a file somebody else changed.
#[tauri::command]
pub async fn save_over(session: Held<'_>) -> Result<(), Failure> {
    let session = Arc::clone(&session);
    tauri::async_runtime::spawn_blocking(move || -> Result<(), Failure> {
        session.with_mut(Vault::save_over)??;
        Ok(())
    })
    .await
    .map_err(|_| Failure::internal("the database could not be written"))?
}

/// Writes what is in the window to a file of its own, leaving the database
/// alone. The way out of a conflict that keeps both, and the way out of a
/// snapshot.
#[tauri::command]
pub async fn save_copy(app: AppHandle, session: Held<'_>) -> Result<Option<Database>, Failure> {
    let database = session.database().ok_or_else(Failure::no_vault)?;
    let suggested = database
        .file_stem()
        .map(|stem| format!("{} copy.kdbx", stem.to_string_lossy()))
        .unwrap_or_else(|| "vault copy.kdbx".to_owned());

    let mut panel = app
        .dialog()
        .file()
        .set_title("Keep this version as a file of its own")
        .add_filter("KeePass database", &["kdbx"])
        .set_file_name(&suggested);
    if let Some(directory) = database.parent() {
        panel = panel.set_directory(directory);
    }

    let Some(chosen) = panel.blocking_save_file() else {
        return Ok(None);
    };
    let path = chosen
        .into_path()
        .map_err(|_| Failure::refused("that place has no path Coffer can write"))?;

    let writing = Arc::clone(&session);
    let target = path.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), Failure> {
        writing.with_mut(|vault| vault.save_copy(&target))??;
        Ok(())
    })
    .await
    .map_err(|_| Failure::internal("the copy could not be written"))??;

    Ok(Some(Database::of(&path)))
}

/// Throws away what is in the window and reads the file again.
#[tauri::command]
pub async fn reload(session: Held<'_>) -> Result<Group, Failure> {
    let reading = Arc::clone(&session);
    tauri::async_runtime::spawn_blocking(move || -> Result<(), Failure> {
        reading.with_mut(Vault::reload)??;
        Ok(())
    })
    .await
    .map_err(|_| Failure::internal("the database could not be read"))??;

    tree_of(&session)
}

/// What the file on disk holds, for the dialog that asks which version to keep.
///
/// Reading it means opening it, which is another second of key derivation, so
/// it is asked for once, when the dialog appears, rather than on a timer.
#[tauri::command]
pub async fn rival(session: Held<'_>) -> Result<Rival, Failure> {
    let reading = Arc::clone(&session);
    let found = tauri::async_runtime::spawn_blocking(move || reading.with(Vault::rival))
        .await
        .map_err(|_| Failure::internal("the file could not be read"))??;

    Ok(Rival {
        modified: found.modified.and_then(dto::moment),
        entries: found.entries,
    })
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
