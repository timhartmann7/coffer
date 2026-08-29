//! Emptying a decrypted database while Coffer still owns it.
//!
//! [`crate::scrub`] covers what has already been handed back to the allocator.
//! This covers what has not: the tree the vault is holding at the moment it is
//! locked. Between them there is nothing left to find.
//!
//! Every write goes through `Zeroize`, which is a volatile store per byte
//! followed by a barrier and covers the whole capacity of a buffer rather than
//! its length. A plain `fill(0)` would be a store the optimiser is entitled to
//! remove, because the value is never read again.
//!
//! Wherever the library allows it a value is written over **where it lies** and
//! the buffer is left in place: emptied rather than dropped. That is not
//! tidiness, it is what makes the wipe checkable - a buffer that is still this
//! process's can be read back and asserted to be zero, and a buffer that has
//! been freed can only be looked for.
//!
//! The maps are the exception: a map hands its keys out by shared reference,
//! and the name of a custom field is the user's text as much as its value is,
//! so the field map and the three custom data maps are drained rather than
//! walked. What comes out of them is written over and then dropped, and the
//! allocator writes over it again on the way out.
//!
//! Two things cannot be reached at all. An entry's attachment *names* live in a
//! map the library keeps private, and the unprotected halves of previous
//! versions live in a `History` whose entries it also keeps private. Both go
//! when the database is dropped.

use std::collections::HashMap;

use keepass::Database;
use keepass::db::{CustomDataItem, CustomDataValue, Value};
use zeroize::Zeroize;

/// Writes over every value in the database.
///
/// What is left afterwards is a shape with nothing in it: field names gone,
/// values gone, protection flags gone with them. It is deliberately not
/// something Coffer can ask for. Saving a wiped database would write that
/// emptiness over the user's file, which is the first hard rule broken as
/// completely as it can be, so the only caller is the destructor.
pub(crate) fn database(database: &mut Database) {
    database.foreach_entry_mut(|mut entry| {
        for (mut name, mut value) in std::mem::take(&mut entry.fields) {
            name.zeroize();
            text(&mut value);
        }

        for tag in &mut entry.tags {
            tag.zeroize();
        }
        if let Some(url) = entry.override_url.as_mut() {
            url.zeroize();
        }
        custom(&mut entry.custom_data);

        if let Some(autotype) = entry.autotype.as_mut() {
            if let Some(sequence) = autotype.default_sequence.as_mut() {
                sequence.zeroize();
            }
            for association in &mut autotype.associations {
                association.window.zeroize();
                association.sequence.zeroize();
            }
        }

        // The library offers no mutable way into a previous version, so this is
        // the whole of what can be done here: dropping the block wipes every
        // protected value in it by itself, and the allocator wipes the rest.
        entry.history = None;
    });

    database.foreach_group_mut(|mut group| {
        group.name.zeroize();
        if let Some(notes) = group.notes.as_mut() {
            notes.zeroize();
        }
        for tag in &mut group.tags {
            tag.zeroize();
        }
        if let Some(sequence) = group.default_autotype_sequence.as_mut() {
            sequence.zeroize();
        }
        custom(&mut group.custom_data);
    });

    database.foreach_attachment_mut(|mut attachment| bytes(&mut attachment.data));

    database.foreach_custom_icon_mut(|mut icon| {
        icon.data.zeroize();
        if let Some(name) = icon.name.as_mut() {
            name.zeroize();
        }
    });

    let meta = &mut database.meta;
    for held in [
        &mut meta.generator,
        &mut meta.database_name,
        &mut meta.database_description,
        &mut meta.default_username,
    ] {
        if let Some(value) = held.as_mut() {
            value.zeroize();
        }
    }
    custom(&mut meta.custom_data);
}

/// A field value, whichever kind it is.
///
/// The protected arm replaces rather than reaches inside: the library holds
/// those in a `SecretBox` that wipes itself when it is dropped, and assigning
/// over the value is what drops it. Reaching inside would mean depending on
/// `secrecy` directly and betting that Cargo picks the same copy of it that the
/// library did.
fn text(value: &mut Value<String>) {
    match value {
        Value::Unprotected(held) => held.zeroize(),
        Value::Protected(_) => *value = Value::Unprotected(String::new()),
    }
}

fn bytes(value: &mut Value<Vec<u8>>) {
    match value {
        Value::Unprotected(held) => held.zeroize(),
        Value::Protected(_) => *value = Value::Unprotected(Vec::new()),
    }
}

fn custom(data: &mut HashMap<String, CustomDataItem>) {
    for (mut name, item) in std::mem::take(data) {
        name.zeroize();
        match item.value {
            Some(CustomDataValue::String(mut held)) => held.zeroize(),
            Some(CustomDataValue::Binary(mut held)) => held.zeroize(),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use keepass::db::{CustomDataItem, fields};

    use super::*;

    /// Where a value is, so that the same bytes can be read back after the
    /// value that owned them has been emptied.
    ///
    /// Zeroizing keeps the buffer: it writes over the whole capacity and sets
    /// the length to nothing, and the value goes on owning what it owned. That
    /// is what makes this assertion exact rather than a search - it looks at
    /// the one place the value was, rather than hunting for it.
    struct Where {
        at: *const u8,
        len: usize,
    }

    impl Where {
        fn of(bytes: &[u8]) -> Where {
            assert!(!bytes.is_empty(), "an empty value proves nothing");
            Where {
                at: bytes.as_ptr(),
                len: bytes.len(),
            }
        }

        /// Whether the buffer this value was in is all zero now.
        fn emptied(&self) -> bool {
            // SAFETY: the buffer belongs to a value that is alive for the whole
            // of the test - the wipe writes over it and never frees it - so the
            // range is mapped and initialised.
            let held = unsafe { std::slice::from_raw_parts(self.at, self.len) };
            held.iter().all(|byte| *byte == 0)
        }
    }

    const OPEN: &str = "an unprotected value nothing upstream wipes";
    const TAG: &str = "a tag long enough to be worth finding";
    const FOLDER: &str = "a folder named after somebody's client";
    const NOTE: &str = "what the folder is for, in the user's own words";
    const CUSTOM: &str = "a custom field name is the user's text as well";
    const NAMED: &str = "the file this entry carries";

    /// A database with something in every place a value can be.
    fn peopled() -> Database {
        let mut database = Database::new();
        database.meta.database_name = Some(OPEN.to_owned());
        database.meta.custom_data.insert(
            CUSTOM.to_owned(),
            CustomDataItem {
                value: Some(CustomDataValue::String(OPEN.to_owned())),
                last_modification_time: None,
            },
        );

        let group = database.root_mut().add_group().id();
        let Some(mut folder) = database.group_mut(group) else {
            return database;
        };
        folder.name = FOLDER.to_owned();
        folder.notes = Some(NOTE.to_owned());
        folder.tags = vec![TAG.to_owned()];

        let entry = folder.add_entry().id();
        let Some(mut entry) = database.entry_mut(entry) else {
            return database;
        };
        entry.edit(|written| {
            written.set_unprotected(fields::NOTES, OPEN);
            written.set_unprotected(CUSTOM, OPEN);
            written.set_protected(fields::PASSWORD, "a password");
        });
        entry.tags = vec![TAG.to_owned()];
        entry.override_url = Some(OPEN.to_owned());
        entry.add_attachment(NAMED, Value::Unprotected(vec![0x5a; 4096]));

        database
    }

    /// Every buffer the wipe writes over where it lies, which is everything the
    /// library gives mutable access to by anything other than a map key.
    fn in_place(database: &Database) -> Vec<Where> {
        let mut found = Vec::new();
        if let Some(name) = database.meta.database_name.as_deref() {
            found.push(Where::of(name.as_bytes()));
        }
        for group in database.iter_all_groups() {
            if !group.name.is_empty() {
                found.push(Where::of(group.name.as_bytes()));
            }
            if let Some(notes) = group.notes.as_deref() {
                found.push(Where::of(notes.as_bytes()));
            }
            for tag in &group.tags {
                found.push(Where::of(tag.as_bytes()));
            }
        }
        for entry in database.iter_all_entries() {
            for tag in &entry.tags {
                found.push(Where::of(tag.as_bytes()));
            }
            if let Some(url) = entry.override_url.as_deref() {
                found.push(Where::of(url.as_bytes()));
            }
        }
        for attachment in database.iter_all_attachments() {
            if let Value::Unprotected(data) = &attachment.data {
                found.push(Where::of(data));
            }
        }
        found
    }

    /// The claim, made against the buffers themselves: after the wipe, the
    /// bytes that held a value are zero.
    ///
    /// The count is exact on purpose. A field the library adds, or a place this
    /// module stops covering, changes it, and a test that only asserted "all of
    /// what I found" would go on passing while finding less.
    #[test]
    fn every_value_left_where_it_lies_is_written_over() {
        let mut held = peopled();
        let record = in_place(&held);

        assert_eq!(
            record.len(),
            7,
            "the database under test has to hold something in every place"
        );
        assert!(
            record.iter().all(|value| !value.emptied()),
            "the values are not in the database this is about to wipe"
        );

        database(&mut held);

        assert!(
            record.iter().all(Where::emptied),
            "a value survived the wipe in the buffer it was in"
        );
    }

    /// The names are the half that has to be dropped rather than emptied,
    /// because a map hands its keys out by shared reference. What can be
    /// asserted here is that they are gone from the database; that their
    /// buffers are zero is what the sweep in `tests/vault/wipe.rs` measures.
    #[test]
    fn a_wiped_database_holds_nothing_that_could_be_written_back() {
        let mut held = peopled();
        database(&mut held);

        assert!(held.iter_all_entries().all(|entry| entry.fields.is_empty()));
        assert!(
            held.iter_all_entries()
                .all(|entry| entry.custom_data.is_empty())
        );
        assert!(held.iter_all_entries().all(|entry| entry.history.is_none()));
        assert!(held.iter_all_groups().all(|group| group.name.is_empty()));
        assert!(
            held.iter_all_groups()
                .all(|group| group.custom_data.is_empty())
        );
        assert!(
            held.iter_all_attachments()
                .all(|attachment| attachment.data.get().is_empty())
        );
        assert!(held.meta.custom_data.is_empty());
        assert_eq!(held.meta.database_name.as_deref(), Some(""));
    }
}
