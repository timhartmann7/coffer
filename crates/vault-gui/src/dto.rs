//! The metadata that crosses the process boundary.
//!
//! One rule shapes every type here: a field's `value` is `null` exactly when
//! the database protects it. A protected value is not in these types, has never
//! been in them, and reaches the screen only through
//! [`reveal`][crate::commands::reveal], one field at a time.
//!
//! None of these derive `Debug`. They carry the user's titles, logins and
//! addresses, which the database does not protect but which still have no
//! business in a panic message.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize, Serializer};
use vault_core::model::{self, FieldValue, fields};
use zeroize::Zeroizing;

use crate::settings;

/// The database Coffer has chosen, whether or not it is open.
#[derive(Serialize)]
pub struct Database {
    /// Absolute, with symbolic links already followed.
    pub path: String,
    /// The file name without its extension: what the window calls the vault.
    pub name: String,
}

/// Where a new vault would go, as the creation screen draws it.
#[derive(Serialize)]
pub struct Target {
    #[serde(flatten)]
    pub place: Database,
    /// The path the way its owner would write it: under the home folder it
    /// starts with `~`. What the screen shows, and never what anything opens.
    pub shown: String,
    /// Whether something is already at the name. Nothing is ever made over
    /// one, so the screen says so before anybody types a password rather than
    /// after.
    pub taken: bool,
}

/// A vault sitting in Coffer's own folder, offered on a launch that remembers
/// none. The path does not cross: opening it asks Rust to find it again.
#[derive(Serialize)]
pub struct Found {
    /// The file name, which is what the offer to open it says.
    pub name: String,
    /// The folder in the home folder it was found in.
    pub folder: &'static str,
}

/// A value revealed out of the vault, on its way to the one screen that asked
/// for it.
///
/// The buffer is wiped when this is dropped. Serde copies it into the message
/// Tauri sends and that copy is out of reach - the same residue `docs/ipc.md`
/// records for the password travelling the other way - but the copy Coffer owns
/// is not left behind.
pub struct Revealed(Zeroizing<String>);

impl Revealed {
    pub fn new(value: &str) -> Revealed {
        Revealed(Zeroizing::new(value.to_owned()))
    }
}

impl Serialize for Revealed {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// The part of a value a reader selected on the screen, from one position to
/// another, counted the way the text node showing it counts: in UTF-16 code
/// units. Two numbers and nothing of the value; whether they name a part of it
/// at all is [`vault_core::SecretValue::part`]'s to say.
#[derive(Deserialize, Clone, Copy)]
pub struct Span {
    pub from: usize,
    pub to: usize,
}

/// The entry an id names, or nothing at all. An id that does not parse is not
/// an entry Coffer has, which is the same answer as an id that parses and names
/// nothing.
pub fn entry_id(text: &str) -> Result<model::EntryId, crate::error::Failure> {
    uuid::Uuid::parse_str(text)
        .map(model::EntryId::from_uuid)
        .map_err(|_| crate::error::Failure::no_such_entry())
}

/// The folder an id names, on the same terms.
pub fn group_id(text: &str) -> Result<model::GroupId, crate::error::Failure> {
    uuid::Uuid::parse_str(text)
        .map(model::GroupId::from_uuid)
        .map_err(|_| crate::error::Failure::no_such_group())
}

impl Snapshot {
    pub fn of(taken: &vault_core::storage::snapshot::Taken) -> Snapshot {
        Snapshot {
            name: file_name(&taken.path),
            index: taken.index,
            taken: taken.taken.and_then(moment),
        }
    }
}

impl Found {
    pub fn of(path: &std::path::Path) -> Found {
        Found {
            name: file_name(path),
            folder: crate::home::FOLDER,
        }
    }
}

impl Rescued {
    pub fn of(kept: &vault_core::storage::unsaved::Kept) -> Rescued {
        Rescued {
            name: file_name(&kept.path),
            written: kept.written.and_then(moment),
        }
    }
}

/// A file as the disk has it now, for the sentences that say what a lock left:
/// what survived a lock that could write nothing, and whether a copy has a
/// vault to go over or an empty name to go back into.
#[derive(Serialize)]
pub struct OnDisk {
    /// Whether anything at all is at the name.
    pub there: bool,
    /// When it was last written, when it is there and the filesystem keeps the
    /// time.
    pub written: Option<String>,
}

impl OnDisk {
    pub fn of(found: vault_core::storage::OnDisk) -> OnDisk {
        match found {
            vault_core::storage::OnDisk::Gone => OnDisk {
                there: false,
                written: None,
            },
            vault_core::storage::OnDisk::Written(written) => OnDisk {
                there: true,
                written: written.and_then(moment),
            },
        }
    }
}

/// The vault a chosen database was copied from, when it is the copy a lock
/// left: what the screen says about the copy and about what making it the vault
/// would do. Names and times only; the paths stay in Rust.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyOf {
    /// The vault's file name, which is the file the copy would go over.
    pub vault: String,
    /// When the copy was last written, as the filesystem has it.
    pub saved: Option<String>,
    /// What the vault's file is called once the copy has gone over it: the
    /// newest snapshot.
    pub kept_as: String,
    /// The vault's file as it stands, which the copy would go over or, when
    /// it has gone, take the name of.
    pub vault_file: OnDisk,
}

impl CopyOf {
    pub fn of(
        vault: &std::path::Path,
        copy: vault_core::storage::OnDisk,
        vault_file: vault_core::storage::OnDisk,
    ) -> CopyOf {
        CopyOf {
            vault: file_name(vault),
            saved: match copy {
                vault_core::storage::OnDisk::Written(written) => written.and_then(moment),
                vault_core::storage::OnDisk::Gone => None,
            },
            kept_as: vault_core::storage::snapshot::slot(vault, 1)
                .map(|slot| file_name(&slot))
                .unwrap_or_default(),
            vault_file: OnDisk::of(vault_file),
        }
    }
}

/// The name a file goes by on the screen.
///
/// The file name and not the stem: an offer to open something has to say what
/// the reader would see in the Finder, and the stem of `vault.kdbx.unsaved.kdbx`
/// reads as a vault called `vault.kdbx.unsaved`.
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl Database {
    pub fn of(path: &std::path::Path) -> Database {
        Database {
            path: path.to_string_lossy().into_owned(),
            name: path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub database: Option<Database>,
    /// A vault in Coffer's own folder, when nothing is remembered to open.
    /// Looked for only then: a launch that knows its vault has nothing to find.
    pub found: Option<Found>,
    /// The key file the next unlock will use, when the reader has chosen one.
    /// Reported rather than remembered by the window, because a lock destroys
    /// the window and the vault it is about is still the same one.
    pub key_file: Option<Database>,
    pub unlocked: bool,
    /// How many entries the vault holds, the recycle bin's included.
    pub entries: usize,
    /// Whether this database can be written back at all. A snapshot, a format
    /// Coffer will not write and a place that refuses a write are all read only.
    pub read_only: bool,
    /// The unsaved copy sitting beside the database Coffer will open next, when
    /// a lock had to write one. Read off the disk rather than remembered, so a
    /// copy left by a run that has since quit is still offered.
    pub rescue: Option<Rescued>,
    /// Whether the last lock in this run found work the file had not got and
    /// could not put it anywhere at all. There is no file to point at, which is
    /// why this is a flag and not a path.
    pub lost: bool,
    /// The chosen database's own file as it stands. Read off the disk each
    /// time, because it is what a lock left and what the reader may since have
    /// moved.
    pub file: Option<OnDisk>,
    /// The vault the chosen database was copied from, when it is the copy a
    /// lock left.
    pub copy: Option<CopyOf>,
    /// Whether the last lock found text the reader was still typing and saved
    /// it into the vault with everything else. Which entry is not said: after a
    /// lock nothing of the vault is left to say it with.
    pub typed: bool,
    /// Why the vault that was open is not open any more, when it is worth
    /// saying. A lock the reader asked for has nothing to explain.
    pub locked_by: Option<&'static str>,
    /// How many seconds the open vault has before it locks itself.
    pub locks_in: Option<u64>,
}

/// What the reader chose, and what they may choose instead.
///
/// The lists come with the values. A screen that held its own copy of what may
/// be chosen would be a second place the answer lives, and the two would part
/// company the first time one of them changed.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub idle_seconds: u64,
    pub clipboard_seconds: u64,
    pub lock_on_sleep: bool,
    pub lock_on_screen_lock: bool,
    pub theme: settings::Theme,
    #[serde(skip_deserializing)]
    pub idle_choices: Vec<u64>,
    #[serde(skip_deserializing)]
    pub clipboard_choices: Vec<u64>,
    #[serde(skip_deserializing)]
    pub theme_choices: Vec<settings::Theme>,
}

impl Settings {
    pub fn of(held: settings::Settings) -> Settings {
        Settings {
            idle_seconds: held.idle_seconds,
            clipboard_seconds: held.clipboard_seconds,
            lock_on_sleep: held.lock_on_sleep,
            lock_on_screen_lock: held.lock_on_screen_lock,
            theme: held.theme,
            idle_choices: settings::IDLE_CHOICES.to_vec(),
            clipboard_choices: settings::CLIPBOARD_CHOICES.to_vec(),
            theme_choices: settings::THEME_CHOICES.to_vec(),
        }
    }

    /// What the window asked for. The lists it was sent do not come back: they
    /// are this side's to decide, and a message naming others would be the
    /// window choosing what it may choose. A look it invented lands on the
    /// default the same way one out of a settings file does.
    pub fn wanted(&self) -> settings::Settings {
        settings::Settings {
            idle_seconds: self.idle_seconds,
            clipboard_seconds: self.clipboard_seconds,
            lock_on_sleep: self.lock_on_sleep,
            lock_on_screen_lock: self.lock_on_screen_lock,
            theme: self.theme,
        }
    }
}

/// What key derivation a new vault will ask for, and what that measured.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Calibration {
    pub iterations: u64,
    /// What the measurement took, in seconds. The one number the creation
    /// screen shows, and it only exists once the measuring is over.
    pub seconds: f64,
}

impl Calibration {
    pub fn of(found: vault_core::kdf::Calibration) -> Calibration {
        Calibration {
            iterations: found.work.iterations,
            seconds: found.took.as_secs_f64(),
        }
    }
}

/// A previous version of an entry, as the versions block lists them.
#[derive(Serialize)]
pub struct Version {
    /// Where it sits in the entry's history, which is how it is addressed.
    /// Several versions can share a modification time, so nothing else
    /// identifies one.
    pub index: usize,
    pub modified: Option<String>,
}

impl Version {
    pub fn of(version: &model::Version) -> Version {
        Version {
            index: version.index,
            modified: stamp(version.modified),
        }
    }
}

/// What a command that changed the shape of the vault hands back: the tree as
/// it is now, and the entry the change was about.
#[derive(Serialize)]
pub struct Made {
    pub tree: Group,
    pub entry: String,
}

/// What the file on disk holds, for the dialog that asks which version to keep.
#[derive(Serialize)]
pub struct Rival {
    /// When the file was last written, as the filesystem has it.
    pub modified: Option<String>,
    /// How many entries it holds, or nothing when it will not open with the
    /// password this vault was opened with.
    pub entries: Option<usize>,
}

/// The unsaved copy a lock left beside the vault.
#[derive(Serialize)]
pub struct Rescued {
    /// The file name, which is what the offer to open it says.
    pub name: String,
    /// When it was written, as the filesystem has it.
    pub written: Option<String>,
}

/// A snapshot Coffer took before one of its own saves.
#[derive(Serialize)]
pub struct Snapshot {
    /// The file name, which is what the offer to open it says.
    pub name: String,
    /// Which slot it is in, counting from 1 for the most recent. This is what
    /// the screen sends back to open it.
    pub index: u32,
    pub taken: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    pub is_recycle_bin: bool,
    /// When the group is in the recycle bin, when it went in and where it goes
    /// back to. `null` for the bin itself and for everything outside it.
    pub binned: Option<Binned>,
    /// What deleting the group would do.
    pub deletion: Deletion,
    pub sections: Vec<Group>,
    pub entries: Vec<EntryRow>,
}

impl Group {
    pub fn of(project: &model::Project) -> Group {
        Group {
            id: project.id.to_string(),
            name: project.name.clone(),
            is_recycle_bin: project.is_recycle_bin,
            binned: project.binned.map(Binned::of),
            deletion: Deletion::of(project.deletion),
            sections: project.sections.iter().map(Group::of).collect(),
            entries: project.entries.iter().map(EntryRow::of).collect(),
        }
    }
}

/// What deleting something does, said before the reader asks for it.
///
/// It travels with the tree and with the entry rather than in `status`,
/// because both are read again after every change and after a reload, and a
/// file another client rewrote can have stopped keeping a bin: a flag read
/// once at unlock would go on promising a bin to a deletion that erases.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Deletion {
    /// It moves to the recycle bin, and can be put back.
    Bin,
    /// It goes out of the file for good.
    Forever,
}

impl Deletion {
    fn of(deletion: model::Deletion) -> Deletion {
        match deletion {
            model::Deletion::Bin => Deletion::Bin,
            model::Deletion::Forever => Deletion::Forever,
        }
    }
}

/// What is known about something in the recycle bin.
#[derive(Serialize)]
pub struct Binned {
    /// When it went in: its own move, or that of the folder it went in with.
    pub since: Option<String>,
    /// The folder it was in, which is where putting it back takes it: `null`
    /// when that is not known, has gone, or is in the bin too, and putting it
    /// back takes it to the top of the vault.
    pub from: Option<String>,
}

impl Binned {
    fn of(binned: model::Binned) -> Binned {
        Binned {
            since: stamp(binned.since),
            from: binned.from.map(|group| group.to_string()),
        }
    }
}

/// What a list row may know. Notes and custom fields are not here: they are the
/// user's data and have no business travelling in bulk to a screen that only
/// wants to draw a row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryRow {
    pub id: String,
    pub group: String,
    pub title: Option<String>,
    pub username: Option<String>,
    pub url: Option<String>,
    pub tags: Vec<String>,
    pub modified: Option<String>,
    pub has_password: bool,
    pub attachments: usize,
    /// When the entry is in the recycle bin, when it went in and where from.
    pub binned: Option<Binned>,
}

impl EntryRow {
    pub fn of(summary: &model::EntrySummary) -> EntryRow {
        EntryRow {
            id: summary.id.to_string(),
            group: summary.group.to_string(),
            title: shown(&summary.title),
            username: shown(&summary.username),
            url: shown(&summary.url),
            tags: summary.tags.clone(),
            modified: stamp(summary.times.modified),
            has_password: summary.has_password,
            attachments: summary.attachments,
            binned: summary.binned.map(Binned::of),
        }
    }
}

/// Which of an entry's fields this is. It says nothing about the value, so
/// unlike everything else here it is safe to print.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    Title,
    Username,
    Password,
    Url,
    Notes,
    Custom,
}

impl FieldKind {
    /// The names live in `vault-core`, which is the only place that knows what
    /// KeePass calls a field.
    fn of(name: &str) -> FieldKind {
        match name {
            fields::TITLE => FieldKind::Title,
            fields::USERNAME => FieldKind::Username,
            fields::PASSWORD => FieldKind::Password,
            fields::URL => FieldKind::Url,
            fields::NOTES => FieldKind::Notes,
            _ => FieldKind::Custom,
        }
    }
}

#[derive(Serialize)]
pub struct Field {
    /// The name the file holds, and the name a reveal asks for.
    pub name: String,
    pub kind: FieldKind,
    /// Whether the database keeps this value protected. The screen sends it
    /// back on an edit, so that rewriting a field never quietly turns a
    /// protected value into plain text inside the file.
    pub protected: bool,
    /// The value, or `null` when it does not cross: the database protects it,
    /// or it is the password. A file can hold a password the database does not
    /// protect, and it is still a password; `empty` says whether there is one,
    /// and a reveal is the only way to see it.
    pub value: Option<String>,
    pub empty: bool,
    /// Whether Coffer would hand this value to the system if asked to open it.
    /// Only ever true for a URL field: the rule is in
    /// [`vault_core::url`], and this is the screen's copy of the answer.
    pub openable: bool,
}

impl Field {
    fn of(field: &model::Field) -> Field {
        let kind = FieldKind::of(&field.name);
        let value = match kind {
            FieldKind::Password => None,
            _ => shown(&field.value),
        };
        Field {
            name: field.name.clone(),
            kind,
            protected: matches!(field.value, FieldValue::Protected { .. }),
            openable: kind == FieldKind::Url
                && value
                    .as_deref()
                    .is_some_and(|text| vault_core::url::openable(text).is_some()),
            value,
            empty: field.is_empty(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    /// As the file holds it. It may contain anything at all, `/` and `..`
    /// included, so nothing may build a path out of it.
    pub name: String,
    pub size: usize,
    /// What the save panel would call it. The rule lives in `vault-core`; this
    /// is the screen's copy of the answer, so that the name a reader sees on
    /// the export button is the name they will get.
    pub file_name: String,
}

#[derive(Serialize)]
pub struct Entry {
    pub id: String,
    pub group: String,
    /// How many previous versions the entry keeps.
    pub versions: usize,
    /// Every field the entry has, standard and custom alike, ordered by name.
    pub fields: Vec<Field>,
    pub attachments: Vec<Attachment>,
    pub tags: Vec<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    /// When the entry is in the recycle bin, when it went in and where it goes
    /// back to. The screen shows such an entry read only.
    pub binned: Option<Binned>,
    /// What deleting the entry would do.
    pub deletion: Deletion,
}

impl Entry {
    pub fn of(entry: &model::Entry) -> Entry {
        Entry {
            id: entry.id.to_string(),
            group: entry.group.to_string(),
            versions: entry.versions,
            fields: entry.fields.iter().map(Field::of).collect(),
            attachments: entry
                .attachments
                .iter()
                .map(|attachment| Attachment {
                    name: attachment.name.clone(),
                    size: attachment.size,
                    file_name: attachment.file_name(),
                })
                .collect(),
            tags: entry.tags.clone(),
            created: stamp(entry.times.created),
            modified: stamp(entry.times.modified),
            binned: entry.binned.map(Binned::of),
            deletion: Deletion::of(entry.deletion),
        }
    }
}

/// What came of a file the reader chose for an entry.
///
/// Not a failure when the name is taken: nothing went wrong, and nothing has
/// happened yet. The file is held in Rust and the window has a question to
/// ask, which is a different thing from a sentence to show.
#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum Attached {
    /// It is on the entry, and this is the entry now.
    Added { entry: Entry },
    /// The entry already gives that name to a file, and nothing changed.
    Taken { clash: Clash },
}

/// A name the entry already gives a file, and what the reader is asked to
/// decide about it. Sizes and names only: neither file's bytes cross.
#[derive(Serialize)]
pub struct Clash {
    /// The name both files go by, exactly as the entry holds it.
    pub name: String,
    /// How large the file already there is.
    pub size: usize,
    /// How large the file just chosen is. Two files by one name are told apart
    /// by size before anything else, and a reader who picked the same file
    /// twice sees two numbers that agree.
    pub chosen: usize,
    /// The name the file just chosen goes under if both are kept.
    pub free: String,
}

impl Clash {
    pub fn of(name: String, chosen: usize, clash: vault_core::Clash) -> Clash {
        Clash {
            name,
            size: clash.size,
            chosen,
            free: clash.free,
        }
    }
}

/// The value when the database does not protect it, and nothing at all when it
/// does.
fn shown(value: &FieldValue) -> Option<String> {
    match value {
        FieldValue::Open(text) => Some(text.clone()),
        FieldValue::Protected { .. } => None,
    }
}

/// KDBX keeps its times in UTC, and the screen turns them into the reader's own
/// clock. Sending the zone with the value is what makes that possible.
fn stamp(time: Option<NaiveDateTime>) -> Option<String> {
    time.map(|time| time.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// A filesystem timestamp, when it is one a calendar can hold.
///
/// It comes from whatever wrote the file, which may be another machine, another
/// filesystem or nothing sensible at all. `DateTime::from(SystemTime)` unwraps
/// its own range check, and a snapshot beside the database is not worth a panic.
pub(crate) fn moment(time: std::time::SystemTime) -> Option<String> {
    let since = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    let moment = chrono::DateTime::from_timestamp(
        i64::try_from(since.as_secs()).ok()?,
        since.subsec_nanos(),
    )?;
    Some(moment.naive_utc().format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use vault_core::model::{Attachment, EntryId, EntrySummary, Field, GroupId, Timestamps};

    use super::*;

    fn open(name: &str, value: &str) -> Field {
        Field {
            name: name.to_owned(),
            value: FieldValue::Open(value.to_owned()),
        }
    }

    fn protected(name: &str, empty: bool) -> Field {
        Field {
            name: name.to_owned(),
            value: FieldValue::Protected { empty },
        }
    }

    fn entry_of(fields: Vec<Field>) -> model::Entry {
        model::Entry {
            id: EntryId::from_uuid(uuid::Uuid::nil()),
            group: GroupId::from_uuid(uuid::Uuid::nil()),
            fields,
            attachments: Vec::new(),
            tags: Vec::new(),
            times: Timestamps::default(),
            versions: 0,
            binned: None,
            deletion: model::Deletion::Bin,
        }
    }

    fn json(value: &impl Serialize) -> String {
        serde_json::to_string(value).expect("the payload serialises")
    }

    /// The whole point of the boundary: what the database protects does not
    /// cross it, and the payload says so rather than saying nothing.
    #[test]
    fn a_protected_value_is_absent_and_declared_absent() {
        let entry = Entry::of(&entry_of(vec![
            open(fields::TITLE, "a login"),
            protected(fields::PASSWORD, false),
            protected("API token", false),
            protected("empty secret", true),
        ]));

        let payload = json(&entry);
        assert!(payload.contains(
            r#""name":"Password","kind":"password","protected":true,"value":null,"empty":false"#
        ));
        assert!(payload.contains(
            r#""name":"API token","kind":"custom","protected":true,"value":null,"empty":false"#
        ));
        assert!(payload.contains(
            r#""name":"empty secret","kind":"custom","protected":true,"value":null,"empty":true"#
        ));
    }

    #[test]
    fn a_row_carries_the_four_things_a_list_filters_on_and_nothing_else() {
        let row = EntryRow::of(&EntrySummary {
            id: EntryId::from_uuid(uuid::Uuid::nil()),
            group: GroupId::from_uuid(uuid::Uuid::nil()),
            title: FieldValue::Open("node-3".to_owned()),
            username: FieldValue::Open("deploy".to_owned()),
            url: FieldValue::Protected { empty: false },
            tags: vec!["prod".to_owned(), "ssh".to_owned()],
            times: Timestamps {
                created: None,
                modified: NaiveDate::from_ymd_opt(2026, 3, 12)
                    .and_then(|day| day.and_hms_opt(18, 42, 0)),
                expires: None,
            },
            has_password: true,
            attachments: 2,
            versions: 7,
            binned: None,
        });

        assert_eq!(
            json(&row),
            r#"{"id":"00000000-0000-0000-0000-000000000000","group":"00000000-0000-0000-0000-000000000000","title":"node-3","username":"deploy","url":null,"tags":["prod","ssh"],"modified":"2026-03-12T18:42:00Z","hasPassword":true,"attachments":2,"binned":null}"#
        );
    }

    #[test]
    fn every_field_the_format_names_is_named_and_the_rest_are_custom() {
        let entry = Entry::of(&entry_of(vec![
            open(fields::TITLE, "t"),
            open(fields::USERNAME, "u"),
            open(fields::URL, "https://example.com"),
            open(fields::NOTES, "n"),
            protected(fields::PASSWORD, false),
            open("Title ", "not the title"),
            open("title", "not the title either"),
            open("KeeAgent.settings", "machine data"),
        ]));

        let kinds: Vec<(&str, FieldKind)> = entry
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field.kind))
            .collect();

        assert_eq!(
            kinds,
            vec![
                ("Title", FieldKind::Title),
                ("UserName", FieldKind::Username),
                ("URL", FieldKind::Url),
                ("Notes", FieldKind::Notes),
                ("Password", FieldKind::Password),
                ("Title ", FieldKind::Custom),
                ("title", FieldKind::Custom),
                ("KeeAgent.settings", FieldKind::Custom),
            ]
        );
    }

    /// The screen decides whether to offer a link, and the answer is not the
    /// screen's to work out. Anything that could run code arrives marked as
    /// something Coffer will not open.
    #[test]
    fn only_an_address_coffer_would_open_is_marked_openable() {
        for (value, openable) in [
            ("https://example.com", true),
            ("HTTP://EXAMPLE.COM/x", true),
            ("  https://example.com  ", true),
            ("mailto:someone@example.com", true),
            ("javascript:alert(document.domain)", false),
            ("JaVaScRiPt:alert(1)", false),
            ("java\tscript:alert(1)", false),
            ("data:text/html,<script>alert(1)</script>", false),
            ("file:///etc/passwd", false),
            ("", false),
        ] {
            let entry = Entry::of(&entry_of(vec![open(fields::URL, value)]));
            let url = entry.fields.first().expect("the field is there");
            assert_eq!(url.openable, openable, "{value:?}");
        }

        // A protected URL has no value to judge, so there is nothing to open.
        let entry = Entry::of(&entry_of(vec![protected(fields::URL, false)]));
        assert!(!entry.fields.first().expect("the field is there").openable);

        // Only an address is ever openable. A custom field that happens to hold
        // one is a value on a screen, not a link.
        let entry = Entry::of(&entry_of(vec![open("Homepage", "https://example.com")]));
        assert!(!entry.fields.first().expect("the field is there").openable);
    }

    /// A value that is markup stays a value. It crosses as JSON text and the
    /// screen writes it as a text node; nothing here escapes it, and nothing
    /// here has to.
    #[test]
    fn markup_and_control_characters_in_a_value_cross_as_text() {
        let hostile = "<script>alert(1)</script> \" \\ </script> \u{202e}gnitirw \u{0} end";
        let entry = Entry::of(&entry_of(vec![open(fields::TITLE, hostile)]));

        // The quote and the backslash come out escaped, and the null byte
        // comes out as an escape rather than as a byte that would end the
        // string early for whoever parses it next.
        let payload = json(&entry);
        assert!(
            payload.contains(r#"<script>alert(1)</script> \" \\ </script>"#),
            "{payload}"
        );
        assert!(payload.contains(r"\u0000"), "{payload}");
        assert_eq!(
            entry
                .fields
                .first()
                .expect("the field is there")
                .value
                .as_deref(),
            Some(hostile)
        );
    }

    /// A file can hold a password the database does not protect. It is still a
    /// password, and the screen still has to ask for it one reveal at a time.
    #[test]
    fn a_password_the_database_left_open_still_does_not_cross() {
        let secret = "hunter2-not-in-any-payload";
        let entry = Entry::of(&entry_of(vec![open(fields::PASSWORD, secret)]));
        let password = entry.fields.first().expect("the field is there");

        assert_eq!(password.kind, FieldKind::Password);
        assert_eq!(password.value, None);
        assert!(!password.empty);
        assert!(!json(&entry).contains(secret));
    }

    /// The screen sends the protection back when it writes a field, so a value
    /// the database keeps protected has to arrive marked as one. Without it the
    /// screen would have to guess, and a wrong guess writes a password into the
    /// file as plain text.
    #[test]
    fn a_field_says_whether_the_database_protects_it() {
        let entry = Entry::of(&entry_of(vec![
            open(fields::TITLE, "a login"),
            protected(fields::PASSWORD, false),
            open("plain", "2202"),
            protected("API token", false),
        ]));

        let marked: Vec<(&str, bool)> = entry
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field.protected))
            .collect();
        assert_eq!(
            marked,
            vec![
                ("Title", false),
                ("Password", true),
                ("plain", false),
                ("API token", true),
            ]
        );
    }

    /// The name in the file may be anything at all. The screen shows that name
    /// and the save panel is offered the other one, so a reader sees what they
    /// will get.
    #[test]
    fn an_attachment_crosses_with_the_name_a_save_panel_would_use() {
        let mut entry = entry_of(Vec::new());
        entry.attachments = vec![Attachment {
            name: "../../escape.txt".to_owned(),
            size: 12,
        }];

        let payload = json(&Entry::of(&entry));
        assert!(
            payload.contains(r#"{"name":"../../escape.txt","size":12,"fileName":"escape.txt"}"#),
            "{payload}"
        );
    }

    /// A row in the bin says when it went in and where it goes back, and the
    /// folder crosses as its id: the name is the tree's to give, so a folder
    /// renamed since is called what it is called now. Nothing known crosses as
    /// `null` rather than as a guess, and the screen says the top of the vault.
    #[test]
    fn an_entry_in_the_bin_crosses_with_when_it_went_and_where_it_goes_back() {
        let from = GroupId::from_uuid(uuid::Uuid::from_u128(7));
        let mut entry = entry_of(vec![open(fields::TITLE, "Bank")]);
        entry.binned = Some(model::Binned {
            since: NaiveDate::from_ymd_opt(2026, 9, 27).and_then(|day| day.and_hms_opt(12, 0, 0)),
            from: Some(from),
        });
        entry.deletion = model::Deletion::Forever;

        let whole = serde_json::to_value(Entry::of(&entry)).expect("the entry serialises");
        assert_eq!(
            whole["binned"],
            serde_json::json!({ "since": "2026-09-27T12:00:00Z", "from": from.to_string() })
        );
        assert_eq!(whole["deletion"], "forever");

        let row = serde_json::to_value(EntryRow::of(&entry.summary())).expect("the row serialises");
        assert_eq!(row["binned"], whole["binned"]);

        entry.binned = Some(model::Binned {
            since: None,
            from: None,
        });
        let unknown = serde_json::to_value(Entry::of(&entry)).expect("the entry serialises");
        assert_eq!(
            unknown["binned"],
            serde_json::json!({ "since": null, "from": null })
        );

        entry.binned = None;
        entry.deletion = model::Deletion::Bin;
        let live = serde_json::to_value(Entry::of(&entry)).expect("the entry serialises");
        assert_eq!(live["binned"], serde_json::Value::Null);
        assert_eq!(live["deletion"], "bin");
    }

    /// A folder says what deleting it would do, and so does every folder in
    /// it: the answer is Rust's for each one, never inferred by the screen from
    /// the folder above.
    #[test]
    fn every_folder_says_what_deleting_it_would_do() {
        let folder = |name: &str, deletion, sections| model::Project {
            id: GroupId::from_uuid(uuid::Uuid::new_v4()),
            name: name.to_owned(),
            notes: None,
            is_recycle_bin: false,
            binned: None,
            deletion,
            sections,
            entries: Vec::new(),
        };
        let tree = folder(
            "Root",
            model::Deletion::Forever,
            vec![folder("Personal", model::Deletion::Bin, Vec::new())],
        );

        let payload = serde_json::to_value(Group::of(&tree)).expect("the tree serialises");
        assert_eq!(payload["deletion"], "forever");
        assert_eq!(payload["binned"], serde_json::Value::Null);
        assert_eq!(payload["sections"][0]["deletion"], "bin");
        assert_eq!(payload["sections"][0]["isRecycleBin"], false);
    }

    #[test]
    fn a_million_characters_of_title_cross_whole() {
        let long = "a".repeat(1_000_000);
        let entry = Entry::of(&entry_of(vec![open(fields::TITLE, &long)]));
        assert_eq!(
            entry
                .fields
                .first()
                .expect("the field is there")
                .value
                .as_deref(),
            Some(long.as_str())
        );
    }

    /// KDBX can hold a date centuries either side of now, and the screen has to
    /// be handed one it can read rather than one it has to guess at.
    #[test]
    fn timestamps_at_the_edges_of_the_range_cross_as_utc() {
        for (year, expected) in [
            (1600, "1600-01-01T00:00:00Z"),
            (3000, "3000-12-31T23:59:59Z"),
        ] {
            let time = if year == 1600 {
                NaiveDate::from_ymd_opt(1600, 1, 1).and_then(|day| day.and_hms_opt(0, 0, 0))
            } else {
                NaiveDate::from_ymd_opt(3000, 12, 31).and_then(|day| day.and_hms_opt(23, 59, 59))
            };
            assert_eq!(stamp(time).as_deref(), Some(expected));
        }

        assert_eq!(stamp(None), None);
    }

    /// The window branches on one key, and a name that is taken crosses as a
    /// question: two sizes and two names, the first exactly as the entry holds
    /// it whatever is in it, and not a byte of either file.
    #[test]
    fn a_taken_name_crosses_as_a_question_with_no_bytes_in_it() {
        let hostile = "../../<b>scan</b>\u{202e}fdp.pdf";
        let taken = serde_json::to_value(Attached::Taken {
            clash: Clash::of(
                hostile.to_owned(),
                3,
                vault_core::Clash {
                    size: 7,
                    free: "scan 2.pdf".to_owned(),
                },
            ),
        })
        .expect("the payload serialises");
        assert_eq!(
            taken,
            serde_json::json!({
                "outcome": "taken",
                "clash": { "name": hostile, "size": 7, "chosen": 3, "free": "scan 2.pdf" }
            })
        );

        let added = serde_json::to_value(Attached::Added {
            entry: Entry::of(&entry_of(Vec::new())),
        })
        .expect("the payload serialises");
        assert_eq!(added["outcome"], "added");
        assert_eq!(added["entry"]["id"], uuid::Uuid::nil().to_string());
    }

    #[test]
    fn an_attachment_crosses_with_its_name_exactly_as_the_file_holds_it() {
        let mut entry = entry_of(Vec::new());
        entry.attachments = vec![
            Attachment {
                name: "../../escape.txt".to_owned(),
                size: 0,
            },
            Attachment {
                name: "nested/path/name.txt".to_owned(),
                size: 100 * 1024 * 1024,
            },
        ];

        let payload = json(&Entry::of(&entry));
        assert!(payload.contains(r#""name":"../../escape.txt","size":0"#));
        assert!(payload.contains(r#""name":"nested/path/name.txt","size":104857600"#));
    }

    /// The window is told what it may choose. A message that answered with a
    /// list of its own would be the window widening that, which is what the
    /// `skip_deserializing` on the lists is for - and what a new list could
    /// quietly miss.
    #[test]
    fn a_look_the_window_made_up_is_settled_like_a_number_nobody_offered() {
        let asked: Settings = serde_json::from_str(
            r#"{"idleSeconds":900,"clipboardSeconds":15,"lockOnSleep":false,
                "lockOnScreenLock":true,"theme":"neon","themeChoices":["neon"]}"#,
        )
        .expect("the window's message parses");

        assert!(
            asked.theme_choices.is_empty(),
            "the window sent a list back"
        );
        assert!(asked.idle_choices.is_empty() && asked.clipboard_choices.is_empty());

        let wanted = asked.wanted();
        assert_eq!(wanted.theme, settings::Theme::Dark);
        assert_eq!(wanted.idle_seconds, 900);
        assert_eq!(wanted.clipboard_seconds, 15);
        assert!(!wanted.lock_on_sleep && wanted.lock_on_screen_lock);
    }

    #[test]
    fn an_identifier_that_is_not_one_names_no_entry() {
        assert!(entry_id("00000000-0000-0000-0000-000000000000").is_ok());
        for text in [
            "",
            "not a uuid",
            "00000000-0000-0000-0000-00000000000",
            "'; drop table entries; --",
            "../../../etc/passwd",
        ] {
            assert!(entry_id(text).is_err(), "{text:?}");
        }
    }

    /// What the screen is told about a copy and the vault it would go over is
    /// file names and times, never a path: the paths stay in Rust, which is
    /// where the move is made. The file it would go over is named as the
    /// snapshot it becomes, so the sentence that promises it is Rust's rule
    /// for snapshot names and not a second one in the window.
    #[test]
    fn a_copy_crosses_with_names_and_times_and_no_path() {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_790_000_000);
        let vault = std::path::Path::new("/Users/someone/Vault/work <b>.kdbx");

        let copy = serde_json::to_value(CopyOf::of(
            vault,
            vault_core::storage::OnDisk::Written(Some(at)),
            vault_core::storage::OnDisk::Gone,
        ))
        .expect("the copy serialises");
        assert_eq!(copy["vault"], "work <b>.kdbx");
        assert_eq!(copy["keptAs"], "work <b>.kdbx.1.bak");
        assert_eq!(copy["saved"], "2026-09-21T14:13:20Z");
        assert_eq!(copy["vaultFile"]["there"], false);
        assert!(!copy.to_string().contains("/Users"), "{copy}");

        let gone = serde_json::to_value(CopyOf::of(
            vault,
            vault_core::storage::OnDisk::Gone,
            vault_core::storage::OnDisk::Written(Some(at)),
        ))
        .expect("the copy serialises");
        assert_eq!(gone["saved"], serde_json::Value::Null);
        assert_eq!(gone["vaultFile"]["written"], "2026-09-21T14:13:20Z");

        let there = serde_json::to_value(OnDisk::of(vault_core::storage::OnDisk::Written(None)))
            .expect("the file serialises");
        assert_eq!(there, serde_json::json!({ "there": true, "written": null }));
        let missing = serde_json::to_value(OnDisk::of(vault_core::storage::OnDisk::Gone))
            .expect("the file serialises");
        assert_eq!(
            missing,
            serde_json::json!({ "there": false, "written": null })
        );
    }
}
