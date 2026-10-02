//! Making entries: of a kind, from one of the vault's templates, and as a copy
//! of another.
//!
//! All three make a new entry in a folder the reader can work in, so all three
//! ask the question a move asks of where it is going (`Vault::destination`):
//! a folder that is there, and neither the recycle bin nor anything inside it.
//! What is in the bin is deleted, and something new appearing there would be
//! deleted without anybody deleting it.
//!
//! A template and a copy are the same act - an entry's content put onto a new
//! one - and differ only in what the new one is called and where it goes. The
//! content is [`Content`], the list a restore puts back, and the files are
//! copied as files of the copy's own.

use keepass::db::{EntryId, GroupId};

use super::{Vault, dated};
use crate::attachment;
use crate::bin::Bin;
use crate::content::Content;
use crate::error::VaultError;
use crate::kind::Kind;
use crate::templates;

/// What goes after an entry's title to name its copy.
const COPY: &str = "copy";

impl Vault {
    /// Makes an entry of a kind in a folder.
    ///
    /// The five fields the format names are written empty, protected as the
    /// database asks for them to be, so that an entry Coffer made looks like an
    /// entry KeePassXC made and the screen has every row to edit. Then the
    /// fields the kind adds, empty, each hidden or not as the kind says; its
    /// tag; and its icon, written even for a login, whose key is what every
    /// client draws for an entry with none.
    ///
    /// Refused, making nothing, for a folder that is not there and for the
    /// recycle bin or anything in it ([`VaultError::IntoRecycleBin`]).
    pub fn create_entry(&mut self, group: GroupId, kind: Kind) -> Result<EntryId, VaultError> {
        self.writable()?;
        self.destination(&Bin::of(&self.database), group)?;
        let protection = self.protection();

        let made = {
            let mut group = self
                .database
                .group_mut(group)
                .ok_or(VaultError::NoSuchGroup)?;
            let mut entry = group.add_entry();
            dated(&mut entry.times);
            // No slot takes a standard name (`kind.rs` tests it), so the two
            // lists write side by side and neither writes over the other.
            let slots = kind.slots().iter().map(|slot| (slot.name, slot.protect));
            for (name, protect) in protection.into_iter().chain(slots) {
                if protect {
                    entry.set_protected(name, "");
                } else {
                    entry.set_unprotected(name, "");
                }
            }
            entry.tags = kind.tag().map(str::to_owned).into_iter().collect();
            entry.set_icon_builtin(kind.icon());
            entry.id()
        };

        self.touched();
        Ok(made)
    }

    /// Makes a copy of an entry beside it, in the same folder, called what it
    /// is called with " copy" after it.
    ///
    /// The copy holds everything the entry holds (see [`Content`]), each value
    /// under the protection it has, and a file of its own for every file on the
    /// entry. It is a new entry and nothing else of the original's: a new id,
    /// no versions, and every date now but the expiry, which is something the
    /// reader chose. The original is not touched, and writes no version.
    ///
    /// An entry with no title makes a copy with none. "Untitled" is what the
    /// window draws for one, and " copy" after it would put the window's word
    /// for nothing into the file. Copies are not numbered: a title is not a
    /// key, and the copy opens with its title ready to be typed over.
    ///
    /// Refused, making nothing, for an entry that is not there and for one in
    /// the recycle bin, where its copy would be a deleted entry nobody deleted
    /// ([`VaultError::IntoRecycleBin`]).
    pub fn duplicate_entry(&mut self, id: EntryId) -> Result<EntryId, VaultError> {
        self.writable()?;
        let into = self
            .database
            .entry(id)
            .ok_or(VaultError::NoSuchEntry)?
            .parent()
            .id();
        self.copy_into(id, into, copy_title)
    }

    /// Makes an entry from one of the vault's templates, in a folder: a copy of
    /// the template on the terms of [`Vault::duplicate_entry`], keeping its
    /// title, which is what the template calls what is made from it.
    ///
    /// Only an entry the templates group holds itself is a template (see
    /// `templates.rs`); anything else is refused with
    /// [`VaultError::NotATemplate`], making nothing. So is a folder that is
    /// not there, or one in the recycle bin.
    pub fn create_from_template(
        &mut self,
        template: EntryId,
        group: GroupId,
    ) -> Result<EntryId, VaultError> {
        self.writable()?;
        if self.database.entry(template).is_none() {
            return Err(VaultError::NoSuchEntry);
        }
        if !templates::holds(&self.database, template) {
            return Err(VaultError::NotATemplate);
        }
        self.copy_into(template, group, str::to_owned)
    }

    /// Puts what `from` holds onto a new entry in `into`, with the title
    /// `title` makes of its own, and its files after it.
    ///
    /// No version is written, on either entry: the new one has no past, and
    /// the one it came from did not change.
    fn copy_into(
        &mut self,
        from: EntryId,
        into: GroupId,
        title: impl FnOnce(&str) -> String,
    ) -> Result<EntryId, VaultError> {
        self.destination(&Bin::of(&self.database), into)?;
        let mut content = {
            let source = self.database.entry(from).ok_or(VaultError::NoSuchEntry)?;
            Content::of(&source)
        };
        content.retitle(title);

        let made = {
            let mut group = self
                .database
                .group_mut(into)
                .ok_or(VaultError::NoSuchGroup)?;
            let mut entry = group.add_entry();
            content.put(&mut entry);
            // After the content, which carries the expiry it was given: only
            // one that has none is given the date it was made.
            dated(&mut entry.times);
            entry.id()
        };
        attachment::copy(&mut self.database, from, made);

        self.touched();
        Ok(made)
    }
}

/// What a copy of an entry titled `title` is called.
///
/// Built at its full length at once, not grown: a string that outgrows its
/// buffer moves to a larger one and leaves what it held so far behind in the
/// old one, unwiped, and a title the database protects is a secret. `format!`
/// starts small and grows.
fn copy_title(title: &str) -> String {
    if title.is_empty() {
        return String::new();
    }
    let mut named = String::with_capacity(title.len() + 1 + COPY.len());
    named.push_str(title);
    named.push(' ');
    named.push_str(COPY);
    named
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name that never grew is one whose buffer is exactly as long as it
    /// is: `String::with_capacity` gives exactly what it is asked for, and
    /// anything that grew would have doubled past it.
    #[test]
    fn a_copys_name_is_written_once_into_a_buffer_of_its_own_size() {
        let long = "x".repeat(1 << 20);
        for title in [
            "Gmail",
            "a",
            "\u{202e}gnp.exe",
            "Bank card \u{1f4b3}",
            long.as_str(),
        ] {
            let named = copy_title(title);
            assert_eq!(named, format!("{title} copy"));
            assert_eq!(named.capacity(), named.len(), "{} bytes grew", title.len());
        }
        assert_eq!(copy_title(""), "");
    }
}
