//! A file offered to an entry under a name the entry already gives another.
//!
//! An entry keys its files by name, so one entry cannot hold two files under
//! one name, and the library's answer to a second is to drop the first. Every
//! scan a phone makes is `Scanned Document.pdf` and every SSH key is
//! `id_ed25519`, so the second page of a passport took the place of the first
//! without a word, and with no version to find it in afterwards. Nothing here
//! replaces anything: it says what is already there and what the new file could
//! be called beside it, and the reader chooses.

use std::collections::HashSet;

use keepass::Database;
use keepass::db::EntryId;

/// A name the entry already gives a file, and what a new file offered under it
/// could go by instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clash {
    /// How large the file already there is, in bytes. Two files by one name
    /// are told apart by their size before anything else.
    pub size: usize,
    /// The name the new file takes if both are kept.
    pub free: String,
}

/// What the entry holds under `name`, when it holds anything.
///
/// The name to the byte, letter case included. The format tells `Scan.pdf`
/// from `scan.pdf`, and so does the library, which drops a file only for a
/// name that is the same one: a name that differs by case is a second file,
/// and adding it loses nothing.
pub(crate) fn clash(database: &Database, entry: EntryId, name: &str) -> Option<Clash> {
    let found = database.entry(entry)?;
    let there = found.attachment_by_name(name)?;
    Some(Clash {
        size: there.data.get().len(),
        free: free(database, entry, name),
    })
}

/// The name a file offered as `name` can have on the entry without taking one
/// away: the name itself when the entry does not use it, and otherwise the name
/// with the first number free put before its last extension.
pub(crate) fn free(database: &Database, entry: EntryId, name: &str) -> String {
    let Some(found) = database.entry(entry) else {
        return name.to_owned();
    };
    let names: Vec<&str> = found.attachments_named().map(|(held, _)| held).collect();
    beside(name, &names)
}

/// The rule behind [`free`], over the names an entry already uses - its files',
/// or its fields' for a field Coffer names itself.
///
/// A number is free only when no name on the entry matches it with letter case
/// ignored. The format would hold `Scan 2.pdf` beside `scan 2.pdf`, but a Mac's
/// disk would not, and a name Coffer makes up should never be the one the
/// reader cannot write out next to its neighbour.
///
/// The number goes after whatever the name already says, and is never read out
/// of it. `Tax return 2025.pdf` is a year and not a count, and a copy of it
/// named `Tax return 2026.pdf` would be a document that lies about itself; so a
/// second `Scan 2.pdf` is `Scan 2 2.pdf`, which is odd to look at and true.
pub(crate) fn beside(name: &str, names: &[&str]) -> String {
    if !names.contains(&name) {
        return name.to_owned();
    }

    let used: HashSet<String> = names.iter().map(|held| held.to_lowercase()).collect();
    let (stem, extension) = split(name);
    let numbered = |number: usize| format!("{stem} {number}{extension}");

    // Every name can stand in the way of one number at most, so one number more
    // than there are names always reaches a free one. Refusing to count past
    // that rather than trusting an endless range keeps the ban on panics whole.
    let last = names.len() + 2;
    (2..=last)
        .map(numbered)
        .find(|candidate| !used.contains(&candidate.to_lowercase()))
        .unwrap_or_else(|| numbered(last))
}

/// A name cut before its last extension: `archive.tar.gz` is `archive.tar`
/// and `.gz`. The last one, because that is the one that says what opens the
/// file, and a copy that no longer opens in the same application is not the
/// same document kept twice.
///
/// Three kinds of dot do not start one, and the number goes at the end of the
/// name instead. The dot a name starts with is part of the name - `.env` is a file
/// called `.env`, not an empty one of type `env`. A dot with nothing after it
/// ends the name. And a dot followed by a space is punctuation: in
/// `Dr. Smith letter` there is no type called ` Smith letter`, and cutting
/// there would put the number in the middle of a sentence.
fn split(name: &str) -> (&str, &str) {
    let Some(at) = name.rfind('.') else {
        return (name, "");
    };
    let Some((stem, extension)) = name.split_at_checked(at) else {
        return (name, "");
    };
    let dotfile = stem.trim_start_matches('.').is_empty();
    let ending = extension == ".";
    let punctuation = extension.chars().any(char::is_whitespace);
    if dotfile || ending || punctuation {
        return (name, "");
    }
    (stem, extension)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shapes a real name takes, each with the one thing about it that
    /// could go wrong.
    #[test]
    fn a_second_file_is_numbered_before_its_last_extension() {
        for (held, offered, wanted) in [
            (
                vec!["Scanned Document.pdf"],
                "Scanned Document.pdf",
                "Scanned Document 2.pdf",
            ),
            // No extension at all: an SSH key.
            (vec!["id_ed25519"], "id_ed25519", "id_ed25519 2"),
            // A dotfile is a name, not an extension.
            (vec![".env"], ".env", ".env 2"),
            (vec![".env.local"], ".env.local", ".env 2.local"),
            (vec![".."], "..", ".. 2"),
            (vec!["..hidden"], "..hidden", "..hidden 2"),
            // Only the last extension counts.
            (vec!["archive.tar.gz"], "archive.tar.gz", "archive.tar 2.gz"),
            (vec!["a.b.c.d"], "a.b.c.d", "a.b.c 2.d"),
            // A trailing dot is not an extension, and neither is a sentence.
            (vec!["file."], "file.", "file. 2"),
            (
                vec!["Dr. Smith letter"],
                "Dr. Smith letter",
                "Dr. Smith letter 2",
            ),
            (vec!["v1.2 notes.txt"], "v1.2 notes.txt", "v1.2 notes 2.txt"),
            (vec!["tab.\there"], "tab.\there", "tab.\there 2"),
            // A number already there is part of the name, and a year stays one.
            (vec!["Scan 2.pdf"], "Scan 2.pdf", "Scan 2 2.pdf"),
            (
                vec!["Tax return 2025.pdf"],
                "Tax return 2025.pdf",
                "Tax return 2025 2.pdf",
            ),
            // A name out of another client's database, which is text and not a
            // path: nothing about it is cleaned here.
            (
                vec!["../../escape.txt"],
                "../../escape.txt",
                "../../escape 2.txt",
            ),
            (
                vec!["ユニコード 🔐.txt"],
                "ユニコード 🔐.txt",
                "ユニコード 🔐 2.txt",
            ),
            (
                vec!["evil\u{202e}fdp.exe"],
                "evil\u{202e}fdp.exe",
                "evil\u{202e}fdp 2.exe",
            ),
        ] {
            assert_eq!(beside(offered, &held), wanted, "{offered:?}");
        }
    }

    /// The name that would come next is taken too, by the file or by one that
    /// differs from it only in letter case.
    #[test]
    fn a_free_name_that_is_taken_is_passed_over() {
        assert_eq!(
            beside("Scan.pdf", &["Scan.pdf", "Scan 2.pdf"]),
            "Scan 3.pdf"
        );
        assert_eq!(
            beside("Scan.pdf", &["Scan.pdf", "scan 2.pdf"]),
            "Scan 3.pdf"
        );
        assert_eq!(
            beside(
                "Scan.pdf",
                &["Scan.pdf", "SCAN 2.PDF", "Scan 3.pdf", "Scan 5.pdf"]
            ),
            "Scan 4.pdf"
        );
    }

    /// A name the entry does not use is free as it stands, including one that
    /// differs from a name it does use only by letter case: that is a second
    /// file to the format, and nothing is lost by adding it.
    #[test]
    fn a_name_the_entry_does_not_use_is_kept_as_it_is() {
        assert_eq!(beside("Scan.pdf", &[]), "Scan.pdf");
        assert_eq!(beside("scan.pdf", &["Scan.pdf"]), "scan.pdf");
        assert_eq!(beside("Scan 2.pdf", &["Scan.pdf"]), "Scan 2.pdf");
    }

    /// Two hundred files already counted up from one name: the next number is
    /// found, and the search ends.
    #[test]
    fn a_long_run_of_numbered_names_ends_at_the_next_one() {
        let mut held: Vec<String> = vec!["f.txt".to_owned()];
        held.extend((2..=200).map(|number| format!("f {number}.txt")));
        let names: Vec<&str> = held.iter().map(String::as_str).collect();

        assert_eq!(beside("f.txt", &names), "f 201.txt");
    }
}
