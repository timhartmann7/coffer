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
    pub entries: Vec<Entry>,
}

/// An entry, as much of it as can be looked at without a reveal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: EntryId,
    /// The group holding this entry.
    pub group: GroupId,
    /// Every field the entry has, standard and custom alike, in the order the
    /// database lists them.
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
        match self.field(name).map(|field| &field.value) {
            Some(FieldValue::Open(text)) => text,
            _ => "",
        }
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
