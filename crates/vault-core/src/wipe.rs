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
    use std::cell::Cell;

    use keepass::db::{AutoType, AutoTypeAssociation, CustomDataItem, fields};

    use super::*;

    const OPEN: &str = "an unprotected value nothing upstream wipes";
    const TAG: &str = "a tag long enough to be worth finding";
    const FOLDER: &str = "a folder named after somebody's client";
    const NOTE: &str = "what the folder is for, in the user's own words";
    const CUSTOM: &str = "a custom field name is the user's text as well";
    const NAMED: &str = "the file this entry carries";
    const SEQUENCE: &str = "the keystrokes this entry types for somebody";
    const WINDOW: &str = "the window title those keystrokes are meant for";
    const ICON: &str = "a picture somebody chose for this folder";
    const WROTE: &str = "the client that wrote this database last";
    const ABOUT: &str = "what the person who made this vault says it is for";
    const USER: &str = "the login this vault uses when it has no other";

    /// Whether a value's whole buffer is zero, capacity and all.
    ///
    /// The pointer is derived from this borrow at the moment of reading rather
    /// than recorded before the wipe. The wipe takes a unique borrow of every
    /// one of these, which invalidates anything derived from the shared borrow
    /// that found it, and reading through that afterwards is undefined however
    /// plainly the bytes are still where they were.
    fn zeroed(text: &mut String) -> bool {
        // SAFETY: the pointer comes from the unique borrow held right here, so
        // it carries the buffer's own provenance. Every byte up to the capacity
        // is initialised: these values are all `to_owned` of a literal, where
        // capacity and length are the same, and `Zeroize` writes the whole of
        // it.
        let held = unsafe { std::slice::from_raw_parts(text.as_mut_ptr(), text.capacity()) };
        held.iter().all(|byte| *byte == 0)
    }

    fn zeroed_bytes(data: &mut Vec<u8>) -> bool {
        // SAFETY: as above.
        let held = unsafe { std::slice::from_raw_parts(data.as_mut_ptr(), data.capacity()) };
        held.iter().all(|byte| *byte == 0)
    }

    /// A database with something in every place the wipe writes over where it
    /// lies.
    fn peopled() -> Database {
        let mut database = Database::new();
        database.meta.generator = Some(WROTE.to_owned());
        database.meta.database_name = Some(OPEN.to_owned());
        database.meta.database_description = Some(ABOUT.to_owned());
        database.meta.default_username = Some(USER.to_owned());
        database.meta.custom_data.insert(CUSTOM.to_owned(), item());

        let group = database.root_mut().add_group().id();
        let entry = {
            let Some(mut folder) = database.group_mut(group) else {
                return database;
            };
            folder.name = FOLDER.to_owned();
            folder.notes = Some(NOTE.to_owned());
            folder.tags = vec![TAG.to_owned()];
            folder.default_autotype_sequence = Some(SEQUENCE.to_owned());
            folder.custom_data.insert(CUSTOM.to_owned(), item());

            let mut icon = folder.set_icon_custom_new(vec![0x89; 512]);
            icon.name = Some(ICON.to_owned());

            folder.add_entry().id()
        };

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
        entry.custom_data.insert(CUSTOM.to_owned(), item());
        entry.autotype = Some(AutoType {
            enabled: true,
            default_sequence: Some(SEQUENCE.to_owned()),
            associations: vec![AutoTypeAssociation {
                window: WINDOW.to_owned(),
                sequence: SEQUENCE.to_owned(),
            }],
            ..AutoType::default()
        });
        entry.add_attachment(NAMED, Value::Unprotected(vec![0x5a; 4096]));

        database
    }

    fn item() -> CustomDataItem {
        CustomDataItem {
            value: Some(CustomDataValue::String(OPEN.to_owned())),
            last_modification_time: None,
        }
    }

    /// How many of the places the wipe writes over where they lie are not zero.
    ///
    /// Counted rather than asserted one by one, so that the same walk says both
    /// "there is something here to wipe" before and "there is nothing left"
    /// after. An arm of the wipe that was deleted shows up as a place still
    /// holding its value.
    fn unwiped(database: &mut Database) -> usize {
        let left = Cell::new(0);
        let text = |value: &mut String| {
            if !zeroed(value) {
                left.set(left.get() + 1);
            }
        };

        database.foreach_group_mut(|mut group| {
            if !group.name.is_empty() {
                text(&mut group.name);
            }
            if let Some(notes) = group.notes.as_mut() {
                text(notes);
            }
            for tag in &mut group.tags {
                text(tag);
            }
            if let Some(sequence) = group.default_autotype_sequence.as_mut() {
                text(sequence);
            }
        });

        database.foreach_entry_mut(|mut entry| {
            for tag in &mut entry.tags {
                text(tag);
            }
            if let Some(url) = entry.override_url.as_mut() {
                text(url);
            }
            if let Some(autotype) = entry.autotype.as_mut() {
                if let Some(sequence) = autotype.default_sequence.as_mut() {
                    text(sequence);
                }
                for association in &mut autotype.associations {
                    text(&mut association.window);
                    text(&mut association.sequence);
                }
            }
        });

        database.foreach_attachment_mut(|mut attachment| {
            if let Value::Unprotected(data) = &mut attachment.data
                && !zeroed_bytes(data)
            {
                left.set(left.get() + 1);
            }
        });

        database.foreach_custom_icon_mut(|mut icon| {
            if !zeroed_bytes(&mut icon.data) {
                left.set(left.get() + 1);
            }
            if let Some(name) = icon.name.as_mut() {
                text(name);
            }
        });

        for held in [
            &mut database.meta.generator,
            &mut database.meta.database_name,
            &mut database.meta.database_description,
            &mut database.meta.default_username,
        ] {
            if let Some(value) = held.as_mut() {
                text(value);
            }
        }

        left.get()
    }

    /// Every place the wipe writes over where it lies. Exact on purpose: a
    /// field the library adds, or a place this module stops covering, changes
    /// it, and a test that only asserted "all of what I found" would go on
    /// passing while finding less.
    const PLACES: usize = 16;

    /// The claim, made against the buffers themselves: after the wipe, the
    /// bytes that held a value are zero.
    #[test]
    fn every_value_left_where_it_lies_is_written_over() {
        let mut held = peopled();
        assert_eq!(
            unwiped(&mut held),
            PLACES,
            "the database under test does not hold something in every place"
        );

        database(&mut held);

        assert_eq!(
            unwiped(&mut held),
            0,
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

        // The maps are not empty to begin with, or emptiness afterwards would
        // say nothing.
        assert!(
            held.iter_all_entries()
                .all(|entry| !entry.fields.is_empty())
        );
        assert!(
            held.iter_all_entries()
                .all(|entry| !entry.custom_data.is_empty())
        );
        assert!(!held.meta.custom_data.is_empty());

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

    /// A vault the size of a real one, nested as deep as the attack list asks
    /// for. The walk collects its ids before it starts rather than recursing,
    /// so a hundred folders inside each other is arithmetic rather than a
    /// hundred stack frames.
    #[test]
    fn a_large_and_deeply_nested_database_is_emptied_without_running_out_of_stack() {
        const DEEP: usize = 100;
        const WIDE: usize = 2_000;

        let mut held = Database::new();
        let mut parent = held.root().id();
        for depth in 0..DEEP {
            let made = held
                .group_mut(parent)
                .map(|mut group| group.add_group().id())
                .unwrap_or(parent);
            if let Some(mut group) = held.group_mut(made) {
                group.name = format!("{FOLDER} {depth}");
            }
            parent = made;
        }

        for _ in 0..WIDE {
            if let Some(mut group) = held.group_mut(parent) {
                group.add_entry().edit(|entry| {
                    entry.set_unprotected(fields::NOTES, OPEN);
                    entry.set_protected(fields::PASSWORD, "a password");
                });
            }
        }

        assert_eq!(held.num_entries(), WIDE);
        assert_eq!(held.num_groups(), DEEP + 1);

        database(&mut held);

        assert!(held.iter_all_entries().all(|entry| entry.fields.is_empty()));
        assert!(held.iter_all_groups().all(|group| group.name.is_empty()));
    }
}
