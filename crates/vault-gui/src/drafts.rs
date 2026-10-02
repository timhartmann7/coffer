//! What the reader is typing into an entry and has not finished.
//!
//! A value reaches the vault when its field is left. Until then it is only in
//! the window, and a lock destroys the window on triggers that cannot wait for
//! it: the Mac going to sleep, the screen locking and Coffer quitting all arrive
//! on the thread the window is drawn on, and neither a message into the page
//! nor its answer can get through while that thread is busy locking. So the
//! window is never asked to finish first. It says what is in a field as the
//! reader types, and a lock writes the last thing it heard into the vault
//! before wiping it.
//!
//! Every word the window says about a field carries a number from one count
//! that only goes up. Tauri runs commands side by side, so a word sent before
//! the value was written - or before the text was taken back, or the entry
//! deleted, or the file read again - can arrive after it. The number is how one
//! that has been overtaken is told apart, and it is dropped rather than brought
//! back over what overtook it.

use std::collections::HashMap;

use vault_core::model::EntryId;
use vault_core::{NewValue, Typing};
use zeroize::Zeroizing;

/// Text typed into one field and not yet written there.
pub struct Typed {
    pub value: Zeroizing<String>,
    /// Whether the database protects the field, as the window read it, so that
    /// a value the file keeps protected is written back protected.
    pub protect: bool,
    /// Whether it was typed into the field itself, or as a new value for it in
    /// a Change field of its own, which a lock never writes over a value.
    pub typing: Typing,
}

impl Typed {
    /// The value, in the shape a write takes, and how it was typed. Moved
    /// rather than copied: the text goes into the vault, and what is left
    /// behind is the empty buffer the wrapper wipes.
    fn into_value(mut self) -> (NewValue, Typing) {
        let value = if self.protect {
            NewValue::Protected(self.value)
        } else {
            NewValue::Open(std::mem::take(&mut *self.value))
        };
        (value, self.typing)
    }
}

impl std::fmt::Debug for Typed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Typed")
            .field("value", &"[redacted]")
            .field("protect", &self.protect)
            .field("typing", &self.typing)
            .finish()
    }
}

/// What a write leaves nothing more to say about.
pub enum Over<'a> {
    /// A value written into this field. The text typed there is in the vault
    /// now, or was refused and put back.
    Field(EntryId, &'a str),
    /// Entries deleted. The pane they were typed in has gone with them.
    Entries(&'a [EntryId]),
    /// The file read again, which throws away everything in the window.
    Everything,
}

/// The last word heard about one field.
struct Said {
    sequence: u64,
    /// What is in the field, or nothing when there is nothing to write: the
    /// text was taken back or written.
    typed: Option<Typed>,
}

/// Everything heard about one entry's fields.
#[derive(Default)]
struct Fields {
    /// The number of the last write that overtook every field of the entry.
    since: u64,
    said: HashMap<String, Said>,
}

/// What the reader is typing, field by field, with what it takes to tell a
/// late word from a new one.
///
/// A field whose text was written or taken back keeps the number of that word
/// and loses the text, so that anything older about it that turns up later is
/// known for what it is.
#[derive(Default)]
pub struct Drafts {
    /// The number of the last write that overtook everything.
    since: u64,
    entries: HashMap<EntryId, Fields>,
}

impl Drafts {
    /// Hears the window's latest word about a field: what is in it now, or
    /// nothing when what was typed was taken back.
    ///
    /// A word no newer than the last one about the field, its entry or the
    /// whole vault is dropped.
    pub fn hear(&mut self, entry: EntryId, field: &str, typed: Option<Typed>, sequence: u64) {
        if sequence <= self.since {
            return;
        }
        let fields = self.entries.entry(entry).or_default();
        if sequence <= fields.since
            || fields
                .said
                .get(field)
                .is_some_and(|said| sequence <= said.sequence)
        {
            return;
        }
        fields
            .said
            .insert(field.to_owned(), Said { sequence, typed });
    }

    /// Lets go of whatever typing a write overtakes. Only what was said before
    /// the write: a word sent after it and arrived first is about text the
    /// reader typed since, and stays.
    pub fn over(&mut self, over: Over<'_>, sequence: u64) {
        match over {
            Over::Field(entry, field) => self.hear(entry, field, None, sequence),
            Over::Entries(entries) => {
                for &entry in entries {
                    let fields = self.entries.entry(entry).or_default();
                    fields.since = fields.since.max(sequence);
                    fields.said.retain(|_, said| said.sequence > sequence);
                }
            }
            Over::Everything => {
                self.since = self.since.max(sequence);
                for fields in self.entries.values_mut() {
                    fields.said.retain(|_, said| said.sequence > sequence);
                }
            }
        }
    }

    /// Everything typed and not finished, in the order it was said, as the
    /// writes that would finish it. Nothing is left behind.
    pub fn take(&mut self) -> Vec<(EntryId, String, NewValue, Typing)> {
        let mut typed: Vec<(u64, EntryId, String, Typed)> = self
            .entries
            .drain()
            .flat_map(|(entry, fields)| {
                fields.said.into_iter().filter_map(move |(field, said)| {
                    said.typed.map(|typed| (said.sequence, entry, field, typed))
                })
            })
            .collect();
        typed.sort_by_key(|(sequence, ..)| *sequence);
        typed
            .into_iter()
            .map(|(_, entry, field, typed)| {
                let (value, typing) = typed.into_value();
                (entry, field, value, typing)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTES: &str = "Notes";

    fn typed(text: &str) -> Option<Typed> {
        Some(Typed {
            value: Zeroizing::new(text.to_owned()),
            protect: false,
            typing: Typing::InPlace,
        })
    }

    fn id(n: u128) -> EntryId {
        EntryId::from_uuid(uuid::Uuid::from_u128(n))
    }

    /// What a lock would write, as text.
    fn written(drafts: &mut Drafts) -> Vec<(EntryId, String, String)> {
        drafts
            .take()
            .into_iter()
            .map(|(entry, field, value, _)| {
                let text = match value {
                    NewValue::Open(text) => text,
                    NewValue::Protected(text) => text.to_string(),
                };
                (entry, field, text)
            })
            .collect()
    }

    /// The race the numbers exist for. A draft on its way when the field was
    /// left arrives after the value it was a draft of, and must not become what
    /// the lock writes.
    #[test]
    fn a_draft_that_arrives_after_its_field_was_written_is_dropped() {
        let mut drafts = Drafts::default();
        drafts.over(Over::Field(id(1), NOTES), 11);
        drafts.hear(id(1), NOTES, typed("half of it"), 10);
        assert!(written(&mut drafts).is_empty());
    }

    /// The same race the other way round: the reader typed again after the
    /// field was written, and the new draft got there first.
    #[test]
    fn a_draft_typed_after_a_write_outlives_the_write_that_arrives_late() {
        let mut drafts = Drafts::default();
        drafts.hear(id(1), NOTES, typed("typed again"), 12);
        drafts.over(Over::Field(id(1), NOTES), 11);
        assert_eq!(
            written(&mut drafts),
            vec![(id(1), NOTES.to_owned(), "typed again".to_owned())]
        );
    }

    /// Taken back is taken back, whichever of the two arrives first.
    #[test]
    fn text_taken_back_stays_taken_back() {
        let mut drafts = Drafts::default();
        drafts.hear(id(1), NOTES, typed("never mind"), 5);
        drafts.hear(id(1), NOTES, None, 6);
        assert!(written(&mut drafts).is_empty(), "in the order it was sent");

        let mut drafts = Drafts::default();
        drafts.hear(id(1), NOTES, None, 6);
        drafts.hear(id(1), NOTES, typed("never mind"), 5);
        assert!(written(&mut drafts).is_empty(), "the other way round");
    }

    /// Only the newest draft of a field is written, and two with the same
    /// number are one word heard twice.
    #[test]
    fn a_field_has_one_draft_and_it_is_the_newest() {
        let mut drafts = Drafts::default();
        drafts.hear(id(1), NOTES, typed("abc"), 3);
        drafts.hear(id(1), NOTES, typed("a"), 1);
        drafts.hear(id(1), NOTES, typed("ab"), 2);
        drafts.hear(id(1), NOTES, typed("abc again"), 3);
        assert_eq!(
            written(&mut drafts),
            vec![(id(1), NOTES.to_owned(), "abc".to_owned())]
        );
    }

    /// A deleted entry takes its drafts with it, and a draft of it still on the
    /// way does not bring them back - while one typed after the deletion was
    /// sent is left alone.
    #[test]
    fn a_deletion_overtakes_every_draft_of_its_entry_and_only_those() {
        let mut drafts = Drafts::default();
        drafts.hear(id(1), NOTES, typed("in the deleted entry"), 1);
        drafts.hear(id(1), "Title", typed("typed since"), 9);
        drafts.hear(id(2), NOTES, typed("in another entry"), 2);
        drafts.over(Over::Entries(&[id(1)]), 5);
        drafts.hear(id(1), "UserName", typed("on its way"), 4);

        assert_eq!(
            written(&mut drafts),
            vec![
                (id(2), NOTES.to_owned(), "in another entry".to_owned()),
                (id(1), "Title".to_owned(), "typed since".to_owned()),
            ]
        );
    }

    /// A batch deletion lets go of what was typed into every entry it names,
    /// and of nothing else: not another entry's typing, and not what was typed
    /// into one of them after the batch was sent.
    #[test]
    fn entries_overtaken_let_go_of_every_one_named_and_no_other() {
        let mut drafts = Drafts::default();
        drafts.hear(id(1), NOTES, typed("in the first"), 1);
        drafts.hear(id(2), NOTES, typed("in the second"), 2);
        drafts.hear(id(3), NOTES, typed("in the third"), 3);
        drafts.over(Over::Entries(&[id(1), id(2), id(1)]), 5);
        drafts.hear(id(2), "Title", typed("typed since"), 6);
        drafts.hear(id(1), "UserName", typed("on its way"), 4);

        assert_eq!(
            written(&mut drafts),
            vec![
                (id(3), NOTES.to_owned(), "in the third".to_owned()),
                (id(2), "Title".to_owned(), "typed since".to_owned()),
            ]
        );
    }

    /// Reading the file again throws away everything the window held, and
    /// nothing said before it comes back afterwards.
    #[test]
    fn reading_the_file_again_overtakes_everything_said_before_it() {
        let mut drafts = Drafts::default();
        for n in 1..=5u64 {
            drafts.hear(id(u128::from(n)), NOTES, typed("mine"), n);
        }
        drafts.over(Over::Everything, 10);
        drafts.hear(id(7), NOTES, typed("late"), 8);
        drafts.hear(id(8), NOTES, typed("after"), 11);
        assert_eq!(
            written(&mut drafts),
            vec![(id(8), NOTES.to_owned(), "after".to_owned())]
        );
    }

    /// What was said is written in the order it was said, and taking it leaves
    /// nothing for a second lock to write.
    #[test]
    fn drafts_are_taken_once_in_the_order_they_were_said() {
        let mut drafts = Drafts::default();
        drafts.hear(id(2), NOTES, typed("second"), 20);
        drafts.hear(id(1), NOTES, typed("first"), 10);
        drafts.hear(id(3), NOTES, typed("third"), 30);
        let order: Vec<String> = written(&mut drafts)
            .into_iter()
            .map(|(_, _, text)| text)
            .collect();
        assert_eq!(order, ["first", "second", "third"]);
        assert!(drafts.take().is_empty());
    }

    /// A protected field's draft is written protected, and an open one's open;
    /// a new value typed in a Change field goes to the lock as one.
    #[test]
    fn a_draft_is_written_under_the_protection_and_the_way_the_window_said() {
        let mut drafts = Drafts::default();
        drafts.hear(
            id(1),
            "PIN",
            Some(Typed {
                value: Zeroizing::new("4711".to_owned()),
                protect: true,
                typing: Typing::Beside,
            }),
            1,
        );
        drafts.hear(id(1), NOTES, typed("plain"), 2);
        let taken = drafts.take();
        assert!(matches!(
            taken.first(),
            Some((_, _, NewValue::Protected(_), Typing::Beside))
        ));
        assert!(matches!(
            taken.get(1),
            Some((_, _, NewValue::Open(_), Typing::InPlace))
        ));
    }

    /// Typing is the reader's own, and printing it prints none of it.
    #[test]
    fn typing_prints_nothing_of_itself() {
        let printed = format!(
            "{:?}",
            Typed {
                value: Zeroizing::new("passport C01X00T47".to_owned()),
                protect: true,
                typing: Typing::Beside,
            }
        );
        assert!(printed.contains("[redacted]"), "{printed}");
        assert!(!printed.contains("C01X00T47"), "{printed}");
    }
}
