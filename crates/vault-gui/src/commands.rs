//! What the webview may ask for.
//!
//! Every command here is thin. It turns what the screen sent into what
//! `vault-core` understands, asks the session, and turns the answer into a DTO.
//! No decision about the database is made in this file.
//!
//! Nearly all of them are `async` on purpose. A plain `#[tauri::command]` runs
//! inline on the thread that handles IPC, which on macOS is the thread that
//! draws the window: key derivation there would freeze the window for a second,
//! and a file dialog there deadlocks, because the panel needs the run loop that
//! the call is blocking.
//!
//! The four that are not are the four that never reach the session: the two
//! that read and write what the reader chose, the one that makes a password out
//! of the machine's randomness, and the one the window sends on every keypress
//! to say somebody is there. That last one is why they stay: it is the most
//! frequent message in the application, it cannot wait on anything, and a hop
//! onto another thread for it would be latency bought with nothing.

use std::path::Path;
use std::sync::Arc;

use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use vault_core::storage::snapshot;
use zeroize::Zeroizing;

use vault_core::generate::{Alphabet, Recipe};
use vault_core::kdf;
use vault_core::{LockPolicy, NewValue, Vault};

use crate::autolock::timer::Timer;
use crate::autolock::{Event, Reason};
use crate::dto::{self, Database, Entry, Group, Made, Revealed, Rival, Snapshot, Status, Version};
use crate::error::Failure;
use crate::session::Session;
use crate::{clipboard, lock, opener, recent, settings, window};

/// The session is behind an `Arc` so that a command can take it onto a blocking
/// thread, which is where key derivation belongs.
///
/// Every command that reaches the session is declared `async`, including the
/// ones that only read. A plain `#[tauri::command]` runs on the thread AppKit
/// draws on, and reaching the session means waiting for whoever holds it: a
/// save holds it for a whole key derivation and then the encryption of the
/// file, which on a vault carrying documents is seconds. A read waiting there
/// is not a slow read. It is the window not answering the mouse.
type Held<'a> = State<'a, Arc<Session>>;

/// What the file panels call a vault. One string, because three panels offer the
/// same filter and a second wording would be a second name for one thing.
const KDBX: &str = "KDBX database";

#[tauri::command(async)]
pub fn status(app: AppHandle, session: Held<'_>) -> Status {
    let (entries, dirty, read_only) = session
        .with(|vault| (vault.count(), vault.is_dirty(), vault.is_read_only()))
        .unwrap_or((0, false, false));

    Status {
        database: session.database().as_deref().map(Database::of),
        key_file: session.key_file().as_deref().map(Database::of),
        unlocked: session.is_unlocked(),
        entries,
        dirty,
        read_only,
        locked_by: session.locked_by().and_then(Reason::explained),
        locks_in: app
            .try_state::<Arc<Timer>>()
            .and_then(|timer| timer.left())
            .map(|left| left.as_secs()),
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
        .set_title("Open a vault")
        .add_filter(KDBX, &["kdbx"]);

    if let Some(directory) = session.database().as_deref().and_then(Path::parent) {
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
pub async fn unlock(
    request: Request<'_>,
    app: AppHandle,
    session: Held<'_>,
) -> Result<(), Failure> {
    opening(request, app, session, LockPolicy::Respect).await
}

/// Opens the chosen database even though somebody's lock file sits beside it.
///
/// A separate command rather than an argument, because the password is the
/// whole body of the message and there is nowhere to put an argument beside it.
///
/// The lock is advisory and the file it names may well be a Mac that lost power
/// with the vault open: the process id in it belongs to a machine that has
/// rebooted since, or to a host whose name has changed, and neither can be
/// proved stale from here. So the reader is shown who holds it and decides. The
/// only alternative was a vault nothing in the application could ever open
/// again.
#[tauri::command]
pub async fn unlock_over(
    request: Request<'_>,
    app: AppHandle,
    session: Held<'_>,
) -> Result<(), Failure> {
    opening(request, app, session, LockPolicy::TakeOver).await
}

async fn opening(
    request: Request<'_>,
    app: AppHandle,
    session: Held<'_>,
    policy: LockPolicy,
) -> Result<(), Failure> {
    let password = password_of(request.body())?;
    let session = Arc::clone(&session);

    // Key derivation is a second of work by design. It happens on a thread that
    // is allowed to block, so the window goes on drawing while it runs.
    tauri::async_runtime::spawn_blocking(move || {
        let opened = session.unlock(password, policy);
        // The stack this thread derived the key on. The heap is the
        // allocator's; the arrays a derivation leaves behind are not.
        vault_core::scrub::stack();
        opened
    })
    .await
    .map_err(|_| Failure::internal("the database could not be opened"))??;

    opened(&app);
    Ok(())
}

/// Asks for the key file some databases need alongside the password.
///
/// Coffer never makes a vault that wants one; this is how a database from
/// KeePassXC or KeePass, whose owner chose one there, can be opened here at all.
/// The path is kept beside the database for the same reason the database's own
/// path is: the password arrives as the whole body of its message and carries
/// nothing beside it.
#[tauri::command]
pub async fn choose_key_file(
    app: AppHandle,
    session: Held<'_>,
) -> Result<Option<Database>, Failure> {
    let mut panel = app
        .dialog()
        .file()
        .set_title("Choose the key file this vault needs");
    if let Some(directory) = session.database().as_deref().and_then(Path::parent) {
        panel = panel.set_directory(directory);
    }

    let Some(chosen) = panel.blocking_pick_file() else {
        return Ok(None);
    };
    let path = chosen
        .into_path()
        .map_err(|_| Failure::refused("that file has no path Coffer can read"))?;

    // Read once here so that a file Coffer cannot read is refused while there is
    // a screen to say so, rather than a second later as a wrong password.
    std::fs::File::open(&path).map_err(Failure::io)?;

    session.use_key_file(Some(path.clone()));
    Ok(Some(Database::of(&path)))
}

/// Takes the key file back off, for a reader who picked the wrong one.
#[tauri::command(async)]
pub fn forget_key_file(session: Held<'_>) {
    session.use_key_file(None);
}

/// Says that a vault is now open, which is what starts the clock that locks it.
///
/// Both ways in go through here. An unlock and a creation are the same event as
/// far as the deadline is concerned, and the one that forgot to say so was a
/// vault nothing could ever lock.
fn opened(app: &AppHandle) {
    if let Some(timer) = app.try_state::<Arc<Timer>>() {
        timer.post(Event::Unlocked);
    }
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

/// Where a first vault goes when nobody has said otherwise.
///
/// Not `~/Documents`: a Mac set up with the default answers synchronises that
/// folder to iCloud, and a vault Coffer put into a sync folder without being
/// asked is the one thing this application says it does not do.
///
/// Nothing is written here. The folder is made at the moment the reader commits,
/// which is where a refusal can still be reported.
#[tauri::command(async)]
pub fn default_new_database(app: AppHandle, session: Held<'_>) -> Result<dto::Database, Failure> {
    let home = app
        .path()
        .home_dir()
        .map_err(|_| Failure::internal("this account has no home directory"))?;
    let path = home.join("Coffer").join("vault.kdbx");

    session.making(path.clone());
    Ok(dto::Database::of(&path))
}

/// Asks where a new vault should go instead, with the system's own save panel.
///
/// The path never comes from the webview here either: the window asks for a
/// panel, the reader chooses, and Coffer keeps the answer. What comes back is
/// only what to draw.
#[tauri::command]
pub async fn choose_new_database(
    app: AppHandle,
    session: Held<'_>,
) -> Result<Option<Database>, Failure> {
    if session.is_unlocked() {
        return Err(Failure::refused("lock the vault before making another"));
    }

    let mut panel = app
        .dialog()
        .file()
        .set_title("Where should the new vault go?")
        .add_filter(KDBX, &["kdbx"])
        .set_file_name("vault.kdbx");
    if let Some(directory) = session.database().as_deref().and_then(Path::parent) {
        panel = panel.set_directory(directory);
    }

    let Some(chosen) = panel.blocking_save_file() else {
        return Ok(None);
    };
    let path = chosen
        .into_path()
        .map_err(|_| Failure::refused("that place has no path Coffer can write"))?;

    session.making(path.clone());
    Ok(Some(Database::of(&path)))
}

/// Measures how many Argon2id passes this machine needs for a one-second
/// unlock, and remembers the answer for the vault about to be made.
///
/// Several derivations, seconds of work, every core busy. It never touches the
/// thread that draws, and it never touches the reader's password: the library
/// that derives keys zeroizes nothing, so a measurement made with the real one
/// would leave a handful of copies of derived material behind.
#[tauri::command]
pub async fn calibrate(session: Held<'_>) -> Result<dto::Calibration, Failure> {
    let session = Arc::clone(&session);
    let found = tauri::async_runtime::spawn_blocking(move || {
        let found = kdf::calibrate(&kdf::Machine);
        vault_core::scrub::stack();
        session.measured(found.work);
        found
    })
    .await
    .map_err(|_| Failure::internal("the key derivation could not be measured"))?;

    Ok(dto::Calibration::of(found))
}

/// Makes the chosen file into a vault and opens it.
///
/// The password arrives as the whole body, exactly as it does for an unlock,
/// which is why where the vault goes and what it costs to open were settled by
/// the two commands above rather than sent alongside it.
#[tauri::command]
pub async fn create_database(
    request: Request<'_>,
    app: AppHandle,
    session: Held<'_>,
) -> Result<(), Failure> {
    let password = password_of(request.body())?;
    let session = Arc::clone(&session);

    tauri::async_runtime::spawn_blocking(move || {
        let made = session.create(password);
        vault_core::scrub::stack();
        made
    })
    .await
    .map_err(|_| Failure::internal("the vault could not be made"))??;

    opened(&app);
    Ok(())
}

/// Wipes the decrypted database out of memory and takes the window down with
/// it, which is what every other way of locking does too.
#[tauri::command(async)]
pub fn lock(app: AppHandle) {
    match app.try_state::<Arc<Timer>>() {
        Some(timer) => timer.post(Event::Locking(Reason::ByHand)),
        // Nothing is keeping a deadline, so there is none to clear.
        None => lock::lock(&app, Reason::ByHand),
    }
}

/// The reader is there.
///
/// Answers with the seconds the open vault has left, so that the bar in the
/// status line counts the clock Rust is counting rather than one of its own.
/// Nothing when no vault is open, which is the honest answer to a message that
/// arrived after a lock.
///
/// The window sends this on real input and at most once every several seconds.
/// It is deliberately not something the countdown itself does: an idle timer
/// that the thing drawing the countdown kept resetting would never fire.
#[tauri::command]
pub fn stirred(app: AppHandle) -> Option<u64> {
    let timer = app.try_state::<Arc<Timer>>()?;
    timer.post(Event::Stirred);
    timer.left().map(|left| left.as_secs())
}

#[tauri::command(async)]
pub fn tree(session: Held<'_>) -> Result<Group, Failure> {
    Ok(Group::of(&session.tree()?))
}

#[tauri::command(async)]
pub fn entry(id: String, session: Held<'_>) -> Result<Entry, Failure> {
    Ok(Entry::of(&session.entry(dto::entry_id(&id)?)?))
}

/// Hands one field's value to the screen. This is the only command that returns
/// a secret, and it returns one field of one entry, once.
#[tauri::command(async)]
pub fn reveal(entry: String, field: String, session: Held<'_>) -> Result<Revealed, Failure> {
    let secret = session.reveal(dto::entry_id(&entry)?, &field)?;
    Ok(Revealed::new(text(&secret)?))
}

/// Copies one field's value to the clipboard. Nothing comes back but the number
/// of seconds until Coffer takes it off again.
#[tauri::command(async)]
pub fn copy(
    entry: String,
    field: String,
    app: AppHandle,
    session: Held<'_>,
) -> Result<u64, Failure> {
    let secret = session.reveal(dto::entry_id(&entry)?, &field)?;
    let after = chosen(&app).clipboard();

    clipboard::copy(text(&secret)?, after);
    Ok(after.as_secs())
}

/// What the reader chose, as the commands see it.
///
/// Resolved out of managed state rather than taken as an argument: the frontend
/// names every argument a command declares, and a settings object arriving from
/// the window is a settings object the window could have made up.
fn chosen(app: &AppHandle) -> settings::Settings {
    app.try_state::<Arc<settings::Preferences>>()
        .map(|held| held.get())
        .unwrap_or_default()
}

/// What the reader chose, and what else they could choose.
#[tauri::command]
pub fn settings(app: AppHandle) -> dto::Settings {
    dto::Settings::of(chosen(&app))
}

/// Puts a choice into effect.
///
/// Answers with what was actually stored rather than with what was sent: a
/// value the screen does not offer is settled onto one it does, and the screen
/// draws what came back.
///
/// A choice that cannot be written down is still in force for this run -
/// `Preferences` decides that, so that a reader who cannot write to their own
/// configuration directory can still change a timer. Which means the two values
/// that need pushing somewhere have to be pushed before the write is reported,
/// or three of the five would apply and two would not, and the two that did not
/// would arrive anyway at the next lock.
#[tauri::command]
pub fn set_settings(settings: dto::Settings, app: AppHandle) -> Result<dto::Settings, Failure> {
    let held = app
        .try_state::<Arc<settings::Preferences>>()
        .ok_or_else(|| Failure::internal("this Mac has nowhere to keep a setting"))?;

    let written = held.set(settings.wanted());
    let stored = held.get();

    // A shorter timeout applies to the vault that is open now, not to the next
    // one. Shortening it past what has already gone locks immediately, which is
    // what a reader who just chose one minute is asking for.
    if let Some(timer) = app.try_state::<Arc<Timer>>() {
        timer.post(Event::TimeoutChanged(stored.idle()));
    }

    // The open vault stays open. Only the appearance changes, and AppKit carries
    // it into the webview, where `prefers-color-scheme` is what the screen reads
    // for a reader who asked for the Mac's own.
    window::retune(&app, stored.theme);

    written.map_err(Failure::io)?;
    Ok(dto::Settings::of(stored))
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
#[tauri::command(async)]
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
#[tauri::command(async)]
pub fn snapshots(session: Held<'_>) -> Result<Vec<Snapshot>, Failure> {
    let database = session.database().ok_or_else(Failure::no_vault)?;
    let taken = snapshot::taken(&database).map_err(Failure::io)?;
    Ok(taken.iter().map(Snapshot::of).collect())
}

/// Points the session at one of those snapshots.
///
/// The index is all the webview sends; the path is built here from the database
/// the user chose, so no message from the screen can name a file.
#[tauri::command(async)]
pub fn choose_snapshot(index: u32, session: Held<'_>) -> Result<Database, Failure> {
    if session.is_unlocked() {
        return Err(Failure::refused("lock the vault before opening another"));
    }

    let database = session.database().ok_or_else(Failure::no_vault)?;
    let path = snapshot::slot(&database, index).map_err(Failure::io)?;

    if !path.is_file() {
        return Err(Failure::gone());
    }

    session.choose_snapshot(path.clone());
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

/// Takes a file off an entry along with the previous versions holding it back.
///
/// One command rather than two, because the two have to happen together or not
/// at all: the versions are put back if the file still cannot go.
#[tauri::command(async)]
pub fn remove_attachment_and_versions(
    entry: String,
    name: String,
    session: Held<'_>,
) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.remove_attachment_and_versions(id, &name))??;
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
#[tauri::command(async)]
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
        .add_filter(KDBX, &["kdbx"])
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

    /// Every way a vault can come to be open has to start the clock that locks
    /// it. There are two, and the second one forgot: a vault made through the
    /// creation screen armed nothing and could not be locked by any route.
    ///
    /// Read out of this file's own source, the way `contract.test.ts` reads it
    /// from the other side. Nothing else connects the two, and a third way in
    /// would be silent.
    /// Every function in this file, cut at each `fn` that begins a line.
    ///
    /// Coarse on purpose. A helper the commands share is a function of its own
    /// here, which is what the check below needs: two commands now open a vault
    /// through one door, and the door is where the clock has to be started.
    fn functions(source: &str) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();

        for line in source.lines() {
            let head = line.trim_start();
            let starts = ["fn ", "pub fn ", "async fn ", "pub async fn "]
                .iter()
                .any(|shape| head.starts_with(shape));

            if starts || found.is_empty() {
                found.push(String::new());
            }
            if let Some(body) = found.last_mut() {
                body.push_str(line);
                body.push('\n');
            }
        }

        found
    }

    #[test]
    fn every_way_a_vault_comes_to_be_open_starts_the_clock() {
        // Everything above the test module: this test's own source names the
        // two calls it is looking for, and would count itself.
        let source = include_str!("commands.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();

        let opens: Vec<String> = functions(source)
            .into_iter()
            .filter(|body| body.contains("session.unlock(") || body.contains("session.create("))
            .collect();

        assert_eq!(
            opens.len(),
            2,
            "there are two ways a vault comes to be open: an unlock and a creation"
        );
        for body in opens {
            assert!(
                body.contains("opened(&app)"),
                "a vault is opened without starting the clock that locks it:\n{}",
                body.lines().take(3).collect::<Vec<_>>().join("\n")
            );
        }
    }

    /// Every command answered inline, from its attribute down to the brace that
    /// opens it. Nothing but attributes sits between the two, so the first brace
    /// is the end of the signature.
    ///
    /// Two spellings take a command off this thread and only one of them is the
    /// attribute: an `async fn` is spawned whether or not the attribute says so,
    /// which is why the signature is read as well.
    fn drawn_on_the_window_thread(source: &str) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        let mut taking: Option<String> = None;

        for line in source.lines() {
            if line.trim() == "#[tauri::command]" {
                taking = Some(String::new());
                continue;
            }
            let Some(head) = taking.as_mut() else {
                continue;
            };
            head.push_str(line);
            head.push('\n');
            if line.contains('{') {
                found.extend(taking.take().filter(|head| !head.contains("async fn")));
            }
        }

        found
    }

    /// A command without `(async)` is answered inline on the thread AppKit
    /// draws on. Waiting for the session there is not a slow answer, it is a
    /// window that stops taking the mouse - and a save holds the session for a
    /// key derivation and the encryption of the whole file, which on a vault
    /// carrying documents is seconds rather than the instant a read is.
    ///
    /// Read out of this file's own source, because there is no way to ask a
    /// running command which thread it is on, and a new command added the
    /// obvious way would be the one that freezes.
    #[test]
    fn nothing_that_waits_for_the_session_runs_where_the_window_is_drawn() {
        let source = include_str!("commands.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();

        for head in drawn_on_the_window_thread(source) {
            assert!(
                !head.contains("Held<"),
                "this command reaches the session from the drawing thread:\n{head}"
            );
        }

        // The one that reaches it without naming it. Locking wipes the tree,
        // which means taking the same mutex a save is holding.
        assert!(
            source.contains("#[tauri::command(async)]\npub fn lock("),
            "the lock command reaches the session and must not be answered inline"
        );
    }

    /// A lock file is taken over only where a reader can have been shown who
    /// holds it. Anything else deciding that on their behalf would make the lock
    /// worth nothing.
    #[test]
    fn only_the_command_a_reader_presses_takes_a_lock_over() {
        let source = include_str!("commands.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();

        let taking: Vec<String> = functions(source)
            .into_iter()
            .filter(|body| body.contains("LockPolicy::TakeOver"))
            .collect();

        assert_eq!(taking.len(), 1, "more than one way into a locked database");
        assert!(
            taking
                .first()
                .is_some_and(|body| body.contains("fn unlock_over")),
            "something other than the reader's own press takes a lock over"
        );
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
