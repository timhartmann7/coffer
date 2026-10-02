//! What a new entry starts as.
//!
//! A login is what every KeePass client makes, and it is what "+ Entry" makes
//! unasked. The other kinds are what a reader keeps beside their logins - a
//! bank card, the Wi-Fi at home, a passport, the codes that get them back into
//! an account - each with the fields it needs already there, empty, hidden
//! where they hold a secret, so that nobody has to name five fields by hand and
//! remember to hide three of them.
//!
//! Everything a kind writes is in the format's own vocabulary and nothing else
//! (`SPEC.md`, section 12): ordinary string fields, one tag and one of the
//! built-in icons every KeePass client draws. There is no field saying which
//! kind an entry was made as. An entry made as a card is an entry with a few
//! more fields, in KeePassXC as much as here, and it can be changed into
//! anything at all afterwards.

/// One of the kinds of entry Coffer makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Login,
    BankCard,
    Wifi,
    Identity,
    Licence,
    RecoveryCodes,
    SecureNote,
    SshKey,
}

/// One field a kind starts with, beside the five every entry has. It starts
/// empty; what it is called, whether it is hidden, and whether it is written
/// in lines are all it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub name: &'static str,
    /// Kept protected by the database: a secret, shown only on request.
    pub protect: bool,
    /// Written in lines from the start. The format has no such flag - a value
    /// is in lines because it has a break in it - so this is only how the
    /// window draws the field before anything is in it.
    pub lines: bool,
}

/// The key every KeePass client draws for an entry with nothing better to be.
const KEY_ICON: usize = 0;
/// KeePass's "Identity", a card with a face on it.
const IDENTITY_ICON: usize = 9;
/// KeePass's "IRCommunication", which KeePassXC draws as a signal going out.
const SIGNAL_ICON: usize = 12;
/// KeePass's "TerminalEncrypted", a terminal with a key.
const TERMINAL_ICON: usize = 29;
/// KeePass's "Note".
const NOTE_ICON: usize = 44;
/// KeePass's "Package", a box.
const PACKAGE_ICON: usize = 47;
/// KeePass's "PaperLocked", a page with a lock.
const LOCKED_PAPER_ICON: usize = 52;
/// KeePass's "Money".
const MONEY_ICON: usize = 66;

const fn open(name: &'static str) -> Slot {
    Slot {
        name,
        protect: false,
        lines: false,
    }
}

const fn hidden(name: &'static str) -> Slot {
    Slot {
        name,
        protect: true,
        lines: false,
    }
}

const fn hidden_in_lines(name: &'static str) -> Slot {
    Slot {
        name,
        protect: true,
        lines: true,
    }
}

impl Kind {
    /// In the order the window offers them. A login first: it is what
    /// "+ Entry" makes when nobody chose.
    pub const ALL: [Kind; 8] = [
        Kind::Login,
        Kind::BankCard,
        Kind::Wifi,
        Kind::Identity,
        Kind::Licence,
        Kind::RecoveryCodes,
        Kind::SecureNote,
        Kind::SshKey,
    ];

    /// What the window calls it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Login => "Login",
            Kind::BankCard => "Bank card",
            Kind::Wifi => "Wi-Fi",
            Kind::Identity => "Passport / ID",
            Kind::Licence => "Software licence",
            Kind::RecoveryCodes => "Recovery codes",
            Kind::SecureNote => "Secure note",
            Kind::SshKey => "SSH key",
        }
    }

    /// The fields it starts with beside the five every entry has.
    ///
    /// The network's password is a Wi-Fi entry's own Password, and an SSH
    /// key's passphrase is its Password too, with the private key a file on
    /// the entry (`SPEC.md`, section 6). A secure note's secret goes in a
    /// hidden field of its own: the entry's Notes are kept in the open by
    /// every database that does not protect notes, and a secret written there
    /// would cross to the window with the rest of the entry.
    pub fn slots(self) -> &'static [Slot] {
        const BANK_CARD: &[Slot] = &[
            open("Cardholder"),
            hidden("Number"),
            open("Expires"),
            hidden("CVV"),
            hidden("PIN"),
            open("Bank phone"),
        ];
        const WIFI: &[Slot] = &[open("Network name"), open("Security")];
        const IDENTITY: &[Slot] = &[
            open("Full name"),
            hidden("Number"),
            open("Date of birth"),
            open("Issued"),
            open("Expires"),
            open("Issued by"),
        ];
        const LICENCE: &[Slot] = &[
            open("Licensed to"),
            hidden_in_lines("Licence key"),
            open("Order number"),
            open("Purchased"),
        ];
        const RECOVERY_CODES: &[Slot] = &[hidden_in_lines("Recovery codes")];
        const SECURE_NOTE: &[Slot] = &[hidden_in_lines("Secret note")];
        const SSH_KEY: &[Slot] = &[open("Public key")];

        match self {
            Kind::Login => &[],
            Kind::BankCard => BANK_CARD,
            Kind::Wifi => WIFI,
            Kind::Identity => IDENTITY,
            Kind::Licence => LICENCE,
            Kind::RecoveryCodes => RECOVERY_CODES,
            Kind::SecureNote => SECURE_NOTE,
            Kind::SshKey => SSH_KEY,
        }
    }

    /// The tag it is made with, which is how a reader finds every card in the
    /// vault with a search. A login has none: it is the entry every client
    /// makes, and a tag on all of them would tell nobody anything.
    pub(crate) fn tag(self) -> Option<&'static str> {
        match self {
            Kind::Login => None,
            Kind::BankCard => Some("card"),
            Kind::Wifi => Some("wifi"),
            Kind::Identity => Some("id"),
            Kind::Licence => Some("licence"),
            Kind::RecoveryCodes => Some("recovery"),
            Kind::SecureNote => Some("note"),
            Kind::SshKey => Some("ssh"),
        }
    }

    /// The built-in icon it is made with, by KeePass's own numbering, which
    /// KeePassXC and every other client draw from.
    pub(crate) fn icon(self) -> usize {
        match self {
            Kind::Login => KEY_ICON,
            Kind::BankCard => MONEY_ICON,
            Kind::Wifi => SIGNAL_ICON,
            Kind::Identity => IDENTITY_ICON,
            Kind::Licence => PACKAGE_ICON,
            Kind::RecoveryCodes => LOCKED_PAPER_ICON,
            Kind::SecureNote => NOTE_ICON,
            Kind::SshKey => TERMINAL_ICON,
        }
    }
}

/// Names offered for a field of the reader's own, the ones a reader adds most
/// often to an entry made as a login, and whether each is a secret.
pub const SUGGESTED: [Slot; 3] = [
    hidden("PIN"),
    open("Account number"),
    hidden("Security answer"),
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::model::fields::Standard;
    use crate::text;

    /// A kind that named a field twice would make one field where it promised
    /// two, and one that named a standard field would write over the title or
    /// the password of every entry it made.
    #[test]
    fn no_kind_names_a_field_twice_or_takes_a_standard_name() {
        for kind in Kind::ALL {
            let mut seen = HashSet::new();
            for slot in kind.slots() {
                assert!(!slot.name.is_empty(), "{kind:?} names a field nothing");
                assert!(seen.insert(slot.name), "{kind:?} names {} twice", slot.name);
                assert_eq!(
                    Standard::of(slot.name),
                    None,
                    "{kind:?} takes {}",
                    slot.name
                );
                assert!(text::writable(slot.name).is_ok(), "{}", slot.name);
                assert_eq!(slot.name.trim(), slot.name, "{}", slot.name);
            }
        }
    }

    /// A tag the format would split, trim or drop is refused wherever a reader
    /// writes one, and a kind writing one would be the one way past that.
    #[test]
    fn every_tag_is_one_a_file_keeps_as_written() {
        let mut seen = HashSet::new();
        for kind in Kind::ALL {
            if let Some(tag) = kind.tag() {
                assert!(text::tag(tag).is_ok(), "{kind:?} is tagged {tag:?}");
                assert!(seen.insert(tag), "two kinds share {tag:?}");
            }
        }
        assert_eq!(Kind::Login.tag(), None);
    }

    #[test]
    fn a_login_is_offered_first() {
        assert_eq!(Kind::ALL.first(), Some(&Kind::Login));
        assert!(Kind::Login.slots().is_empty());
        let named: HashSet<_> = Kind::ALL.iter().map(|kind| kind.name()).collect();
        assert_eq!(named.len(), Kind::ALL.len(), "two kinds share a name");
    }

    /// KeePass numbers sixty-nine icons, from 0 to 68, and a client handed a
    /// number past them draws nothing, or the key, or fails.
    #[test]
    fn every_icon_is_one_of_the_sixty_nine_built_in() {
        for kind in Kind::ALL {
            assert!(kind.icon() < 69, "{kind:?} is drawn with {}", kind.icon());
        }
        assert_eq!(
            Kind::Login.icon(),
            0,
            "a login is the key every client draws"
        );
    }

    #[test]
    fn no_suggestion_is_a_standard_name_or_a_duplicate() {
        let mut seen = HashSet::new();
        for slot in SUGGESTED {
            assert!(seen.insert(slot.name), "{} is offered twice", slot.name);
            assert_eq!(Standard::of(slot.name), None, "{}", slot.name);
            assert!(text::writable(slot.name).is_ok(), "{}", slot.name);
            assert!(!slot.lines, "{} is offered in lines", slot.name);
        }
    }
}
