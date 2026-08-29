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
}

/// The KeePass field names Coffer treats as standard. Every other field an
/// entry carries is a custom field.
pub mod fields {
    pub use keepass::db::fields::{NOTES, PASSWORD, TITLE, URL, USERNAME};
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

    /// The entry's title, empty when it has none. Every other named accessor
    /// goes through the same path, so there is one rule for what a field name
    /// means.
    pub fn title(&self) -> &str {
        self.open(fields::TITLE)
    }

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
    /// is empty, which is the one thing about a protected value that may be
    /// known without revealing it.
    pub fn is_empty(&self) -> bool {
        match &self.value {
            FieldValue::Open(text) => text.is_empty(),
            FieldValue::Protected { empty } => *empty,
        }
    }
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
            FieldValue::Protected { empty } => write!(f, "Protected {{ empty: {empty} }}"),
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
