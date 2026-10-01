//! What Coffer says about a database without revealing anything.
//!
//! Everything here can cross a process boundary. Values that cannot are not in
//! these types at all: a protected field arrives as [`FieldValue::Protected`],
//! which carries no bytes, and the bytes come one at a time through
//! [`Vault::reveal`][crate::Vault::reveal].

use std::fmt;

use chrono::NaiveDateTime;
pub use keepass::db::{EntryId, GroupId};

/// A group of entries.
///
/// The top level of the tree is what Coffer calls a project, and every group
/// nested inside one is a section of it. The file makes no such distinction and
/// neither does this type; the words describe where a group sits, not what it
/// is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: GroupId,
    pub name: String,
    pub notes: Option<String>,
    /// True for the group the database nominates as its recycle bin. Deleted
    /// entries live there, and the tree shows it apart from the rest.
    pub is_recycle_bin: bool,
    /// When the group is in the recycle bin, what is known about how it got
    /// there. Nothing for the bin itself and for everything outside it.
    pub binned: Option<Binned>,
    /// What deleting the group would do.
    pub deletion: Deletion,
    /// The groups nested directly inside this one.
    pub sections: Vec<Project>,
    /// The entries held directly by this group. Entry history is not here: a
    /// previous version belongs to its entry and never appears in a tree, a
    /// list or a search.
    pub entries: Vec<EntrySummary>,
}

/// What a tree or a list may know about an entry.
///
/// The screen that shows one entry asks for the whole of it. A list of a
/// thousand gets the four things it filters on and nothing else: notes and
/// custom field values are the user's data and have no business travelling in
/// bulk to a screen that only wants to draw a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySummary {
    pub id: EntryId,
    pub group: GroupId,
    /// The three fields a list shows and filters on. One the database protects
    /// arrives without its value, like any other protected field.
    pub title: FieldValue,
    pub username: FieldValue,
    pub url: FieldValue,
    pub tags: Vec<String>,
    pub times: Timestamps,
    /// Whether there is a password worth revealing.
    pub has_password: bool,
    /// How many attachments the entry carries. Their names come with the entry
    /// itself.
    pub attachments: usize,
    /// How many previous versions the entry keeps.
    pub versions: usize,
    /// When the entry is in the recycle bin, what is known about how it got
    /// there. A row in the bin says when and where from.
    pub binned: Option<Binned>,
}

/// An entry, as much of it as can be looked at without a reveal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: EntryId,
    /// The group holding this entry.
    pub group: GroupId,
    /// Every field the entry has, standard and custom alike, ordered by name.
    /// The database stores them in a map and writes them in whatever order it
    /// iterates, so there is no file order to preserve.
    pub fields: Vec<Field>,
    pub attachments: Vec<Attachment>,
    pub tags: Vec<String>,
    pub times: Timestamps,
    /// How many previous versions this entry keeps.
    pub versions: usize,
    /// When the entry is in the recycle bin, what is known about how it got
    /// there.
    pub binned: Option<Binned>,
    /// What deleting the entry would do.
    pub deletion: Deletion,
}

/// What deleting something does, known before anybody asks for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deletion {
    /// It moves to the recycle bin, and can be put back from there.
    Bin,
    /// It goes out of the file for good: it is in the bin already, the
    /// database keeps no bin, or it is a folder the bin itself is inside.
    Forever,
}

/// What is known about something in the recycle bin.
///
/// Dates and a folder, none of it the reader's data, so it prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binned {
    /// When it went in. A folder takes everything in it along, so what sits
    /// inside a deleted folder went in when the folder did.
    pub since: Option<NaiveDateTime>,
    /// The deleted folder it went in with, the one the bin holds, or nothing
    /// for something deleted on its own.
    pub within: Option<GroupId>,
    /// Where putting it back takes it: the folder it was deleted from, or for
    /// something that went in with a folder, the one that folder was deleted
    /// from. Nothing when that is not known, is not there any more, or is in
    /// the bin itself, and putting it back takes it to the top of the vault
    /// instead.
    pub from: Option<GroupId>,
}

/// The KeePass field names Coffer treats as standard. Every other field an
/// entry carries is a custom field.
pub mod fields {
    pub use keepass::db::fields::{NOTES, PASSWORD, TITLE, URL, USERNAME};

    /// One of the five. Every entry is drawn with all of them whether or not
    /// the file gives it each one, so each is a field that can be written into
    /// on any entry.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Standard {
        Title,
        Username,
        Password,
        Url,
        Notes,
    }

    impl Standard {
        /// Which of the five a name is, or nothing for a custom field. The
        /// match is exact, as KeePass's is: `title` is a custom field.
        pub fn of(name: &str) -> Option<Standard> {
            match name {
                TITLE => Some(Standard::Title),
                USERNAME => Some(Standard::Username),
                PASSWORD => Some(Standard::Password),
                URL => Some(Standard::Url),
                NOTES => Some(Standard::Notes),
                _ => None,
            }
        }
    }
}

impl Entry {
    /// What a tree or a list may know about this entry.
    pub fn summary(&self) -> EntrySummary {
        EntrySummary {
            id: self.id,
            group: self.group,
            title: self.value_of(fields::TITLE),
            username: self.value_of(fields::USERNAME),
            url: self.value_of(fields::URL),
            tags: self.tags.clone(),
            times: self.times,
            has_password: self.has_password(),
            attachments: self.attachments.len(),
            versions: self.versions,
            binned: self.binned,
        }
    }

    fn value_of(&self, name: &str) -> FieldValue {
        self.field(name).map_or_else(
            || FieldValue::Open(String::new()),
            |field| field.value.clone(),
        )
    }

    /// The field with this name, if the entry has one.
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|field| field.name == name)
    }

    /// The entry's login, empty when it has none. Every named accessor goes
    /// through the same path, so there is one rule for what a field name means.
    pub fn username(&self) -> &str {
        self.open(fields::USERNAME)
    }

    pub fn url(&self) -> &str {
        self.open(fields::URL)
    }

    /// Whether the entry has a password worth revealing.
    pub fn has_password(&self) -> bool {
        self.field(fields::PASSWORD)
            .is_some_and(|field| !field.is_empty())
    }

    fn open(&self, name: &str) -> &str {
        self.field(name)
            .and_then(|field| field.value.open())
            .unwrap_or_default()
    }
}

/// One field of an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub value: FieldValue,
}

impl Field {
    /// Whether the field holds nothing. True for a protected field whose value
    /// is empty, which is one of the two things about a protected value that
    /// may be known without revealing it.
    pub fn is_empty(&self) -> bool {
        match &self.value {
            FieldValue::Open(text) => text.is_empty(),
            FieldValue::Protected { empty, .. } => *empty,
        }
    }

    /// Whether the value is written in lines, which is the other.
    pub fn in_lines(&self) -> bool {
        match &self.value {
            FieldValue::Open(text) => in_lines(text),
            FieldValue::Protected { lines, .. } => *lines,
        }
    }
}

/// Whether a value has a line break in it.
///
/// The format has no multi-line flag. Ten recovery codes are in lines because
/// there is a break between each of them, and a CR on its own is one as much
/// as a line feed is, the way a text area reads it.
pub(crate) fn in_lines(text: &str) -> bool {
    text.contains(['\n', '\r'])
}

/// A field's value, or the fact that it has one.
#[derive(Clone, PartialEq, Eq)]
pub enum FieldValue {
    /// The database does not mark this field protected, so its value travels
    /// with the rest of the metadata.
    Open(String),
    /// The database marks this field protected. The value is not here; it comes
    /// from [`Vault::reveal`][crate::Vault::reveal], one field at a time.
    Protected {
        /// Whether the protected value is empty. The screen needs to know
        /// whether there is anything to reveal.
        empty: bool,
        /// Whether the protected value has a line break in it. The screen
        /// needs to know how a new one is written: ten recovery codes are
        /// replaced in lines, where Return starts the next one rather than
        /// saving the first over all ten.
        lines: bool,
    },
}

impl FieldValue {
    /// The value when the database does not protect it, and `None` when it
    /// does. A protected value comes from
    /// [`Vault::reveal`][crate::Vault::reveal] and from nowhere else.
    pub fn open(&self) -> Option<&str> {
        match self {
            FieldValue::Open(text) => Some(text),
            FieldValue::Protected { .. } => None,
        }
    }
}

impl fmt::Debug for FieldValue {
    /// Prints the shape of the value and never the value. An unprotected field
    /// is not a secret by the database's own reckoning, but it is still the
    /// user's data and it has no business in a panic message.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldValue::Open(_) => f.write_str("Open([redacted])"),
            FieldValue::Protected { empty, lines } => {
                write!(f, "Protected {{ empty: {empty}, lines: {lines} }}")
            }
        }
    }
}

/// An attachment, without its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// The name the database gives it. It comes from the file and may contain
    /// anything at all, `/` and `..` included, so it is never used as a path
    /// without being made safe first.
    pub name: String,
    pub size: usize,
}

impl Attachment {
    /// A name to offer the save panel when this file is written out.
    ///
    /// The name in the database is somebody's text, not a path: the fixture
    /// alone carries `../../escape.txt` and `nested/path/name.txt`. Everything
    /// that could make it mean a directory is dropped rather than escaped, and
    /// a name that is nothing but those characters becomes a plain one, because
    /// a save panel that opens on an empty name is a save panel that saves the
    /// wrong thing.
    ///
    /// The user still chooses where the file goes and under what name. This is
    /// the suggestion, and it is the only place a name from a database is
    /// turned into one.
    pub fn file_name(&self) -> String {
        /// Long enough for any name a person types and short enough for every
        /// filesystem a Mac mounts.
        const LONGEST: usize = 200;

        let last = self
            .name
            .rsplit(['/', '\\', ':'])
            .next()
            .unwrap_or_default();

        let cleaned: String = last
            .chars()
            .filter(|character| !character.is_control())
            .take(LONGEST)
            .collect();
        let cleaned = cleaned.trim().trim_start_matches('.').trim();

        if cleaned.is_empty() {
            "attachment".to_owned()
        } else {
            cleaned.to_owned()
        }
    }
}

/// The dates an entry carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Timestamps {
    pub created: Option<NaiveDateTime>,
    pub modified: Option<NaiveDateTime>,
    /// When the entry expires, if it is set to expire at all.
    pub expires: Option<NaiveDateTime>,
}

/// A previous version of an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    /// Where the version sits in the entry's history, oldest first.
    pub index: usize,
    pub modified: Option<NaiveDateTime>,
}
