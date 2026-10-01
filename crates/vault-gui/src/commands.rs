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
//! The two that are not are the two that reach neither the session, the lock
//! nor the disk: the one that reads what the reader chose, and the one that
//! reads what the generator was last asked for. Neither can wait on anything,
//! and a hop onto another thread for them would be latency bought with
//! nothing. Making a password is not one of them any more: it writes down the
//! recipe it was made from.
//!
//! The message the window sends to say somebody is there is not one of them,
//! however often it comes. A stir that finds the time already spent - the first
//! key pressed after a Mac slept through the deadline - locks the vault, and a
//! lock runs on the thread that asked for it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::ipc::{Channel, InvokeBody, Request};
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use vault_core::storage::{self, snapshot, unsaved};
use zeroize::Zeroizing;

use vault_core::generate::Recipe;
use vault_core::kdf;
use vault_core::{LockPolicy, NewValue, Typing, Vault};

use crate::autolock::timer::Timer;
use crate::autolock::{Event, Reason};
use crate::drafts::{Over, Typed};
use crate::dto::{
    self, Action, Database, Entry, Group, Made, Revealed, Rival, Snapshot, Status, Target, Versions,
};
use crate::error::Failure;
use crate::home::Standing;
use crate::menu::{self, Command};
use crate::route::Route;
use crate::session::Session;
use crate::{clipboard, closing, generator, home, lock, opener, settings, window};

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
    let (entries, read_only) = session
        .with(|vault| (vault.count(), vault.is_read_only()))
        .unwrap_or((0, false));
    let database = session.database();

    Status {
        found: looked_home(&session, home_of(&app).ok().as_deref())
            .as_deref()
            .map(dto::Found::of),
        // Asked of the filesystem rather than remembered, so that a copy left
        // by a run that has since quit is still offered. A copy that cannot be
        // read is one Coffer does not offer, and the rest of the answer still
        // has to come back.
        rescue: database
            .as_deref()
            .and_then(|path| unsaved::found(path).ok().flatten())
            .as_ref()
            .map(dto::Rescued::of),
        lost: session.lost(),
        // Read off the disk each time as well: what a lock left behind is
        // what the reader may have moved since.
        file: database
            .as_deref()
            .map(|chosen| dto::OnDisk::of(storage::on_disk(chosen))),
        // What is said here about the vault's file is what "Make this my
        // vault" is then held to, so it is the session that reads it.
        copy: database.as_deref().and_then(|copy| {
            let (vault, vault_file) = session.telling()?;
            Some(dto::CopyOf::of(&vault, storage::on_disk(copy), vault_file))
        }),
        typed: session.typed(),
        typed_beside: session.typed_beside(),
        database: database.as_deref().map(Database::of),
        key_file: session.key_file().as_deref().map(Database::of),
        unlocked: session.is_unlocked(),
        entries,
        read_only,
        locked_by: session.locked_by().and_then(Reason::explained),
        locks_in: app
            .try_state::<Arc<Timer>>()
            .and_then(|timer| timer.left())
            .map(|left| left.as_secs()),
    }
}

/// The vault in Coffer's own folder, for the first-run screen to offer, kept
/// in the session so that the offer opens that file and no other.
///
/// Looked for only when nothing is remembered, which is the launch that would
/// otherwise greet somebody with a vault as somebody with none.
fn looked_home(session: &Session, home: Option<&Path>) -> Option<PathBuf> {
    let found = session
        .database()
        .is_none()
        .then_some(home)
        .flatten()
        .and_then(home::found);
    session.finding(found.clone());
    found
}

/// Asks for a database with the system's own file dialog.
///
/// The path never comes from the webview: the webview asks for a picker, the
/// user picks, and Coffer keeps the answer. Nothing the frontend sends can
/// point Coffer at a file.
///
/// Nothing is written down for the next launch here. A file that was picked is
/// not yet a vault that opened, and the session writes one down when it does.
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

    let home = home_of(&app).ok();
    if let Some(directory) = home::opening_in(session.database().as_deref(), home.as_deref()) {
        picker = picker.set_directory(directory);
    }

    if session.is_unlocked() {
        return Err(Failure::lock_first());
    }

    let Some(chosen) = picker.blocking_pick_file() else {
        return Ok(None);
    };

    let path = chosen
        .into_path()
        .map_err(|_| Failure::refused("that file has no path Coffer can open"))?;

    session.choose(path.clone());
    Ok(Some(Database::of(&path)))
}

/// Points the session at the vault found in Coffer's own folder.
///
/// Nothing is sent: the file is the one `status` last named, which the session
/// is holding, so no message from the window can name a file. It is asked
/// again by the rule the search used, and one that went away or stopped being
/// a vault after the screen was drawn is answered with `gone` rather than
/// opened, or swapped for another file the folder holds.
#[tauri::command(async)]
pub fn choose_found(session: Held<'_>) -> Result<Database, Failure> {
    found_chosen(&session)
}

fn found_chosen(session: &Session) -> Result<Database, Failure> {
    if session.is_unlocked() {
        return Err(Failure::lock_first());
    }

    let path = session
        .found()
        .filter(|path| home::offered(path))
        .ok_or_else(Failure::gone)?;
    Ok(chosen_now(session, path))
}

/// Points the session at whatever already sits where the new vault would go,
/// for a reader who meant to open it rather than make another.
///
/// Nothing is sent here either. The place is the one `default_new_database` or
/// `choose_new_database` settled, which the session is still holding. Only a
/// vault, or the copy a lock left of one whose file has gone, is opened: the
/// unlock screen for that name is where such a copy is put back. Anything else
/// is answered with `gone`, and the screen reads the place again with `target`.
#[tauri::command(async)]
pub fn choose_existing(session: Held<'_>) -> Result<Database, Failure> {
    existing_chosen(&session)
}

fn existing_chosen(session: &Session) -> Result<Database, Failure> {
    if session.is_unlocked() {
        return Err(Failure::lock_first());
    }

    let target = session.target().ok_or_else(Failure::nowhere_chosen)?;
    match home::standing(&target) {
        Standing::Vault | Standing::Copy => Ok(chosen_now(session, target)),
        Standing::Free | Standing::Empty | Standing::Other => Err(Failure::gone()),
    }
}

/// Points the session at a file Rust found for itself, and answers with what
/// the session now holds: the file, with the links on the way to it followed.
fn chosen_now(session: &Session, path: PathBuf) -> Database {
    session.choose(path.clone());
    Database::of(&session.database().unwrap_or(path))
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

/// Where a first vault goes when nobody has said otherwise: see [`home`].
///
/// Nothing is written here. The folder is made at the moment the reader commits,
/// which is where a refusal can still be reported. Whether something is there
/// already is said now, though, before a password is typed for a vault that
/// could never be made.
#[tauri::command(async)]
pub fn default_new_database(app: AppHandle, session: Held<'_>) -> Result<Target, Failure> {
    let home = home_of(&app)?;
    let path = home::first(&home);

    session.making(path.clone());
    Ok(target_of(&path, Some(&home)))
}

/// What is at the place a new vault would go, read again: after the offer to
/// open what was there found something else, and after a creation found the
/// place taken. The place stays the one the session holds.
#[tauri::command(async)]
pub fn target(app: AppHandle, session: Held<'_>) -> Result<Target, Failure> {
    let path = session.target().ok_or_else(Failure::nowhere_chosen)?;
    Ok(target_of(&path, home_of(&app).ok().as_deref()))
}

/// The reader's home folder, which only the account can lack.
fn home_of(app: &AppHandle) -> Result<PathBuf, Failure> {
    app.path()
        .home_dir()
        .map_err(|_| Failure::internal("this account has no home directory"))
}

/// A place for a new vault, as the creation screen draws it.
fn target_of(path: &Path, home: Option<&Path>) -> Target {
    Target {
        shown: home::shown(path, home),
        standing: home::standing(path),
    }
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
) -> Result<Option<Target>, Failure> {
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

    // The panel asks whether to replace a file that is there, and a reader who
    // says yes is still refused: a creation takes no snapshot, so what was
    // there would be gone. The screen is told now rather than after the
    // password.
    session.making(path.clone());
    Ok(Some(target_of(&path, home_of(&app).ok().as_deref())))
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
    by_hand(&app);
}

/// The lock a reader's press asks for, through the timer so that the deadline
/// it was keeping is cleared by the same message that takes the window down.
fn by_hand(app: &AppHandle) {
    match app.try_state::<Arc<Timer>>() {
        Some(timer) => timer.post(Event::Locking(Reason::ByHand)),
        // Nothing is keeping a deadline, so there is none to clear.
        None => lock::lock(app, Reason::ByHand),
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
///
/// A stir does not always start the clock again. One that arrives after the
/// time has run out - the first key after a Mac slept through the deadline,
/// before the watcher has woken - locks the vault instead, and nothing comes
/// back because nothing is open. The lock runs right here, on the thread that
/// posted: it writes the vault out, which is a key derivation, and destroys the
/// window, so this is answered off the thread the window is drawn on.
#[tauri::command(async)]
pub fn stirred(app: AppHandle) -> Option<u64> {
    let timer = app.try_state::<Arc<Timer>>()?;
    timer.post(Event::Stirred);
    timer.left().map(|left| left.as_secs())
}

/// Hands Rust the page's way in: what the reader chooses in the menu bar, and
/// the close button, arrive through this channel (see `route.rs`). One per
/// window; the next window's page replaces it, and Rust lets go of it when the
/// window goes.
#[tauri::command(async)]
pub fn listen(channel: Channel<Action>, window: WebviewWindow) {
    if let Some(route) = window.try_state::<Route>() {
        route.listen(window.label(), channel);
    }
}

/// Which of Coffer's items in the menu bar the page's screens can do now, for
/// the bar to grey out the rest.
///
/// Locking and opening another vault follow the session as well: a bar that
/// offered Open Vault… over an open vault would offer a choice Rust refuses.
/// Whether one is open is read without waiting for the session, which a save
/// holds for seconds: a report stuck behind it would leave an item grey that
/// applies, and AppKit drops the key of a grey item. The items are AppKit's,
/// and are changed on the thread the window is drawn on by a message posted
/// there rather than waited for. A report from a page that is no longer the
/// one listening is dropped: it can arrive after the next window's page has
/// said what it offers.
#[tauri::command(async)]
pub fn menu_state(enabled: Vec<Command>, window: WebviewWindow, session: Held<'_>) {
    let unlocked = session.is_unlocked();
    let said: Vec<Command> = enabled
        .into_iter()
        .filter(|command| command.allowed(unlocked))
        .collect();
    let app = window.app_handle().clone();
    let label = window.label().to_owned();
    let _ = window.run_on_main_thread(move || {
        if app
            .try_state::<Route>()
            .is_some_and(|route| route.hears(&label))
        {
            menu::enable(&app, Some(&said));
        }
    });
}

/// Locks the vault and takes the window down without building it again,
/// because the reader closed it. Asked by the page once what was being typed
/// has reached Rust; Coffer then waits in the Dock.
#[tauri::command(async)]
pub fn close_window(window: WebviewWindow) {
    closing::lock(window.app_handle());
    // The lock takes the window down when a vault was open; with none open it
    // still has to go.
    let _ = window.destroy();
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

/// Copies one field's value to the clipboard, or the part of it the reader
/// selected on the screen. Nothing comes back but the number of seconds until
/// Coffer takes it off again.
#[tauri::command(async)]
pub fn copy(
    entry: String,
    field: String,
    range: Option<dto::Span>,
    app: AppHandle,
    session: Held<'_>,
) -> Result<u64, Failure> {
    let secret = session.reveal(dto::entry_id(&entry)?, &field)?;
    copied(&app, &secret, range)
}

/// Copies one field's value out of a previous version, on the same terms as
/// [`copy`]. A version is read on the screen like the entry is, and a value
/// selected there is as much a secret as one selected in the entry.
#[tauri::command(async)]
pub fn copy_version(
    entry: String,
    index: usize,
    revision: u64,
    field: String,
    range: Option<dto::Span>,
    app: AppHandle,
    session: Held<'_>,
) -> Result<u64, Failure> {
    let secret = session.reveal_version(dto::entry_id(&entry)?, index, revision, &field)?;
    copied(&app, &secret, range)
}

/// Puts a value on the pasteboard, whole or the part that was asked for, and
/// answers with how long it stays there.
fn copied(
    app: &AppHandle,
    secret: &vault_core::SecretValue,
    range: Option<dto::Span>,
) -> Result<u64, Failure> {
    let after = chosen(app).clipboard();
    match range {
        None => clipboard::copy(text(secret)?, after),
        Some(span) => clipboard::copy(text(&secret.part(span.from, span.to)?)?, after),
    }
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
#[tauri::command(async)]
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
        return Err(Failure::lock_first());
    }

    let database = session.database().ok_or_else(Failure::no_vault)?;
    let path = snapshot::slot(&database, index).map_err(Failure::io)?;

    if !path.is_file() {
        return Err(Failure::gone());
    }

    session.choose_sibling(path.clone());
    Ok(Database::of(&path))
}

/// Points the session at the unsaved copy a lock left beside the database.
///
/// Nothing is sent: the path is built here from the database the reader chose,
/// so no message from the window can name a file. The same shape as
/// `choose_snapshot`, for the same reason.
#[tauri::command(async)]
pub fn choose_rescue(session: Held<'_>) -> Result<Database, Failure> {
    if session.is_unlocked() {
        return Err(Failure::lock_first());
    }

    let database = session.database().ok_or_else(Failure::no_vault)?;
    let path = unsaved::beside(&database).map_err(Failure::io)?;

    if !path.is_file() {
        return Err(Failure::gone());
    }

    session.choose_sibling(path.clone());
    Ok(Database::of(&path))
}

/// Takes that copy off the disk, and the snapshots saves inside it left
/// beside it (see [`unsaved::discard`]).
///
/// Only ever from a press. The copy holds the one version of work the vault has
/// not got, so nothing in Coffer removes it on its own initiative: the reader is
/// shown what it is and decides when they are done with it.
#[tauri::command(async)]
pub fn discard_rescue(session: Held<'_>) -> Result<(), Failure> {
    if session.is_unlocked() {
        return Err(Failure::lock_first());
    }

    let database = session.database().ok_or_else(Failure::no_vault)?;
    unsaved::discard(&database).map_err(Failure::io)
}

/// Moves the copy a lock left into the name of a vault whose file has gone:
/// see [`unsaved::put_back`].
///
/// Nothing is sent and no password is asked for. The copy is moved rather than
/// opened, and only into a name that holds nothing, so a vault that is there -
/// or came back since the screen was drawn - is answered with `taken` and left
/// alone.
#[tauri::command(async)]
pub fn put_back_rescue(session: Held<'_>) -> Result<Database, Failure> {
    if session.is_unlocked() {
        return Err(Failure::lock_first());
    }

    let database = session.database().ok_or_else(Failure::no_vault)?;
    unsaved::put_back(&database)?;
    Ok(Database::of(&database))
}

/// Makes the open copy a lock left the vault it was taken from, with the
/// vault's file as it stood kept as the newest snapshot: see
/// [`Vault::promote`]. Answers with the vault, which is what is open now.
///
/// A save's worth of work - a key derivation and the whole file encrypted - so
/// it happens on a thread that is allowed to block.
#[tauri::command]
pub async fn promote_rescue(session: Held<'_>) -> Result<Database, Failure> {
    let session = Arc::clone(&session);
    let vault = tauri::async_runtime::spawn_blocking(move || session.promote())
        .await
        .map_err(|_| Failure::internal("the copy could not be made the vault"))??;

    Ok(Database::of(&vault))
}

/// Goes back from the copy a lock left to the vault it was taken from, and
/// answers with what is chosen afterwards.
///
/// A copy that is open is locked on the way, which is how any open vault is
/// left: what it holds is written out into the copy, its window goes, and the
/// window that comes back asks for the vault's password - or for the copy's,
/// when the lock had to keep the copy's work somewhere else or lost it, so
/// that the screen saying so is the one about the copy (see
/// [`Session::back_to_vault`]). A copy that is only chosen is simply not
/// chosen any more, and nothing is locked.
#[tauri::command(async)]
pub fn leave_rescue(app: AppHandle, session: Held<'_>) -> Result<Database, Failure> {
    session.back_to_vault()?;
    by_hand(&app);
    let chosen = session.database().ok_or_else(Failure::no_vault)?;
    Ok(Database::of(&chosen))
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

/// Deletes an entry, and lets go of anything typed into it that the window
/// said before it asked: the pane it was typed in goes with the entry.
///
/// `deletion` is what the window showed the deletion would do. Two deletions
/// can wait behind one save, and the thread that takes the session first is
/// not the one that asked first, so an entry whose folder went into the bin
/// ahead of it is refused with `deletionChanged` rather than erased.
#[tauri::command(async)]
pub fn delete_entry(
    entry: String,
    deletion: dto::Deletion,
    sequence: u64,
    session: Held<'_>,
) -> Result<Group, Failure> {
    let id = dto::entry_id(&entry)?;
    session.overtaking(Over::Entry(id), sequence, |vault| {
        vault.delete_entry(id, deletion.shown())
    })??;
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

/// Deletes a folder, on the terms [`delete_entry`] gives.
#[tauri::command(async)]
pub fn delete_group(
    group: String,
    deletion: dto::Deletion,
    session: Held<'_>,
) -> Result<Group, Failure> {
    let group = dto::group_id(&group)?;
    session.with_mut(|vault| vault.delete_group(group, deletion.shown()))??;
    tree_of(&session)
}

/// Takes an entry out of the recycle bin, back to the folder it was deleted
/// from, or to the top of the vault when that folder is nowhere to go. The
/// undo of a move to the bin, and the bin's own way out.
#[tauri::command(async)]
pub fn put_back_entry(entry: String, session: Held<'_>) -> Result<Group, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.put_back_entry(id))??;
    tree_of(&session)
}

/// Takes a folder out of the recycle bin with everything in it, on the same
/// terms.
#[tauri::command(async)]
pub fn put_back_group(group: String, session: Held<'_>) -> Result<Group, Failure> {
    let group = dto::group_id(&group)?;
    session.with_mut(|vault| vault.put_back_group(group))??;
    tree_of(&session)
}

#[tauri::command(async)]
pub fn empty_recycle_bin(session: Held<'_>) -> Result<Group, Failure> {
    session.with_mut(vault_core::Vault::empty_recycle_bin)??;
    tree_of(&session)
}

/// Writes one field of one entry.
///
/// `protect` is how a field the entry does not have yet is made. One it has
/// keeps the protection it has, whatever the screen read before the value was
/// sent: [`set_protection`] is the one way to change it, and a value written on
/// the way out of a field the reader had just hidden would otherwise put it
/// back into the file as plain text.
///
/// `sequence` is the window's number for the write, from the count its drafts
/// carry (see [`draft`]). A draft of this field said before it is finished by
/// it, and is let go under the same lock.
#[tauri::command(async)]
pub fn set_field(
    entry: String,
    field: String,
    value: String,
    protect: bool,
    sequence: u64,
    session: Held<'_>,
) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    let written = if protect {
        NewValue::Protected(Zeroizing::new(value))
    } else {
        NewValue::Open(value)
    };

    session.overtaking(Over::Field(id, &field), sequence, |vault| {
        vault.set_field(id, &field, written)
    })??;
    entry_of(&session, &entry)
}

/// Holds what the reader is typing into a field and has not finished, so that
/// a lock can write it.
///
/// A value is written when its field is left, and until then it was only in
/// the window, which a lock destroys - on triggers that arrive on the thread
/// the window is drawn on and cannot wait for the page to answer. So the window
/// sends what is in the field a moment after the last key, and at once when it
/// loses focus, and the lock writes the last of it into the vault through the
/// same edit a commit makes. `value` is nothing when the typing was taken back:
/// Escape, Cancel, Discard, or a field left as it was.
///
/// `beside` is true for a new value typed in a Change field of its own rather
/// than into the field itself. The reader has not saved it, so a lock keeps it
/// beside the value it was for and never writes it over one.
///
/// The text travels the way `set_field`'s does and is wrapped the moment it
/// arrives. `sequence` only goes up in any one window; a word about a field
/// that is not newer than the last one heard about the field, its entry or
/// the whole vault is dropped, because Tauri runs these side by side and a
/// draft sent before a commit can arrive after it.
#[tauri::command(async)]
pub fn draft(
    entry: String,
    field: String,
    value: Option<String>,
    protect: bool,
    beside: bool,
    sequence: u64,
    session: Held<'_>,
) -> Result<(), Failure> {
    let typed = value.map(|value| Typed {
        value: Zeroizing::new(value),
        protect,
        typing: if beside {
            Typing::Beside
        } else {
            Typing::InPlace
        },
    });
    session.draft(dto::entry_id(&entry)?, &field, typed, sequence)
}

/// Takes a field of the reader's own off an entry.
///
/// Refused with `forGood` when the vault's limits leave no version to bring
/// the field back from, until the window sends `forever` to say the reader
/// agreed: see [`vault_core::Vault::remove_field`]. The question and the
/// removal are one call, so the answer the reader gave is about the entry the
/// removal acts on.
#[tauri::command(async)]
pub fn remove_field(
    entry: String,
    field: String,
    forever: bool,
    session: Held<'_>,
) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.remove_field(id, &field, forever))??;
    entry_of(&session, &entry)
}

/// Hides a field of the reader's own, or stops hiding it.
///
/// Nothing of the value crosses: the window sends the field's name and what it
/// should be, and the value moves from one kind of storage to the other inside
/// the vault. See [`vault_core::Vault::set_protection`].
#[tauri::command(async)]
pub fn set_protection(
    entry: String,
    field: String,
    protect: bool,
    session: Held<'_>,
) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.set_protection(id, &field, protect))??;
    entry_of(&session, &entry)
}

/// Gives a field of the reader's own another name, its value and protection
/// going with it inside the vault. See [`vault_core::Vault::rename_field`].
#[tauri::command(async)]
pub fn rename_field(
    entry: String,
    from: String,
    to: String,
    session: Held<'_>,
) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.rename_field(id, &from, &to))??;
    entry_of(&session, &entry)
}

/// Puts back a field that just came off, when its removal is still the last
/// thing that happened to the entry, and refuses with `superseded` when it is
/// not. The window sends the field's name and nothing else: which version puts
/// it back is decided under the same lock the restore runs in. See
/// [`vault_core::Vault::undo_removal`].
#[tauri::command(async)]
pub fn undo_removal(entry: String, field: String, session: Held<'_>) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.undo_removal(id, &field))??;
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
///
/// Nothing comes back when the reader closed the panel without choosing, the
/// way it does from every other panel here. An entry handed back unchanged
/// reads to the window as a change, and what the window does with a change is
/// write the vault: a press of Escape cost a key derivation, the re-encryption
/// of every file in the database, and one of the ten snapshots - the oldest,
/// pushed off the end of the only chain that leads back to a version of the
/// vault from an hour ago.
///
/// A name the entry already gives a file is not written over. Nothing changes,
/// the file just read is held beside the vault, and the window is told what is
/// there so that it can ask; the answer is one of the three commands below, and
/// the reader never has to find the file again to give it.
#[tauri::command]
pub async fn add_attachment(
    app: AppHandle,
    entry: String,
    session: Held<'_>,
) -> Result<Option<dto::Attached>, Failure> {
    let id = dto::entry_id(&entry)?;

    // A press of the button is a new question, so the last one about this entry
    // is let go before the panel opens: a panel closed without a choice leaves
    // nothing waiting that the window has stopped asking about.
    session.withdraw(id);

    let Some(chosen) = app
        .dialog()
        .file()
        .set_title("Add a file to this entry")
        .blocking_pick_file()
    else {
        return Ok(None);
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
    let chosen = data.len();

    match session.offer(id, name.clone(), data)? {
        vault_core::Attached::Added => Ok(Some(dto::Attached::Added {
            entry: entry_of(&session, &entry)?,
        })),
        vault_core::Attached::Taken(clash) => Ok(Some(dto::Attached::Taken {
            clash: dto::Clash::of(name, chosen, clash),
        })),
    }
}

/// Keeps both: the file waiting on this entry goes on beside the one that has
/// its name, under the next name free.
#[tauri::command(async)]
pub fn keep_both_attachments(entry: String, session: Held<'_>) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    session.answer(id, Vault::keep_both)?;
    entry_of(&session, &entry)
}

/// Puts the file waiting on this entry in place of the one that has its name.
///
/// Refused on the same terms as a removal, because it is one: while earlier
/// versions hold the file that is there, it stays, and so does the one waiting.
#[tauri::command(async)]
pub fn replace_attachment(entry: String, session: Held<'_>) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    session.answer(id, Vault::replace_attachment)?;
    entry_of(&session, &entry)
}

/// Lets go of the file waiting on this entry: the reader said not to add it,
/// or stopped looking at the entry it was chosen for.
#[tauri::command(async)]
pub fn withdraw_attachment(entry: String, session: Held<'_>) -> Result<(), Failure> {
    session.withdraw(dto::entry_id(&entry)?);
    Ok(())
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

/// The previous versions of an entry, with the revision of the vault they
/// were listed at.
///
/// Every command below that names a version by its position takes that
/// revision back, and refuses with `versionsChanged` when the vault has
/// changed since: see [`Session::at`].
#[tauri::command(async)]
pub fn versions(entry: String, session: Held<'_>) -> Result<Versions, Failure> {
    let id = dto::entry_id(&entry)?;
    let (revision, found) = session.listing(|vault| vault.versions(id))?;
    Ok(Versions::of(revision, &found))
}

#[tauri::command(async)]
pub fn version(
    entry: String,
    index: usize,
    revision: u64,
    session: Held<'_>,
) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    let found = session.at(revision, |vault| vault.version(id, index))?;
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
    revision: u64,
    field: String,
    session: Held<'_>,
) -> Result<Revealed, Failure> {
    let secret = session.reveal_version(dto::entry_id(&entry)?, index, revision, &field)?;
    Ok(Revealed::new(text(&secret)?))
}

#[tauri::command(async)]
pub fn restore_version(
    entry: String,
    index: usize,
    revision: u64,
    session: Held<'_>,
) -> Result<Entry, Failure> {
    let id = dto::entry_id(&entry)?;
    session.at_mut(revision, |vault| vault.restore_version(id, index))??;
    entry_of(&session, &entry)
}

#[tauri::command(async)]
pub fn delete_version(
    entry: String,
    index: usize,
    revision: u64,
    session: Held<'_>,
) -> Result<Versions, Failure> {
    let id = dto::entry_id(&entry)?;
    session.at_mut(revision, |vault| vault.delete_version(id, index))??;
    versions(entry, session)
}

#[tauri::command(async)]
pub fn clear_history(entry: String, session: Held<'_>) -> Result<Versions, Failure> {
    let id = dto::entry_id(&entry)?;
    session.with_mut(|vault| vault.clear_history(id))??;
    versions(entry, session)
}

/// The recipe a generator opens with - the last one it made a password from -
/// and what the window draws around it.
#[tauri::command]
pub fn generator(purpose: dto::Purpose, app: AppHandle) -> dto::Generator {
    dto::Generator::of(&remembered(&app, purpose))
}

/// Makes a password, and remembers what it was made from for the generator
/// that asked.
///
/// It comes back the way a revealed value does, because that is what it is: the
/// screen shows it, the reader looks at it, and it goes into a field or it goes
/// nowhere. With it come the kinds of character it was asked for and happens to
/// lack, and the generator as the recipe now stands: the recipe is settled
/// first, so a length the slider cannot show is never the one used.
///
/// Off the drawing thread, because remembering the recipe is a write to disk.
#[tauri::command(async)]
pub fn generate_password(
    recipe: dto::Recipe,
    purpose: dto::Purpose,
    app: AppHandle,
) -> Result<dto::Generated, Failure> {
    let recipe = recipe.recipe().settled();
    let made = vault_core::generate::password(&recipe)?;

    if let Some(held) = app.try_state::<Arc<generator::Remembered>>() {
        // A recipe that could not be written down is still held for as long as
        // Coffer runs. A password nobody can have because a settings file
        // would not write is a worse answer than one the next launch forgets
        // how it was made.
        let _ = held.keep(purpose, &recipe);
    }
    Ok(dto::Generated::of(&made, &recipe))
}

/// The recipe a generator last made a password from, as Rust holds it.
fn remembered(app: &AppHandle, purpose: dto::Purpose) -> Recipe {
    app.try_state::<Arc<generator::Remembered>>()
        .map(|held| held.recipe(purpose))
        .unwrap_or_default()
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

/// Throws away what is in the window and reads the file again, typing that
/// was never finished included.
#[tauri::command]
pub async fn reload(sequence: u64, session: Held<'_>) -> Result<Group, Failure> {
    let reading = Arc::clone(&session);
    tauri::async_runtime::spawn_blocking(move || -> Result<(), Failure> {
        reading.overtaking(Over::Everything, sequence, Vault::reload)??;
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
    use std::os::unix::fs::symlink;

    use super::*;
    use crate::source::{functions, shipped};

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
    #[test]
    fn every_way_a_vault_comes_to_be_open_starts_the_clock() {
        let source = shipped(include_str!("commands.rs"));

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

    /// Every command answered inline, whole: from its attribute down to the
    /// closing brace in the first column, which in formatted source ends the
    /// item and nothing else.
    ///
    /// The body as well as the signature, because a command can reach the
    /// session without ever naming it. Two spellings take a command off this
    /// thread and only one of them is the attribute: an `async fn` is spawned
    /// whether or not the attribute says so, so the signature is read too.
    fn drawn_on_the_window_thread(source: &str) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        let mut taking: Option<String> = None;

        for line in source.lines() {
            if line.trim() == "#[tauri::command]" {
                taking = Some(String::new());
                continue;
            }
            let Some(item) = taking.as_mut() else {
                continue;
            };
            item.push_str(line);
            item.push('\n');
            if line == "}" {
                found.extend(taking.take().filter(|item| !item.contains("async fn")));
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
        let source = shipped(include_str!("commands.rs"));

        for item in drawn_on_the_window_thread(source) {
            let named = item.lines().take(4).collect::<Vec<_>>().join("\n");
            assert!(
                !item.contains("Held<"),
                "this command reaches the session from the drawing thread:\n{named}"
            );
            // Posting anything to the timer is reaching the lock.
            // `Deadline::on` answers every event but an unlock with
            // `Decision::Lock` when the time is already spent - a stir as well,
            // since the first key after a Mac slept through the deadline is
            // what finds it - and `Shared::post` fires on the thread that
            // posted. A lock writes the vault out before wiping it, so that
            // thread pays a key derivation.
            for firing in ["Timer", "Event::"] {
                assert!(
                    !item.contains(firing),
                    "this command reaches the timer from the drawing thread:\n{named}"
                );
            }
        }

        // The ones that reach it, named so that the reason survives a rewrite
        // of their bodies. Locking wipes the tree, which takes the same mutex a
        // save is holding - and writes the vault out first, so it costs a key
        // derivation as well - and going back from an open copy to its vault
        // is a lock. A shortened timeout that has already gone, and a stir
        // that arrives after the time ran out, are both answered by a lock on
        // the thread that posted them. Closing the window is a lock reached
        // through `closing::lock`, which the check above cannot see into.
        for reaching in [
            "pub fn lock(",
            "pub fn leave_rescue(",
            "pub fn set_settings(",
            "pub fn stirred(",
            "pub fn close_window(",
        ] {
            assert!(
                source.contains(&format!("#[tauri::command(async)]\n{reaching}")),
                "{reaching} reaches the lock and must not be answered inline"
            );
        }
    }

    /// The menu bar and the close button reach Rust on the thread AppKit draws
    /// on, outside any command: a menu event, the run callback's arms, and the
    /// change to the bar `menu_state` posts there. The close button's lock is
    /// held off that thread by `closing.rs`, whose tests say so. Every other
    /// way in is read here, with the functions each one calls in this crate,
    /// because the check above sees commands only, and a session read or a lock
    /// added the obvious way - in an arm, or in one of the functions behind it -
    /// freezes the window behind a save.
    #[test]
    fn nothing_the_menu_bar_or_the_close_button_runs_where_the_window_is_drawn_waits() {
        fn after<'a>(source: &'a str, from: &str, to: &str) -> &'a str {
            let start = source
                .find(from)
                .unwrap_or_else(|| panic!("{from} is no longer there"));
            let rest = &source[start..];
            &rest[..rest.find(to).unwrap_or(rest.len())]
        }

        /// The function of `file` whose head is `head`.
        fn function(file: &str, head: &str) -> String {
            functions(shipped(file))
                .into_iter()
                .find(|body| body.trim_start().starts_with(head))
                .unwrap_or_else(|| panic!("{head} is no longer there"))
        }

        let commands = shipped(include_str!("commands.rs"));
        let route = include_str!("route.rs");
        let window = include_str!("window.rs");
        let run = shipped(include_str!("lib.rs"));
        let mut drawn: Vec<(&str, String)> = vec![
            (
                "the change menu_state posts",
                after(
                    after(commands, "pub fn menu_state(", "\n}\n"),
                    "run_on_main_thread(move ||",
                    "\n}\n",
                )
                .to_owned(),
            ),
            (
                "the close button",
                after(run, "WindowEvent::CloseRequested", "WindowEvent::Destroyed").to_owned(),
            ),
            (
                "a window that went",
                after(run, "WindowEvent::Destroyed", "RunEvent::ExitRequested").to_owned(),
            ),
            (
                "the Dock icon",
                after(run, "RunEvent::Reopen", "RunEvent::Exit =>").to_owned(),
            ),
        ];
        for (what, file, head) in [
            ("a menu event", route, "pub fn chosen<"),
            ("telling the page", route, "pub fn tell("),
            ("handing the page a choice", route, "pub fn deliver("),
            ("reaching the page", route, "fn reach("),
            ("forgetting the page", route, "pub fn forget("),
            ("greying the bar", include_str!("menu.rs"), "pub fn enable<"),
            ("bringing the window back", window, "pub fn bring_back<"),
            ("building it again", window, "pub fn again<"),
            ("building it", window, "fn open<"),
            (
                "asking the page to close",
                include_str!("closing.rs"),
                "pub fn requested<",
            ),
        ] {
            drawn.push((what, function(file, head)));
        }

        for (what, code) in drawn {
            assert!(code.len() > 40, "{what} was not found:\n{code}");
            let code: String = code
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            // `.post(` rather than `Event::`, which every window event's arm
            // spells: posting to the timer is what reaches the lock.
            for waiting in [
                "Held<",
                "Session",
                "is_unlocked",
                "busy(",
                "Timer",
                ".post(",
                "lock::lock(",
                "closing::lock(",
            ] {
                assert!(
                    !code.contains(waiting),
                    "{what} reaches {waiting} on the thread the window is drawn on:\n{code}"
                );
            }
        }
    }

    /// A version is named by its position, and a position is an answer about
    /// the vault as it stood when the list was read. Every command that takes
    /// one takes the revision it was read at as well, and reaches the vault
    /// only through the doors that check it under the lock the action runs in.
    /// A command added the obvious way - `session.with` and an index - would
    /// act on whichever version a save had moved into the place.
    ///
    /// Read out of this file's own source, because the check is about which
    /// door a command goes through, and no running command can be asked that.
    #[test]
    fn every_command_that_names_a_version_by_position_checks_the_revision() {
        let source = shipped(include_str!("commands.rs"));

        let naming: Vec<String> = functions(source)
            .into_iter()
            .filter(|body| body.contains("index: usize"))
            .collect();
        assert_eq!(
            naming.len(),
            5,
            "a version is read, revealed, copied, restored and dropped by position"
        );

        for body in naming {
            let named = body.lines().take(2).collect::<Vec<_>>().join("\n");
            assert!(
                body.contains("revision: u64"),
                "this command takes a position without its revision:\n{named}"
            );
            assert!(
                [
                    "session.at(revision,",
                    "session.at_mut(revision,",
                    "index, revision,"
                ]
                .iter()
                .any(|door| body.contains(door)),
                "this command does not check the revision it is sent:\n{named}"
            );
            assert!(
                !body.contains("session.with(") && !body.contains("session.with_mut("),
                "this command reaches the vault past the check:\n{named}"
            );
        }
    }

    /// A lock file is taken over only where a reader can have been shown who
    /// holds it. Anything else deciding that on their behalf would make the lock
    /// worth nothing.
    #[test]
    fn only_the_command_a_reader_presses_takes_a_lock_over() {
        let source = shipped(include_str!("commands.rs"));

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

    /// A home folder with Coffer's folder in it, empty.
    fn home() -> (tempfile::TempDir, PathBuf) {
        let home = tempfile::tempdir().expect("a scratch directory");
        let folder = home.path().join(home::FOLDER);
        std::fs::create_dir(&folder).expect("the folder is made");
        (home, folder)
    }

    /// A file that is a vault by the folder's rule: something in it.
    fn vault(at: &Path) {
        std::fs::write(at, b"not empty").expect("the file is written");
    }

    fn code_of(failure: Failure) -> String {
        serde_json::to_value(failure).expect("a failure serialises")["code"]
            .as_str()
            .expect("a code")
            .to_owned()
    }

    /// A session with a vault open, made with the cheapest key derivation.
    fn unlocked(at: &Path) -> Session {
        let session = Session::new(None, None);
        session.making(at.to_path_buf());
        session.measured(kdf::Work::at(1));
        session
            .create(Zeroizing::new(b"coffer-test".to_vec()))
            .expect("the vault is made");
        session
    }

    /// The card named `vault.kdbx`, and the reader moved it away before
    /// pressing. A second search would find `old.kdbx` and open that under a
    /// card that named another file: the reader would type their password into
    /// a stale vault.
    #[test]
    fn the_offer_opens_the_file_the_card_named_or_nothing() {
        let (home, folder) = home();
        vault(&folder.join("vault.kdbx"));
        vault(&folder.join("old.kdbx"));
        let session = Session::new(None, None);

        assert_eq!(
            looked_home(&session, Some(home.path())),
            Some(folder.join("vault.kdbx"))
        );
        std::fs::rename(folder.join("vault.kdbx"), home.path().join("moved.kdbx"))
            .expect("the vault is moved away");

        let refused = found_chosen(&session)
            .err()
            .expect("the named file is gone");
        assert_eq!(code_of(refused), "gone");
        assert_eq!(session.database(), None, "something else was chosen");

        // What the screen reads next names what is there now.
        assert_eq!(
            looked_home(&session, Some(home.path())),
            Some(folder.join("old.kdbx"))
        );
        let chosen = found_chosen(&session).expect("the file now named opens");
        assert_eq!(
            Some(PathBuf::from(chosen.path)),
            folder.join("old.kdbx").canonicalize().ok()
        );
    }

    /// The file the card named stopped being a vault between the screen and
    /// the press. Each is answered `gone` and nothing is chosen. A vault put at
    /// the same name is the file the card named, and is what opens.
    #[test]
    fn an_offer_whose_file_became_something_else_is_gone() {
        type Replace = fn(&Path);
        let replacements: [(&str, Replace); 4] = [
            ("a folder", |at| {
                std::fs::create_dir(at).expect("the folder is made")
            }),
            ("a link to nothing", |at| {
                symlink(at.with_file_name("nowhere.kdbx"), at).expect("a link")
            }),
            ("an empty file", |at| {
                std::fs::write(at, b"").expect("the file is written")
            }),
            ("nothing at all", |_| {}),
        ];

        for (what, replace) in replacements {
            let (home, folder) = home();
            let named = folder.join("vault.kdbx");
            vault(&named);
            let session = Session::new(None, None);
            assert_eq!(
                looked_home(&session, Some(home.path())),
                Some(named.clone())
            );

            std::fs::remove_file(&named).expect("the vault goes");
            replace(&named);

            let refused = found_chosen(&session).err();
            assert_eq!(refused.map(code_of).as_deref(), Some("gone"), "{what}");
            assert_eq!(session.database(), None, "{what} was chosen");
        }

        let (home, folder) = home();
        let named = folder.join("vault.kdbx");
        vault(&named);
        let session = Session::new(None, None);
        looked_home(&session, Some(home.path()));
        std::fs::write(&named, b"written again since").expect("the file is replaced");

        assert!(found_chosen(&session).is_ok());
        assert_eq!(session.database(), named.canonicalize().ok());
    }

    /// A launch that remembers a vault offers nothing from the folder, and a
    /// press that arrives anyway opens nothing.
    #[test]
    fn nothing_is_offered_over_a_remembered_vault() {
        let (home, folder) = home();
        vault(&folder.join("vault.kdbx"));
        let remembered = home.path().join("old.kdbx");
        vault(&remembered);
        let session = Session::new(Some(remembered.clone()), None);

        assert_eq!(looked_home(&session, Some(home.path())), None);
        assert_eq!(
            found_chosen(&session).err().map(code_of).as_deref(),
            Some("gone")
        );
        assert_eq!(session.database(), Some(remembered));
    }

    /// Only a copy a lock left is in the folder: its vault's file went while it
    /// was open. The offer opens the vault's name, whose unlock screen puts the
    /// copy back.
    #[test]
    fn a_copy_whose_vault_has_gone_is_offered_under_the_vault_s_name() {
        let (home, folder) = home();
        vault(&folder.join("vault.kdbx.unsaved.kdbx"));
        let session = Session::new(None, None);

        let named = folder.join("vault.kdbx");
        assert_eq!(
            looked_home(&session, Some(home.path())),
            Some(named.clone())
        );
        assert!(
            dto::Found::of(&named).copy,
            "the offer says it found a vault that is not there"
        );
        assert!(found_chosen(&session).is_ok());
        assert_eq!(session.database(), Some(named.clone()));

        // The vault back at its name is a vault found, whatever is beside it.
        vault(&named);
        assert!(!dto::Found::of(&named).copy);
    }

    /// Choosing either way would lock the open vault, and a press on a screen
    /// that should not be showing is not the reader asking for that.
    #[test]
    fn neither_offer_opens_anything_while_a_vault_is_open() {
        let (home, folder) = home();
        let open = home.path().join("open.kdbx");
        let session = unlocked(&open);
        vault(&folder.join("vault.kdbx"));
        session.finding(Some(folder.join("vault.kdbx")));
        session.making(folder.join("vault.kdbx"));

        assert_eq!(
            found_chosen(&session).err().map(code_of).as_deref(),
            Some("refused")
        );
        assert_eq!(
            existing_chosen(&session).err().map(code_of).as_deref(),
            Some("refused")
        );
        assert!(session.is_unlocked(), "the open vault was locked");
        assert_eq!(session.database(), open.canonicalize().ok());
    }

    /// What sits where the new vault would go is opened only when it is a
    /// vault, or the copy a lock left of one whose file has gone. Anything
    /// else is answered `gone`, and nothing is chosen.
    #[test]
    fn only_a_vault_is_opened_instead_of_making_one() {
        let (home, folder) = home();
        let at = |name: &str| folder.join(name);

        let session = Session::new(None, None);
        assert_eq!(
            existing_chosen(&session).err().map(code_of).as_deref(),
            Some("refused"),
            "nothing was settled, and something was chosen"
        );

        std::fs::create_dir(at("folder.kdbx")).expect("the folder is made");
        symlink(at("nowhere.kdbx"), at("dangling.kdbx")).expect("a link");
        std::fs::write(at("empty.kdbx"), b"").expect("the file is written");
        vault(&at("vanished.kdbx"));
        std::fs::remove_file(at("vanished.kdbx")).expect("the file goes");

        for name in [
            "folder.kdbx",
            "dangling.kdbx",
            "empty.kdbx",
            "vanished.kdbx",
        ] {
            session.making(at(name));
            let refused = existing_chosen(&session).err();
            assert_eq!(refused.map(code_of).as_deref(), Some("gone"), "{name}");
            assert_eq!(session.database(), None, "{name} was chosen");
        }

        // A link to a vault is followed to the file, which is what opens.
        let kept = home.path().join("kept.kdbx");
        vault(&kept);
        symlink(&kept, at("linked.kdbx")).expect("a link");
        session.making(at("linked.kdbx"));
        let chosen = existing_chosen(&session).expect("the vault is chosen");
        assert_eq!(Some(PathBuf::from(chosen.path)), kept.canonicalize().ok());

        vault(&at("gone.kdbx.unsaved.kdbx"));
        session.making(at("gone.kdbx"));
        assert!(existing_chosen(&session).is_ok());
        assert_eq!(session.database(), Some(at("gone.kdbx")));
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
