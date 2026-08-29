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
            name: taken
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            index: taken.index,
            taken: taken.taken.and_then(moment),
        }
    }
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
    pub unlocked: bool,
    /// How many entries the vault holds, the recycle bin's included.
    pub entries: usize,
    /// Whether there is a change in the window that the file does not have.
    pub dirty: bool,
    /// Whether this database can be written back at all. A snapshot and a
    /// format Coffer will not write are both read only.
    pub read_only: bool,
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
    #[serde(skip_deserializing)]
    pub idle_choices: Vec<u64>,
    #[serde(skip_deserializing)]
    pub clipboard_choices: Vec<u64>,
}

impl Settings {
    pub fn of(held: settings::Settings) -> Settings {
        Settings {
            idle_seconds: held.idle_seconds,
            clipboard_seconds: held.clipboard_seconds,
            lock_on_sleep: held.lock_on_sleep,
            lock_on_screen_lock: held.lock_on_screen_lock,
            idle_choices: settings::IDLE_CHOICES.to_vec(),
            clipboard_choices: settings::CLIPBOARD_CHOICES.to_vec(),
        }
    }

    /// What the window asked for. The lists it was sent do not come back: they
    /// are this side's to decide, and a message naming others would be the
    /// window choosing what it may choose.
    pub fn wanted(&self) -> settings::Settings {
        settings::Settings {
            idle_seconds: self.idle_seconds,
            clipboard_seconds: self.clipboard_seconds,
            lock_on_sleep: self.lock_on_sleep,
            lock_on_screen_lock: self.lock_on_screen_lock,
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
    pub sections: Vec<Group>,
    pub entries: Vec<EntryRow>,
}

impl Group {
    pub fn of(project: &model::Project) -> Group {
        Group {
            id: project.id.to_string(),
            name: project.name.clone(),
            is_recycle_bin: project.is_recycle_bin,
            sections: project.sections.iter().map(Group::of).collect(),
            entries: project.entries.iter().map(EntryRow::of).collect(),
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
        });

        assert_eq!(
            json(&row),
            r#"{"id":"00000000-0000-0000-0000-000000000000","group":"00000000-0000-0000-0000-000000000000","title":"node-3","username":"deploy","url":null,"tags":["prod","ssh"],"modified":"2026-03-12T18:42:00Z","hasPassword":true,"attachments":2}"#
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
}
