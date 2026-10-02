//! What an entry holds, as opposed to where it sits, when it was touched, its
//! versions and its files.
//!
//! Two things put one entry's content onto another: a restore, which puts a
//! previous version back over its entry, and a copy, which puts an entry onto
//! one made for it. They take the same list from here, so a part of an entry
//! one of them carries and the other forgets cannot happen: a restore that
//! brought back the colours and a copy that left them behind would be two
//! answers to what an entry is.
//!
//! The files are not in the list. A file belongs to the pool the whole database
//! shares and an entry names it by a number (see `attachment.rs`), so what a
//! restore does with them and what a copy does with them are different rules,
//! each kept where the pool is.

use std::collections::HashMap;

use chrono::NaiveDateTime;
use keepass::db::{AutoType, Color, CustomDataItem, Entry, EntryMut, Icon, Value};

use crate::model::fields;

/// One entry's fields, tags, custom data, auto-type, colours, override URL,
/// quality check, expiry and icon, taken out of it.
///
/// No `Debug`: the fields are the reader's values, protected ones included
/// (see "Derived `Debug` prints secrets" in `docs/vault-core.md`).
pub(crate) struct Content {
    fields: HashMap<String, Value<String>>,
    tags: Vec<String>,
    custom_data: HashMap<String, CustomDataItem>,
    autotype: Option<AutoType>,
    foreground_color: Option<Color>,
    background_color: Option<Color>,
    override_url: Option<String>,
    quality_check: bool,
    /// Whether the entry expires and when are the one pair of dates a reader
    /// chose. When it was made, changed, looked at or moved is about this
    /// entry and nobody else's.
    expires: Option<bool>,
    expiry: Option<NaiveDateTime>,
    icon: Option<Icon>,
}

impl Content {
    /// What `entry` holds. A protected value stays protected: cloning one makes
    /// a box of its own, and nothing of it is exposed on the way.
    pub(crate) fn of(entry: &Entry) -> Content {
        Content {
            fields: entry.fields.clone(),
            tags: entry.tags.clone(),
            custom_data: entry.custom_data.clone(),
            autotype: entry.autotype.clone(),
            foreground_color: entry.foreground_color.clone(),
            background_color: entry.background_color.clone(),
            override_url: entry.override_url.clone(),
            quality_check: entry.quality_check,
            expires: entry.times.expires,
            expiry: entry.times.expiry,
            icon: entry.icon().cloned(),
        }
    }

    /// Gives the title another name made from the one it has, kept the way it
    /// was: a title the database protects stays protected, and the new name
    /// goes into the box with no second copy of it left behind. An entry with
    /// no title field is left with none.
    pub(crate) fn retitle(&mut self, rule: impl FnOnce(&str) -> String) {
        let Some(title) = self.fields.get_mut(fields::TITLE) else {
            return;
        };
        let named = rule(title.get());
        *title = if title.is_protected() {
            Value::protected(named)
        } else {
            Value::unprotected(named)
        };
    }

    /// Puts this over what `onto` holds. Nothing else about it changes: not
    /// its id, its folder, its dates but the expiry, its versions or its files.
    ///
    /// The icon goes through the library's setters, because it is not a field
    /// anybody can assign: an entry holds a back-reference in the custom icon
    /// it uses, and only the setters keep that right. A custom icon somebody
    /// deleted meanwhile leaves the entry with none rather than with a
    /// reference to nothing.
    pub(crate) fn put(self, onto: &mut EntryMut<'_>) {
        onto.fields = self.fields;
        onto.tags = self.tags;
        onto.custom_data = self.custom_data;
        onto.autotype = self.autotype;
        onto.foreground_color = self.foreground_color;
        onto.background_color = self.background_color;
        onto.override_url = self.override_url;
        onto.quality_check = self.quality_check;
        onto.times.expires = self.expires;
        onto.times.expiry = self.expiry;

        match self.icon {
            None => onto.set_icon_none(),
            Some(Icon::BuiltIn(icon)) => onto.set_icon_builtin(icon),
            Some(Icon::Custom(icon)) => {
                if onto.set_icon_custom(icon).is_err() {
                    onto.set_icon_none();
                }
            }
        }
    }
}
