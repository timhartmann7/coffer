//! The metadata that crosses the process boundary.
//!
//! One rule shapes every type here: a field's `value` is `null` exactly when
//! the database protects it. A protected value is not in these types, has never
//! been in them, and reaches the screen only through
//! [`reveal`][crate::commands::reveal], one field at a time.
//!
//! None of these derive `Debug`. They carry the user's titles, logins and
//! addresses, which the database does not protect but which still have no
//! business in a panic message.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize, Serializer};
use vault_core::generate;
use vault_core::kind;
use vault_core::model::{self, FieldValue, fields::Standard};
use zeroize::Zeroizing;

use crate::settings;

/// The database Coffer has chosen, whether or not it is open.
#[derive(Serialize)]
pub struct Database {
    /// Absolute, with symbolic links already followed.
    pub path: String,
    /// The file name without its extension: what the window calls the vault.
    pub name: String,
}

/// Where a new vault would go, as the creation screen draws it. Only what it
/// draws: the place itself stays in the session, and nothing the window sends
/// names it.
#[derive(Serialize)]
pub struct Target {
    /// The path the way its owner would write it: under the home folder it
    /// starts with `~`. What the screen shows, and never what anything opens.
    pub shown: String,
    /// What is already at the name. Nothing is ever made over anything, so
    /// the screen says so before anybody types a password rather than after,
    /// and says what it is, because only a vault can be opened instead.
    pub standing: crate::home::Standing,
}

/// A vault sitting in Coffer's own folder, offered on a launch that remembers
/// none. The path does not cross: the session keeps the one it named, and
/// opening it opens that file or answers `gone`.
#[derive(Serialize)]
pub struct Found {
    /// The file name, which is what the offer to open it says.
    pub name: String,
    /// The folder in the home folder it was found in.
    pub folder: &'static str,
    /// Whether nothing is at the name any more, and what was found is the copy
    /// a lock left of the vault beside it. The offer must not say it found a
    /// file that is not there.
    pub copy: bool,
}

/// A value revealed out of the vault, on its way to the one screen that asked
/// for it.
///
/// The buffer is wiped when this is dropped. Serde copies it into the message
/// Tauri sends and that copy is out of reach - the same residue `docs/ipc.md`
/// records for the password travelling the other way - but the copy Coffer owns
/// is not left behind.
pub struct Revealed(Zeroizing<String>);

impl Revealed {
    pub fn new(value: &str) -> Revealed {
        Revealed(Zeroizing::new(value.to_owned()))
    }
}

impl Serialize for Revealed {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// The part of a value a reader selected on the screen, from one position to
/// another, counted the way the text node showing it counts: in UTF-16 code
/// units. Two numbers and nothing of the value; whether they name a part of it
/// at all is [`vault_core::SecretValue::part`]'s to say.
///
/// It goes back to the window too, in a Copy chosen from the menu over a
/// revealed value, which the window then sends here again.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Span {
    pub from: usize,
    pub to: usize,
}

/// The entry an id names, or nothing at all. An id that does not parse is not
/// an entry Coffer has, which is the same answer as an id that parses and names
/// nothing.
pub fn entry_id(text: &str) -> Result<model::EntryId, crate::error::Failure> {
    uuid::Uuid::parse_str(text)
        .map(model::EntryId::from_uuid)
        .map_err(|_| crate::error::Failure::no_such_entry())
}

/// The entries a batch names, or nothing at all: one id that does not parse
/// refuses the whole batch before anything is asked of the vault, as one that
/// parses and names nothing refuses it there.
pub fn entry_ids(texts: &[String]) -> Result<Vec<model::EntryId>, crate::error::Failure> {
    texts.iter().map(|text| entry_id(text)).collect()
}

/// The folder an id names, on the same terms.
pub fn group_id(text: &str) -> Result<model::GroupId, crate::error::Failure> {
    uuid::Uuid::parse_str(text)
        .map(model::GroupId::from_uuid)
        .map_err(|_| crate::error::Failure::no_such_group())
}

impl Snapshot {
    pub fn of(taken: &vault_core::storage::snapshot::Taken) -> Snapshot {
        Snapshot {
            name: file_name(&taken.path),
            index: taken.index,
            taken: taken.taken.and_then(moment),
        }
    }
}

impl Found {
    pub fn of(path: &std::path::Path) -> Found {
        Found {
            name: file_name(path),
            folder: crate::home::FOLDER,
            copy: crate::home::standing(path) == crate::home::Standing::Copy,
        }
    }
}

impl Rescued {
    pub fn of(kept: &vault_core::storage::unsaved::Kept) -> Rescued {
        Rescued {
            name: file_name(&kept.path),
            written: kept.written.and_then(moment),
        }
    }
}

/// A file as the disk has it now, for the sentences that say what a lock left:
/// what survived a lock that could write nothing, and whether a copy has a
/// vault to go over or an empty name to go back into.
#[derive(Serialize)]
pub struct OnDisk {
    /// Whether anything at all is at the name.
    pub there: bool,
    /// When it was last written, when it is there and the filesystem keeps the
    /// time.
    pub written: Option<String>,
}

impl OnDisk {
    pub fn of(found: vault_core::storage::OnDisk) -> OnDisk {
        match found {
            vault_core::storage::OnDisk::Gone => OnDisk {
                there: false,
                written: None,
            },
            vault_core::storage::OnDisk::Written(written) => OnDisk {
                there: true,
                written: written.and_then(moment),
            },
        }
    }
}

/// The vault a chosen database was copied from, when it is the copy a lock
/// left: what the screen says about the copy and about what making it the vault
/// would do. Names and times only; the paths stay in Rust.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyOf {
    /// The vault's file name, which is the file the copy would go over.
    pub vault: String,
    /// When the copy was last written, as the filesystem has it.
    pub saved: Option<String>,
    /// What the vault's file is called once the copy has gone over it: the
    /// newest snapshot.
    pub kept_as: String,
    /// The vault's file as it stands, which the copy would go over or, when
    /// it has gone, take the name of.
    pub vault_file: OnDisk,
}

impl CopyOf {
    pub fn of(
        vault: &std::path::Path,
        copy: vault_core::storage::OnDisk,
        vault_file: vault_core::storage::OnDisk,
    ) -> CopyOf {
        CopyOf {
            vault: file_name(vault),
            saved: match copy {
                vault_core::storage::OnDisk::Written(written) => written.and_then(moment),
                vault_core::storage::OnDisk::Gone => None,
            },
            kept_as: vault_core::storage::snapshot::slot(vault, 1)
                .map(|slot| file_name(&slot))
                .unwrap_or_default(),
            vault_file: OnDisk::of(vault_file),
        }
    }
}

/// The name a file goes by on the screen.
///
/// The file name and not the stem: an offer to open something has to say what
/// the reader would see in the Finder, and the stem of `vault.kdbx.unsaved.kdbx`
/// reads as a vault called `vault.kdbx.unsaved`.
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl Database {
    pub fn of(path: &std::path::Path) -> Database {
        Database {
            path: path.to_string_lossy().into_owned(),
            name: path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub database: Option<Database>,
    /// A vault in Coffer's own folder, when nothing is remembered to open.
    /// Looked for only then: a launch that knows its vault has nothing to find.
    pub found: Option<Found>,
    /// The key file the next unlock will use, when the reader has chosen one.
    /// Reported rather than remembered by the window, because a lock destroys
    /// the window and the vault it is about is still the same one.
    pub key_file: Option<Database>,
    pub unlocked: bool,
    /// How many entries the vault holds, the recycle bin's included.
    pub entries: usize,
    /// Whether this database can be written back at all. A snapshot, a format
    /// Coffer will not write and a place that refuses a write are all read only.
    pub read_only: bool,
    /// The unsaved copy sitting beside the database Coffer will open next, when
    /// a lock had to write one. Read off the disk rather than remembered, so a
    /// copy left by a run that has since quit is still offered.
    pub rescue: Option<Rescued>,
    /// Whether the last lock in this run found work the file had not got and
    /// could not put it anywhere at all. There is no file to point at, which is
    /// why this is a flag and not a path.
    pub lost: bool,
    /// The chosen database's own file as it stands. Read off the disk each
    /// time, because it is what a lock left and what the reader may since have
    /// moved.
    pub file: Option<OnDisk>,
    /// The vault the chosen database was copied from, when it is the copy a
    /// lock left.
    pub copy: Option<CopyOf>,
    /// Whether the last lock found text the reader was still typing and saved
    /// it into the vault with everything else. Which entry is not said: after a
    /// lock nothing of the vault is left to say it with.
    pub typed: bool,
    /// Whether some of what it saved was a new value typed in a Change field
    /// and kept in a field of its own beside the value it was for, which the
    /// field still holds. Said apart, because "saved" alone reads as the new
    /// value being the field's now. Which field is not said, for the same
    /// reason.
    pub typed_beside: bool,
    /// Why the vault that was open is not open any more, when it is worth
    /// saying. A lock the reader asked for has nothing to explain.
    pub locked_by: Option<&'static str>,
    /// How many seconds the open vault has before it locks itself.
    pub locks_in: Option<u64>,
}

/// What the reader chose, and what they may choose instead.
///
/// The lists come with the values. A screen that held its own copy of what may
/// be chosen would be a second place the answer lives, and the two would part
/// company the first time one of them changed.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub idle_seconds: u64,
    pub clipboard_seconds: u64,
    pub lock_on_sleep: bool,
    pub lock_on_screen_lock: bool,
    pub theme: settings::Theme,
    #[serde(skip_deserializing)]
    pub idle_choices: Vec<u64>,
    #[serde(skip_deserializing)]
    pub clipboard_choices: Vec<u64>,
    #[serde(skip_deserializing)]
    pub theme_choices: Vec<settings::Theme>,
}

impl Settings {
    pub fn of(held: settings::Settings) -> Settings {
        Settings {
            idle_seconds: held.idle_seconds,
            clipboard_seconds: held.clipboard_seconds,
            lock_on_sleep: held.lock_on_sleep,
            lock_on_screen_lock: held.lock_on_screen_lock,
            theme: held.theme,
            idle_choices: settings::IDLE_CHOICES.to_vec(),
            clipboard_choices: settings::CLIPBOARD_CHOICES.to_vec(),
            theme_choices: settings::THEME_CHOICES.to_vec(),
        }
    }

    /// What the window asked for. The lists it was sent do not come back: they
    /// are this side's to decide, and a message naming others would be the
    /// window choosing what it may choose. A look it invented lands on the
    /// default the same way one out of a settings file does.
    pub fn wanted(&self) -> settings::Settings {
        settings::Settings {
            idle_seconds: self.idle_seconds,
            clipboard_seconds: self.clipboard_seconds,
            lock_on_sleep: self.lock_on_sleep,
            lock_on_screen_lock: self.lock_on_screen_lock,
            theme: self.theme,
        }
    }
}

/// One of the kinds of character the generator draws from, by the name the
/// window gives it.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Alphabet {
    Lower,
    Upper,
    Digits,
    Symbols,
}

impl Alphabet {
    fn of(alphabet: generate::Alphabet) -> Alphabet {
        match alphabet {
            generate::Alphabet::Lower => Alphabet::Lower,
            generate::Alphabet::Upper => Alphabet::Upper,
            generate::Alphabet::Digits => Alphabet::Digits,
            generate::Alphabet::Symbols => Alphabet::Symbols,
        }
    }

    fn alphabet(self) -> generate::Alphabet {
        match self {
            Alphabet::Lower => generate::Alphabet::Lower,
            Alphabet::Upper => generate::Alphabet::Upper,
            Alphabet::Digits => generate::Alphabet::Digits,
            Alphabet::Symbols => generate::Alphabet::Symbols,
        }
    }
}

/// What the generator is asked to make: the window's slider and switches.
///
/// The same shape comes back with every password made and goes into the file
/// the generator remembers it in, so there is one way to write a recipe down.
/// A field missing from the file - one an older Coffer did not write - takes
/// the default rather than costing the rest.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Recipe {
    pub length: usize,
    pub alphabets: Vec<Alphabet>,
    /// Whether characters that look alike may appear. The window's switch
    /// says the opposite - "Avoid look-alikes" - and is on when this is off.
    pub similar: bool,
    pub avoid: String,
}

impl Default for Recipe {
    fn default() -> Recipe {
        Recipe::of(&generate::Recipe::default())
    }
}

impl Recipe {
    pub fn of(recipe: &generate::Recipe) -> Recipe {
        Recipe {
            length: recipe.length,
            alphabets: recipe.alphabets.iter().copied().map(Alphabet::of).collect(),
            similar: recipe.similar,
            avoid: recipe.avoid.clone(),
        }
    }

    /// What the engine makes of it. Not settled: the caller settles it, once.
    pub fn recipe(&self) -> generate::Recipe {
        generate::Recipe {
            length: self.length,
            alphabets: self.alphabets.iter().map(|kind| kind.alphabet()).collect(),
            similar: self.similar,
            avoid: self.avoid.clone(),
        }
    }
}

/// Which generator is asking: the password's, or one under a field of the
/// reader's own.
///
/// Each remembers its own recipe. One recipe for both meant a PIN made for a
/// card's field was what the password's generator opened with next, and one
/// press of "Put it in the field" made four digits the account's password.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Purpose {
    Password,
    Field,
}

/// The generator as the window draws it for a recipe.
///
/// The lengths the slider runs between, and the characters two of its
/// switches stand for, come from the engine that draws them: a slider holding
/// its own copy of the range was a second answer to what the generator makes,
/// and the two had to be changed together by hand.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Generator {
    pub recipe: Recipe,
    pub shortest: usize,
    pub longest: usize,
    /// Digits and nothing else, which is a PIN, and the slider says so.
    pub pin: bool,
    /// Every character "Symbols" draws from, in order.
    pub symbols: &'static str,
    /// The characters "Avoid look-alikes" leaves out, or nothing for a PIN,
    /// which it does not apply to and the window does not offer it for.
    pub look_alikes: &'static str,
}

impl Generator {
    pub fn of(recipe: &generate::Recipe) -> Generator {
        let lengths = recipe.lengths();
        Generator {
            recipe: Recipe::of(recipe),
            shortest: *lengths.start(),
            longest: *lengths.end(),
            pin: recipe.is_pin(),
            symbols: generate::Alphabet::Symbols.characters(),
            look_alikes: recipe.look_alikes(),
        }
    }
}

/// A password made, the way a revealed value is sent, with the kinds of
/// character it was asked to draw from that it happens to lack and the
/// generator as it now stands: the length the engine settled on is the one
/// the slider moves to.
#[derive(Serialize)]
pub struct Generated {
    pub value: Revealed,
    pub missing: Vec<Alphabet>,
    pub generator: Generator,
}

impl Generated {
    pub fn of(made: &str, recipe: &generate::Recipe) -> Generated {
        Generated {
            value: Revealed::new(made),
            missing: recipe.lacking(made).into_iter().map(Alphabet::of).collect(),
            generator: Generator::of(recipe),
        }
    }
}

/// What key derivation a new vault will ask for, and what that measured.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Calibration {
    pub iterations: u64,
    /// What the measurement took, in seconds. The one number the creation
    /// screen shows, and it only exists once the measuring is over.
    pub seconds: f64,
}

impl Calibration {
    pub fn of(found: vault_core::kdf::Calibration) -> Calibration {
        Calibration {
            iterations: found.work.iterations,
            seconds: found.took.as_secs_f64(),
        }
    }
}

/// A previous version of an entry, as the versions block lists them.
#[derive(Serialize)]
pub struct Version {
    /// Where it sits in the entry's history, which is how it is addressed.
    /// Several versions can share a modification time, so nothing else
    /// identifies one.
    pub index: usize,
    pub modified: Option<String>,
}

impl Version {
    pub fn of(version: &model::Version) -> Version {
        Version {
            index: version.index,
            modified: stamp(version.modified),
        }
    }
}

/// An entry's previous versions, oldest first, with the revision of the vault
/// they were listed at. Every position taken from the list goes back to Rust
/// with that revision; see [`crate::session::Session::at`].
#[derive(Serialize)]
pub struct Versions {
    pub revision: u64,
    pub versions: Vec<Version>,
}

impl Versions {
    pub fn of(revision: u64, versions: &[model::Version]) -> Versions {
        Versions {
            revision,
            versions: versions.iter().map(Version::of).collect(),
        }
    }
}

/// What a command that changed the shape of the vault hands back: the tree as
/// it is now, and the entry the change was about.
#[derive(Serialize)]
pub struct Made {
    pub tree: Group,
    pub entry: String,
}

/// One of the kinds of entry Coffer makes, by the word the window sends back.
///
/// The window holds no list of them: it draws what [`Kinds`] offered and sends
/// back the word that came with each, so a kind it made up is a word serde
/// refuses rather than one read as the nearest kind there is.
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
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

impl Kind {
    fn of(kind: kind::Kind) -> Kind {
        match kind {
            kind::Kind::Login => Kind::Login,
            kind::Kind::BankCard => Kind::BankCard,
            kind::Kind::Wifi => Kind::Wifi,
            kind::Kind::Identity => Kind::Identity,
            kind::Kind::Licence => Kind::Licence,
            kind::Kind::RecoveryCodes => Kind::RecoveryCodes,
            kind::Kind::SecureNote => Kind::SecureNote,
            kind::Kind::SshKey => Kind::SshKey,
        }
    }

    /// The kind the engine makes for this word.
    pub fn wanted(self) -> kind::Kind {
        match self {
            Kind::Login => kind::Kind::Login,
            Kind::BankCard => kind::Kind::BankCard,
            Kind::Wifi => kind::Kind::Wifi,
            Kind::Identity => kind::Kind::Identity,
            Kind::Licence => kind::Kind::Licence,
            Kind::RecoveryCodes => kind::Kind::RecoveryCodes,
            Kind::SecureNote => kind::Kind::SecureNote,
            Kind::SshKey => kind::Kind::SshKey,
        }
    }
}

/// One kind as "+ Entry" offers it: the word to send back, what to call it,
/// and the fields it writes in lines, which the window draws as text areas
/// before anything is in them. The rest of what it writes arrives with the
/// entry it makes.
#[derive(Serialize)]
pub struct Offer {
    pub kind: Kind,
    pub name: &'static str,
    pub lined: Vec<&'static str>,
}

/// A name offered for a field of the reader's own, and whether a field made
/// under it is hidden.
#[derive(Serialize)]
pub struct Suggestion {
    pub name: &'static str,
    pub protect: bool,
}

/// What a new entry can start as, a login first, and the names offered for a
/// field of the reader's own. The same for every vault and every launch, and
/// Rust's, so the window holds no copy of either list.
#[derive(Serialize)]
pub struct Kinds {
    pub offered: Vec<Offer>,
    pub suggested: Vec<Suggestion>,
}

impl Kinds {
    pub fn of() -> Kinds {
        Kinds {
            offered: kind::Kind::ALL
                .into_iter()
                .map(|offered| Offer {
                    kind: Kind::of(offered),
                    name: offered.name(),
                    lined: offered
                        .slots()
                        .iter()
                        .filter(|slot| slot.lines)
                        .map(|slot| slot.name)
                        .collect(),
                })
                .collect(),
            suggested: kind::SUGGESTED
                .into_iter()
                .map(|slot| Suggestion {
                    name: slot.name,
                    protect: slot.protect,
                })
                .collect(),
        }
    }
}

/// What a move between folders hands back: the tree as it is now, and every
/// entry that changed folder with the folder it left, which is what taking the
/// move back sends again.
#[derive(Serialize)]
pub struct Moved {
    pub tree: Group,
    pub moved: Vec<Move>,
}

/// One entry a move took out of a folder, and that folder.
///
/// It comes back from the window to take the move back, where it is only a
/// claim: [`vault_core::Vault::move_entries_back`] takes nothing back unless
/// the file says the same.
#[derive(Serialize, Deserialize)]
pub struct Move {
    pub entry: String,
    pub from: String,
}

impl Move {
    pub fn of(moved: model::Move) -> Move {
        Move {
            entry: moved.entry.to_string(),
            from: moved.from.to_string(),
        }
    }

    /// The move this names, or nothing when either id does not parse, which
    /// is answered the way an id naming nothing is.
    pub fn parsed(&self) -> Result<model::Move, crate::error::Failure> {
        Ok(model::Move {
            entry: entry_id(&self.entry)?,
            from: group_id(&self.from)?,
        })
    }
}

/// One entry a deletion names, with what the window showed deleting it would
/// do. The answer the reader agreed to travels with each entry, because it is
/// for each entry that Rust can find it changed.
///
/// It goes the other way in a deletion chosen from a menu under the pointer,
/// carrying what that menu said each deletion does, which the window sends
/// back as the one the reader was shown.
#[derive(Serialize, Deserialize)]
pub struct Deleting {
    pub entry: String,
    pub deletion: Deletion,
}

impl Deleting {
    /// The entry and the deletion shown, or nothing when the id does not
    /// parse, which is answered the way an id naming nothing is.
    pub fn shown(&self) -> Result<(model::EntryId, model::Deletion), crate::error::Failure> {
        Ok((entry_id(&self.entry)?, self.deletion.shown()))
    }
}

/// What putting a tag on entries hands back: the tree, and the entries the tag
/// went on, which are those that did not have it. Only those are what taking
/// it off again sends.
#[derive(Serialize)]
pub struct Tagged {
    pub tree: Group,
    pub changed: Vec<String>,
}

impl Tagged {
    /// The answer for a tag that went on `changed`, with the vault as it now
    /// stands.
    pub fn of(tree: &model::Project, changed: &[model::EntryId]) -> Tagged {
        Tagged {
            tree: Group::of(tree),
            changed: changed.iter().map(ToString::to_string).collect(),
        }
    }
}

/// What the file on disk holds, for the dialog that asks which version to keep.
#[derive(Serialize)]
pub struct Rival {
    /// When the file was last written, as the filesystem has it.
    pub modified: Option<String>,
    /// How many entries it holds, or nothing when it will not open with the
    /// password this vault was opened with.
    pub entries: Option<usize>,
}

/// The unsaved copy a lock left beside the vault.
#[derive(Serialize)]
pub struct Rescued {
    /// The file name, which is what the offer to open it says.
    pub name: String,
    /// When it was written, as the filesystem has it.
    pub written: Option<String>,
}

/// A snapshot Coffer took before one of its own saves.
#[derive(Serialize)]
pub struct Snapshot {
    /// The file name, which is what the offer to open it says.
    pub name: String,
    /// Which slot it is in, counting from 1 for the most recent. This is what
    /// the screen sends back to open it.
    pub index: u32,
    pub taken: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    pub is_recycle_bin: bool,
    /// The group the vault keeps its entry templates in: what "+ Entry"
    /// offers to make an entry from is what it holds itself.
    pub is_templates: bool,
    /// When the group is in the recycle bin, when it went in and where it goes
    /// back to. `null` for the bin itself and for everything outside it.
    pub binned: Option<Binned>,
    /// What deleting the group would do.
    pub deletion: Deletion,
    pub sections: Vec<Group>,
    pub entries: Vec<EntryRow>,
}

impl Group {
    pub fn of(project: &model::Project) -> Group {
        Group {
            id: project.id.to_string(),
            name: project.name.clone(),
            is_recycle_bin: project.is_recycle_bin,
            is_templates: project.is_templates,
            binned: project.binned.map(Binned::of),
            deletion: Deletion::of(project.deletion),
            sections: project.sections.iter().map(Group::of).collect(),
            entries: project.entries.iter().map(EntryRow::of).collect(),
        }
    }
}

/// What deleting something does, said before the reader asks for it.
///
/// It travels with the tree and with the entry rather than in `status`,
/// because both are read again after every change and after a reload, and a
/// file another client rewrote can have stopped keeping a bin: a flag read
/// once at unlock would go on promising a bin to a deletion that erases.
///
/// It comes back with a deletion, as the one the reader was shown, and Rust
/// refuses a deletion that would now do something else.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Deletion {
    /// It moves to the recycle bin, and can be put back.
    Bin,
    /// It goes out of the file for good.
    Forever,
}

impl Deletion {
    pub fn of(deletion: model::Deletion) -> Deletion {
        match deletion {
            model::Deletion::Bin => Deletion::Bin,
            model::Deletion::Forever => Deletion::Forever,
        }
    }

    pub fn shown(self) -> model::Deletion {
        match self {
            Deletion::Bin => model::Deletion::Bin,
            Deletion::Forever => model::Deletion::Forever,
        }
    }
}

/// What is known about something in the recycle bin.
#[derive(Serialize)]
pub struct Binned {
    /// When it went in: its own move, or that of the folder it went in with.
    pub since: Option<String>,
    /// The deleted folder it went in with, or `null` for something deleted on
    /// its own.
    pub within: Option<String>,
    /// Where putting it back takes it: the folder it was deleted from, or the
    /// one the folder it went in with was deleted from. `null` when that is not
    /// known, has gone, or is in the bin too, and putting it back takes it to
    /// the top of the vault.
    pub from: Option<String>,
}

impl Binned {
    fn of(binned: model::Binned) -> Binned {
        Binned {
            since: stamp(binned.since),
            within: binned.within.map(|group| group.to_string()),
            from: binned.from.map(|group| group.to_string()),
        }
    }
}

/// What a list row may know. Notes and custom fields are not here: they are the
/// user's data and have no business travelling in bulk to a screen that only
/// wants to draw a row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryRow {
    pub id: String,
    pub group: String,
    pub title: Option<String>,
    pub username: Option<String>,
    pub url: Option<String>,
    pub tags: Vec<String>,
    pub modified: Option<String>,
    pub has_password: bool,
    pub attachments: usize,
    /// When the entry is in the recycle bin, when it went in and where from.
    pub binned: Option<Binned>,
    /// What deleting the entry would do, so that the window can say before a
    /// press which of several chosen entries go to the bin and which for good.
    pub deletion: Deletion,
}

impl EntryRow {
    pub fn of(summary: &model::EntrySummary) -> EntryRow {
        EntryRow {
            id: summary.id.to_string(),
            group: summary.group.to_string(),
            title: shown(&summary.title),
            username: shown(&summary.username),
            url: shown(&summary.url),
            tags: summary.tags.clone(),
            modified: stamp(summary.times.modified),
            has_password: summary.has_password,
            attachments: summary.attachments,
            binned: summary.binned.map(Binned::of),
            deletion: Deletion::of(summary.deletion),
        }
    }
}

/// Which of an entry's fields this is. It says nothing about the value, so
/// unlike everything else here it is safe to print.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    Title,
    Username,
    Password,
    Url,
    Notes,
    Custom,
}

impl FieldKind {
    /// Which names are standard is decided in `vault-core`, which is the only
    /// place that knows what KeePass calls a field, and the one a lock asks
    /// before it writes a draft into a field the entry does not hold yet, and
    /// the one a menu under the pointer asks of a field the entry lacks.
    pub fn of(name: &str) -> FieldKind {
        match Standard::of(name) {
            Some(Standard::Title) => FieldKind::Title,
            Some(Standard::Username) => FieldKind::Username,
            Some(Standard::Password) => FieldKind::Password,
            Some(Standard::Url) => FieldKind::Url,
            Some(Standard::Notes) => FieldKind::Notes,
            None => FieldKind::Custom,
        }
    }
}

#[derive(Serialize)]
pub struct Field {
    /// The name the file holds, and the name a reveal asks for.
    pub name: String,
    pub kind: FieldKind,
    /// Whether the database keeps this value protected: the field's lock on
    /// the screen. An edit sends it back as `protect`, which decides only how a
    /// field the entry does not have yet is made; a field it has keeps its own
    /// protection, and only `set_protection` changes it.
    pub protected: bool,
    /// The value, or `null` when it does not cross: the database protects it,
    /// or it is the password. A file can hold a password the database does not
    /// protect, and it is still a password; `empty` says whether there is one,
    /// and a reveal is the only way to see it.
    pub value: Option<String>,
    pub empty: bool,
    /// Whether Coffer would hand this value to the system if asked to open it.
    /// Only ever true for a URL field: the rule is in
    /// [`vault_core::url`], and this is the screen's copy of the answer.
    pub openable: bool,
    /// Whether the value has a line break in it. One fact about a value that
    /// does not cross, the way `empty` is, and nothing of what it says: ten
    /// recovery codes are replaced in a field written in lines, where Return
    /// starts the next code rather than saving the first over all ten.
    pub lines: bool,
}

impl Field {
    fn of(field: &model::Field) -> Field {
        let kind = FieldKind::of(&field.name);
        let value = match kind {
            FieldKind::Password => None,
            _ => shown(&field.value),
        };
        Field {
            name: field.name.clone(),
            kind,
            protected: matches!(field.value, FieldValue::Protected { .. }),
            openable: kind == FieldKind::Url
                && value
                    .as_deref()
                    .is_some_and(|text| vault_core::url::openable(text).is_some()),
            value,
            empty: field.is_empty(),
            lines: field.in_lines(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    /// As the file holds it. It may contain anything at all, `/` and `..`
    /// included, so nothing may build a path out of it.
    pub name: String,
    pub size: usize,
    /// What the save panel would call it. The rule lives in `vault-core`; this
    /// is the screen's copy of the answer, so that the name a reader sees on
    /// the export button is the name they will get.
    pub file_name: String,
}

#[derive(Serialize)]
pub struct Entry {
    pub id: String,
    pub group: String,
    /// How many previous versions the entry keeps.
    pub versions: usize,
    /// Every field the entry has, standard and custom alike, ordered by name.
    pub fields: Vec<Field>,
    pub attachments: Vec<Attachment>,
    pub tags: Vec<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    /// When the entry is in the recycle bin, when it went in and where it goes
    /// back to. The screen shows such an entry read only.
    pub binned: Option<Binned>,
    /// What deleting the entry would do.
    pub deletion: Deletion,
}

impl Entry {
    pub fn of(entry: &model::Entry) -> Entry {
        Entry {
            id: entry.id.to_string(),
            group: entry.group.to_string(),
            versions: entry.versions,
            fields: entry.fields.iter().map(Field::of).collect(),
            attachments: entry
                .attachments
                .iter()
                .map(|attachment| Attachment {
                    name: attachment.name.clone(),
                    size: attachment.size,
                    file_name: attachment.file_name(),
                })
                .collect(),
            tags: entry.tags.clone(),
            created: stamp(entry.times.created),
            modified: stamp(entry.times.modified),
            binned: entry.binned.map(Binned::of),
            deletion: Deletion::of(entry.deletion),
        }
    }
}

/// What came of a file the reader chose for an entry.
///
/// Not a failure when the name is taken: nothing went wrong, and nothing has
/// happened yet. The file is held in Rust and the window has a question to
/// ask, which is a different thing from a sentence to show.
#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum Attached {
    /// It is on the entry, and this is the entry now.
    Added { entry: Entry },
    /// The entry already gives that name to a file, and nothing changed.
    Taken { clash: Clash },
}

/// A name the entry already gives a file, and what the reader is asked to
/// decide about it. Sizes and names only: neither file's bytes cross.
#[derive(Serialize)]
pub struct Clash {
    /// The name both files go by, exactly as the entry holds it.
    pub name: String,
    /// How large the file already there is.
    pub size: usize,
    /// How large the file just chosen is. Two files by one name are told apart
    /// by size before anything else, and a reader who picked the same file
    /// twice sees two numbers that agree.
    pub chosen: usize,
    /// The name the file just chosen goes under if both are kept.
    pub free: String,
}

impl Clash {
    pub fn of(name: String, chosen: usize, clash: vault_core::Clash) -> Clash {
        Clash {
            name,
            size: clash.size,
            chosen,
            free: clash.free,
        }
    }
}

/// What the page is told the reader chose outside it: an item of the menu bar,
/// or the window's own close button.
///
/// Small on purpose. A message under 8 KiB is evaluated straight into the page
/// (`ipc/channel.rs` in tauri 2.11.5); a larger one is parked in Rust and
/// fetched back over `ipc:`, which is a second request for every choice.
#[derive(Serialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum Action {
    /// An item of Coffer's own in the menu bar.
    Command { command: crate::menu::Command },
    /// The close button, or Close Window. The page sends what is being typed
    /// and then asks for `close_window` itself, the way the Lock button asks
    /// for a lock.
    Closing,
    /// An item of the menu the window asked for under the pointer
    /// (`context_menu`), with the number the window gave that menu. A choice
    /// for a selection carries every id in it, which can be over the 8 KiB:
    /// it is fetched then, the one choice that pays for a second request.
    Context { serial: u64, chosen: Chosen },
}

/// What a right-click was on, as the window names it: ids, names, whether a
/// value is on the screen, and for a revealed value the part of it selected.
/// Nothing of any value, and nothing Rust takes on trust: every id is read
/// again in the vault, and a place is only where an item says a move goes.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Subject {
    /// A row of the list, and where the window would let it be moved.
    Entry { entry: String, places: Vec<Place> },
    /// The rows chosen in the list, when the right-click was on one of them.
    Entries {
        entries: Vec<String>,
        places: Vec<Place>,
    },
    /// A folder in the tree or in the recycle bin.
    Folder { group: String, places: Vec<Place> },
    /// The recycle bin's own row.
    Bin,
    /// A field's row, and whether its value is on the screen.
    Field {
        entry: String,
        field: String,
        shown: bool,
    },
    /// A file on an entry, by the name the entry gives it.
    File { entry: String, name: String },
    /// A revealed value, and the part of it the reader selected, or nothing
    /// for all of it.
    Value {
        entry: String,
        field: String,
        range: Option<Span>,
    },
}

/// A place a menu's Move to offers, as the window's folder list has it: in
/// the tree's order, `depth` folders down from the top, its name already set
/// apart and cut the way a sentence sets one apart, and whether a move there
/// would be taken and change something.
///
/// The window's own rule (`placesFor` in `places.ts`), sent rather than worked
/// out again here: a menu that offered another list of places than the
/// window's folder list would be a second answer to one question. Flat,
/// because a hundred folders nested inside each other would be more nesting
/// than serde_json reads. A place the window was wrong about is refused by the
/// move itself.
#[derive(Deserialize)]
pub struct Place {
    pub id: String,
    pub name: String,
    pub open: bool,
    pub depth: usize,
}

/// Where the pointer was when the reader right-clicked, in the window's points
/// from its top-left corner.
#[derive(Deserialize, Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    /// Where AppKit is asked to draw the menu. A coordinate that is not a
    /// place in the window - less than nothing, or not a number at all - is
    /// its edge.
    pub fn logical(self) -> tauri::LogicalPosition<f64> {
        let inside = |at: f64| if at.is_finite() && at > 0.0 { at } else { 0.0 };
        tauri::LogicalPosition::new(inside(self.x), inside(self.y))
    }
}

/// An item chosen from a menu under the pointer, with the ids it is about. The
/// window runs the function the item's button runs, on those ids, and nothing
/// when they are not what it shows any more.
#[derive(Serialize)]
#[serde(tag = "item", rename_all = "camelCase")]
pub enum Chosen {
    /// A field's value, whole, through Rust.
    CopyField {
        entry: String,
        field: String,
    },
    /// A revealed value, or the part of it selected, through Rust.
    CopyValue {
        entry: String,
        field: String,
        range: Option<Span>,
    },
    ShowField {
        entry: String,
        field: String,
    },
    HideField {
        entry: String,
        field: String,
    },
    ChangeField {
        entry: String,
        field: String,
    },
    MakeOne {
        entry: String,
        field: String,
    },
    RemoveField {
        entry: String,
        field: String,
    },
    OpenAddress {
        entry: String,
    },
    Duplicate {
        entry: String,
    },
    MoveEntries {
        entries: Vec<String>,
        into: String,
    },
    /// Each entry with what the menu said deleting it does.
    DeleteEntries {
        entries: Vec<Deleting>,
    },
    PutBackEntries {
        entries: Vec<String>,
    },
    NewEntryIn {
        group: String,
    },
    NewFolderIn {
        group: String,
    },
    RenameFolder {
        group: String,
    },
    MoveFolder {
        group: String,
        into: String,
    },
    DeleteFolder {
        group: String,
        deletion: Deletion,
    },
    PutBackFolder {
        group: String,
    },
    EmptyBin,
    SaveFile {
        entry: String,
        name: String,
    },
    RemoveFile {
        entry: String,
        name: String,
    },
}

/// The value when the database does not protect it, and nothing at all when it
/// does.
fn shown(value: &FieldValue) -> Option<String> {
    match value {
        FieldValue::Open(text) => Some(text.clone()),
        FieldValue::Protected { .. } => None,
    }
}

/// KDBX keeps its times in UTC, and the screen turns them into the reader's own
/// clock. Sending the zone with the value is what makes that possible.
fn stamp(time: Option<NaiveDateTime>) -> Option<String> {
    time.map(|time| time.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// A filesystem timestamp, when it is one a calendar can hold.
///
/// It comes from whatever wrote the file, which may be another machine, another
/// filesystem or nothing sensible at all. `DateTime::from(SystemTime)` unwraps
/// its own range check, and a snapshot beside the database is not worth a panic.
pub(crate) fn moment(time: std::time::SystemTime) -> Option<String> {
    let since = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    let moment = chrono::DateTime::from_timestamp(
        i64::try_from(since.as_secs()).ok()?,
        since.subsec_nanos(),
    )?;
    stamp(Some(moment.naive_utc()))
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use vault_core::model::{
        Attachment, EntryId, EntrySummary, Field, GroupId, Timestamps, fields,
    };

    use super::*;

    fn open(name: &str, value: &str) -> Field {
        Field {
            name: name.to_owned(),
            value: FieldValue::Open(value.to_owned()),
        }
    }

    fn protected(name: &str, empty: bool) -> Field {
        Field {
            name: name.to_owned(),
            value: FieldValue::Protected {
                empty,
                lines: false,
            },
        }
    }

    fn entry_of(fields: Vec<Field>) -> model::Entry {
        model::Entry {
            id: EntryId::from_uuid(uuid::Uuid::nil()),
            group: GroupId::from_uuid(uuid::Uuid::nil()),
            fields,
            attachments: Vec::new(),
            tags: Vec::new(),
            times: Timestamps::default(),
            versions: 0,
            binned: None,
            deletion: model::Deletion::Bin,
        }
    }

    fn json(value: &impl Serialize) -> String {
        serde_json::to_string(value).expect("the payload serialises")
    }

    /// The kinds a mask names, one bit each in the order the switches stand.
    fn kinds(mask: u8) -> Vec<generate::Alphabet> {
        generate::Alphabet::ALL
            .into_iter()
            .enumerate()
            .filter(|(bit, _)| mask & (1 << bit) != 0)
            .map(|(_, kind)| kind)
            .collect()
    }

    fn making(alphabets: Vec<generate::Alphabet>, similar: bool, avoid: &str) -> generate::Recipe {
        generate::Recipe {
            length: 12,
            alphabets,
            similar,
            avoid: avoid.to_owned(),
        }
    }

    /// The page reads a choice by its tag and nothing else, so the shape is
    /// pinned exactly. Every one stays on the path that evaluates it into the
    /// page rather than parking it for a second request.
    #[test]
    fn an_action_is_a_small_message_read_by_its_tag() {
        assert_eq!(
            json(&Action::Command {
                command: crate::menu::Command::NewEntry
            }),
            r#"{"action":"command","command":"newEntry"}"#
        );
        assert_eq!(json(&Action::Closing), r#"{"action":"closing"}"#);

        for command in crate::menu::Command::ALL {
            let sent = json(&Action::Command { command });
            assert!(sent.len() < 8192, "{command:?} is fetched rather than told");
            assert!(
                sent.starts_with(r#"{"action":"command","command":""#),
                "{sent}"
            );
        }
    }

    /// An item chosen from a menu under the pointer reaches the page under
    /// the menu's number, read by its own tag, with the ids it is about and
    /// the part of a value selected: positions, never the value.
    #[test]
    fn a_choice_from_a_menu_under_the_pointer_is_read_by_its_tag() {
        assert_eq!(
            json(&Action::Context {
                serial: 3,
                chosen: Chosen::CopyValue {
                    entry: "an entry".to_owned(),
                    field: "PIN".to_owned(),
                    range: Some(Span { from: 1, to: 4 }),
                },
            }),
            r#"{"action":"context","serial":3,"chosen":{"item":"copyValue","entry":"an entry","field":"PIN","range":{"from":1,"to":4}}}"#
        );
        assert_eq!(
            json(&Action::Context {
                serial: 1,
                chosen: Chosen::EmptyBin
            }),
            r#"{"action":"context","serial":1,"chosen":{"item":"emptyBin"}}"#
        );
        assert_eq!(
            json(&Chosen::DeleteEntries {
                entries: vec![Deleting {
                    entry: "an entry".to_owned(),
                    deletion: Deletion::Forever,
                }],
            }),
            r#"{"item":"deleteEntries","entries":[{"entry":"an entry","deletion":"forever"}]}"#
        );
        assert_eq!(
            json(&Chosen::MoveFolder {
                group: "a".to_owned(),
                into: "b".to_owned(),
            }),
            r#"{"item":"moveFolder","group":"a","into":"b"}"#
        );
    }

    /// The pointer is where the window says, and a number that is no place in
    /// the window - less than nothing, too large to be a number, not one at
    /// all - is the window's edge rather than a menu drawn off the screen.
    #[test]
    fn a_point_off_the_window_is_its_edge() {
        let at = |x: f64, y: f64| {
            let logical = Point { x, y }.logical();
            (logical.x, logical.y)
        };
        assert_eq!(at(12.5, 300.0), (12.5, 300.0));
        assert_eq!(at(-4.0, -0.0), (0.0, 0.0));
        assert_eq!(at(f64::NAN, f64::INFINITY), (0.0, 0.0));
        assert_eq!(at(f64::NEG_INFINITY, 1e9), (0.0, 1e9));

        let read: Point = serde_json::from_str(r#"{"x":10,"y":20.25}"#).expect("a point reads");
        assert_eq!((read.x, read.y), (10.0, 20.25));
        for refused in [r#"{"x":1}"#, r#"{"x":"1","y":2}"#, r#"{"x":1e999,"y":2}"#] {
            assert!(serde_json::from_str::<Point>(refused).is_err(), "{refused}");
        }
    }

    /// The whole point of the boundary: what the database protects does not
    /// cross it, and the payload says so rather than saying nothing.
    #[test]
    fn a_protected_value_is_absent_and_declared_absent() {
        let entry = Entry::of(&entry_of(vec![
            open(fields::TITLE, "a login"),
            protected(fields::PASSWORD, false),
            protected("API token", false),
            protected("empty secret", true),
        ]));

        let payload = json(&entry);
        assert!(payload.contains(
            r#""name":"Password","kind":"password","protected":true,"value":null,"empty":false"#
        ));
        assert!(payload.contains(
            r#""name":"API token","kind":"custom","protected":true,"value":null,"empty":false"#
        ));
        assert!(payload.contains(
            r#""name":"empty secret","kind":"custom","protected":true,"value":null,"empty":true"#
        ));
    }

    /// Whether a value is in lines crosses for every field, as one bit beside
    /// `empty`, and the value it is about still does not: ten recovery codes
    /// are replaced in lines without a word of them reaching the window.
    #[test]
    fn a_value_in_lines_says_so_and_nothing_more() {
        let entry = Entry::of(&entry_of(vec![
            Field {
                name: "Recovery codes".to_owned(),
                value: FieldValue::Protected {
                    empty: false,
                    lines: true,
                },
            },
            protected(fields::PASSWORD, false),
            open(fields::NOTES, "first\r\nsecond"),
            open(fields::URL, "https://example.com"),
        ]));

        let payload = json(&entry);
        for (name, lines) in [
            ("Recovery codes", true),
            ("Password", false),
            ("Notes", true),
            ("URL", false),
        ] {
            let field = entry
                .fields
                .iter()
                .find(|field| field.name == name)
                .expect("the field crosses");
            assert_eq!(field.lines, lines, "{name}");
        }
        assert!(payload.contains(r#""name":"Recovery codes","kind":"custom","protected":true,"value":null,"empty":false,"openable":false,"lines":true"#), "{payload}");
    }

    #[test]
    fn a_row_carries_what_the_list_draws_and_no_protected_value() {
        let row = EntryRow::of(&EntrySummary {
            id: EntryId::from_uuid(uuid::Uuid::nil()),
            group: GroupId::from_uuid(uuid::Uuid::nil()),
            title: FieldValue::Open("node-3".to_owned()),
            username: FieldValue::Open("deploy".to_owned()),
            url: FieldValue::Protected {
                empty: false,
                lines: false,
            },
            tags: vec!["prod".to_owned(), "ssh".to_owned()],
            times: Timestamps {
                created: None,
                modified: NaiveDate::from_ymd_opt(2026, 3, 12)
                    .and_then(|day| day.and_hms_opt(18, 42, 0)),
                expires: None,
            },
            has_password: true,
            attachments: 2,
            versions: 7,
            binned: None,
            deletion: model::Deletion::Bin,
        });

        // What deleting it would do is on the row so that the window knows it
        // before a press on several rows, rather than only once one is open.
        assert_eq!(
            json(&row),
            r#"{"id":"00000000-0000-0000-0000-000000000000","group":"00000000-0000-0000-0000-000000000000","title":"node-3","username":"deploy","url":null,"tags":["prod","ssh"],"modified":"2026-03-12T18:42:00Z","hasPassword":true,"attachments":2,"binned":null,"deletion":"bin"}"#
        );
    }

    #[test]
    fn every_field_the_format_names_is_named_and_the_rest_are_custom() {
        let entry = Entry::of(&entry_of(vec![
            open(fields::TITLE, "t"),
            open(fields::USERNAME, "u"),
            open(fields::URL, "https://example.com"),
            open(fields::NOTES, "n"),
            protected(fields::PASSWORD, false),
            open("Title ", "not the title"),
            open("title", "not the title either"),
            open("KeeAgent.settings", "machine data"),
        ]));

        let kinds: Vec<(&str, FieldKind)> = entry
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field.kind))
            .collect();

        assert_eq!(
            kinds,
            vec![
                ("Title", FieldKind::Title),
                ("UserName", FieldKind::Username),
                ("URL", FieldKind::Url),
                ("Notes", FieldKind::Notes),
                ("Password", FieldKind::Password),
                ("Title ", FieldKind::Custom),
                ("title", FieldKind::Custom),
                ("KeeAgent.settings", FieldKind::Custom),
            ]
        );
    }

    /// The screen decides whether to offer a link, and the answer is not the
    /// screen's to work out. Anything that could run code arrives marked as
    /// something Coffer will not open.
    #[test]
    fn only_an_address_coffer_would_open_is_marked_openable() {
        for (value, openable) in [
            ("https://example.com", true),
            ("HTTP://EXAMPLE.COM/x", true),
            ("  https://example.com  ", true),
            ("mailto:someone@example.com", true),
            ("javascript:alert(document.domain)", false),
            ("JaVaScRiPt:alert(1)", false),
            ("java\tscript:alert(1)", false),
            ("data:text/html,<script>alert(1)</script>", false),
            ("file:///etc/passwd", false),
            ("", false),
        ] {
            let entry = Entry::of(&entry_of(vec![open(fields::URL, value)]));
            let url = entry.fields.first().expect("the field is there");
            assert_eq!(url.openable, openable, "{value:?}");
        }

        // A protected URL has no value to judge, so there is nothing to open.
        let entry = Entry::of(&entry_of(vec![protected(fields::URL, false)]));
        assert!(!entry.fields.first().expect("the field is there").openable);

        // Only an address is ever openable. A custom field that happens to hold
        // one is a value on a screen, not a link.
        let entry = Entry::of(&entry_of(vec![open("Homepage", "https://example.com")]));
        assert!(!entry.fields.first().expect("the field is there").openable);
    }

    /// A value that is markup stays a value. It crosses as JSON text and the
    /// screen writes it as a text node; nothing here escapes it, and nothing
    /// here has to.
    #[test]
    fn markup_and_control_characters_in_a_value_cross_as_text() {
        let hostile = "<script>alert(1)</script> \" \\ </script> \u{202e}gnitirw \u{0} end";
        let entry = Entry::of(&entry_of(vec![open(fields::TITLE, hostile)]));

        // The quote and the backslash come out escaped, and the null byte
        // comes out as an escape rather than as a byte that would end the
        // string early for whoever parses it next.
        let payload = json(&entry);
        assert!(
            payload.contains(r#"<script>alert(1)</script> \" \\ </script>"#),
            "{payload}"
        );
        assert!(payload.contains(r"\u0000"), "{payload}");
        assert_eq!(
            entry
                .fields
                .first()
                .expect("the field is there")
                .value
                .as_deref(),
            Some(hostile)
        );
    }

    /// A file can hold a password the database does not protect. It is still a
    /// password, and the screen still has to ask for it one reveal at a time.
    #[test]
    fn a_password_the_database_left_open_still_does_not_cross() {
        let secret = "hunter2-not-in-any-payload";
        let entry = Entry::of(&entry_of(vec![open(fields::PASSWORD, secret)]));
        let password = entry.fields.first().expect("the field is there");

        assert_eq!(password.kind, FieldKind::Password);
        assert_eq!(password.value, None);
        assert!(!password.empty);
        assert!(!json(&entry).contains(secret));
    }

    /// The screen sends the protection back when it writes a field, so a value
    /// the database keeps protected has to arrive marked as one. Without it the
    /// screen would have to guess, and a wrong guess writes a password into the
    /// file as plain text.
    #[test]
    fn a_field_says_whether_the_database_protects_it() {
        let entry = Entry::of(&entry_of(vec![
            open(fields::TITLE, "a login"),
            protected(fields::PASSWORD, false),
            open("plain", "2202"),
            protected("API token", false),
        ]));

        let marked: Vec<(&str, bool)> = entry
            .fields
            .iter()
            .map(|field| (field.name.as_str(), field.protected))
            .collect();
        assert_eq!(
            marked,
            vec![
                ("Title", false),
                ("Password", true),
                ("plain", false),
                ("API token", true),
            ]
        );
    }

    /// The name in the file may be anything at all. The screen shows that name
    /// and the save panel is offered the other one, so a reader sees what they
    /// will get.
    #[test]
    fn an_attachment_crosses_with_the_name_a_save_panel_would_use() {
        let mut entry = entry_of(Vec::new());
        entry.attachments = vec![Attachment {
            name: "../../escape.txt".to_owned(),
            size: 12,
        }];

        let payload = json(&Entry::of(&entry));
        assert!(
            payload.contains(r#"{"name":"../../escape.txt","size":12,"fileName":"escape.txt"}"#),
            "{payload}"
        );
    }

    /// A row in the bin says when it went in, the deleted folder it went in
    /// with, and where it goes back, and each folder crosses as its id: the
    /// name is the tree's to give, so a folder
    /// renamed since is called what it is called now. Nothing known crosses as
    /// `null` rather than as a guess, and the screen says the top of the vault.
    #[test]
    fn an_entry_in_the_bin_crosses_with_when_it_went_and_where_it_goes_back() {
        let from = GroupId::from_uuid(uuid::Uuid::from_u128(7));
        let within = GroupId::from_uuid(uuid::Uuid::from_u128(8));
        let mut entry = entry_of(vec![open(fields::TITLE, "Bank")]);
        entry.binned = Some(model::Binned {
            since: NaiveDate::from_ymd_opt(2026, 9, 27).and_then(|day| day.and_hms_opt(12, 0, 0)),
            within: Some(within),
            from: Some(from),
        });
        entry.deletion = model::Deletion::Forever;

        let whole = serde_json::to_value(Entry::of(&entry)).expect("the entry serialises");
        assert_eq!(
            whole["binned"],
            serde_json::json!({
                "since": "2026-09-27T12:00:00Z",
                "within": within.to_string(),
                "from": from.to_string(),
            })
        );
        assert_eq!(whole["deletion"], "forever");

        let row = serde_json::to_value(EntryRow::of(&entry.summary())).expect("the row serialises");
        assert_eq!(row["binned"], whole["binned"]);

        entry.binned = Some(model::Binned {
            since: None,
            within: None,
            from: None,
        });
        let unknown = serde_json::to_value(Entry::of(&entry)).expect("the entry serialises");
        assert_eq!(
            unknown["binned"],
            serde_json::json!({ "since": null, "within": null, "from": null })
        );

        entry.binned = None;
        entry.deletion = model::Deletion::Bin;
        let live = serde_json::to_value(Entry::of(&entry)).expect("the entry serialises");
        assert_eq!(live["binned"], serde_json::Value::Null);
        assert_eq!(live["deletion"], "bin");
    }

    /// A folder says what deleting it would do, and so does every folder in
    /// it: the answer is Rust's for each one, never inferred by the screen from
    /// the folder above.
    #[test]
    fn every_folder_says_what_deleting_it_would_do() {
        let folder = |name: &str, deletion, sections| model::Project {
            id: GroupId::from_uuid(uuid::Uuid::new_v4()),
            name: name.to_owned(),
            notes: None,
            is_recycle_bin: false,
            is_templates: false,
            binned: None,
            deletion,
            sections,
            entries: Vec::new(),
        };
        let tree = folder(
            "Root",
            model::Deletion::Forever,
            vec![folder("Personal", model::Deletion::Bin, Vec::new())],
        );

        let payload = serde_json::to_value(Group::of(&tree)).expect("the tree serialises");
        assert_eq!(payload["deletion"], "forever");
        assert_eq!(payload["binned"], serde_json::Value::Null);
        assert_eq!(payload["sections"][0]["deletion"], "bin");
        assert_eq!(payload["sections"][0]["isRecycleBin"], false);
    }

    /// The group a vault keeps its templates in says so, and no other does:
    /// the window lists the templates from the one group marked.
    #[test]
    fn a_group_says_whether_it_holds_the_templates() {
        let folder = |name: &str, is_templates, sections| model::Project {
            id: GroupId::from_uuid(uuid::Uuid::new_v4()),
            name: name.to_owned(),
            notes: None,
            is_recycle_bin: false,
            is_templates,
            binned: None,
            deletion: model::Deletion::Bin,
            sections,
            entries: Vec::new(),
        };
        let tree = folder(
            "Root",
            false,
            vec![
                folder("Templates", true, Vec::new()),
                folder("Personal", false, Vec::new()),
            ],
        );

        let payload = serde_json::to_value(Group::of(&tree)).expect("the tree serialises");
        assert_eq!(payload["isTemplates"], false);
        assert_eq!(payload["sections"][0]["isTemplates"], true);
        assert_eq!(payload["sections"][1]["isTemplates"], false);
    }

    /// Every kind goes to the window under one word and comes back under the
    /// same word as the same kind. A word the window made up, or one spelled
    /// another way, is refused rather than read as the nearest kind: an entry
    /// nobody chose would be made.
    #[test]
    fn every_kind_crosses_by_the_name_the_window_sends_back() {
        let offered = serde_json::to_value(Kinds::of()).expect("the kinds serialise");
        let words: Vec<&str> = offered["offered"]
            .as_array()
            .expect("a list")
            .iter()
            .filter_map(|offer| offer["kind"].as_str())
            .collect();
        assert_eq!(
            words,
            [
                "login",
                "bankCard",
                "wifi",
                "identity",
                "licence",
                "recoveryCodes",
                "secureNote",
                "sshKey"
            ]
        );

        for made in kind::Kind::ALL {
            let word = serde_json::to_value(Kind::of(made)).expect("a kind serialises");
            let read: Kind = serde_json::from_value(word).expect("the word comes back");
            assert_eq!(read.wanted(), made);
        }

        for wrong in [
            "Login",
            "bank_card",
            "bankcard",
            "card",
            "",
            " login",
            "template",
            "Bank card",
        ] {
            assert!(
                serde_json::from_value::<Kind>(serde_json::json!(wrong)).is_err(),
                "{wrong:?} was read as a kind"
            );
        }
        assert!(serde_json::from_value::<Kind>(serde_json::json!(0)).is_err());
        assert!(serde_json::from_value::<Kind>(serde_json::json!(null)).is_err());
    }

    /// What "+ Entry" makes unasked is a login, the first thing offered.
    #[test]
    fn the_first_kind_offered_is_a_login() {
        let offered = serde_json::to_value(Kinds::of()).expect("the kinds serialise");
        assert_eq!(offered["offered"][0]["kind"], "login");
        assert_eq!(offered["offered"][0]["name"], "Login");
        assert_eq!(offered["offered"][0]["lined"], serde_json::json!([]));
    }

    /// A kind says which of its fields it writes in lines, and only those; what
    /// it hides arrives with the entry it makes, and the names offered for a
    /// field of the reader's own say whether each is hidden.
    #[test]
    fn a_kind_offers_the_fields_it_writes_in_lines() {
        let offered = serde_json::to_value(Kinds::of()).expect("the kinds serialise");
        let lined = |word: &str| {
            offered["offered"]
                .as_array()
                .and_then(|all| all.iter().find(|offer| offer["kind"] == word))
                .map(|offer| offer["lined"].clone())
        };
        assert_eq!(
            lined("recoveryCodes"),
            Some(serde_json::json!(["Recovery codes"]))
        );
        assert_eq!(
            lined("secureNote"),
            Some(serde_json::json!(["Secret note"]))
        );
        assert_eq!(lined("licence"), Some(serde_json::json!(["Licence key"])));
        assert_eq!(lined("bankCard"), Some(serde_json::json!([])));
        assert_eq!(
            offered["suggested"],
            serde_json::json!([
                { "name": "PIN", "protect": true },
                { "name": "Account number", "protect": false },
                { "name": "Security answer", "protect": true },
            ])
        );
    }

    #[test]
    fn a_million_characters_of_title_cross_whole() {
        let long = "a".repeat(1_000_000);
        let entry = Entry::of(&entry_of(vec![open(fields::TITLE, &long)]));
        assert_eq!(
            entry
                .fields
                .first()
                .expect("the field is there")
                .value
                .as_deref(),
            Some(long.as_str())
        );
    }

    /// KDBX can hold a date centuries either side of now, and the screen has to
    /// be handed one it can read rather than one it has to guess at.
    #[test]
    fn timestamps_at_the_edges_of_the_range_cross_as_utc() {
        for (year, expected) in [
            (1600, "1600-01-01T00:00:00Z"),
            (3000, "3000-12-31T23:59:59Z"),
        ] {
            let time = if year == 1600 {
                NaiveDate::from_ymd_opt(1600, 1, 1).and_then(|day| day.and_hms_opt(0, 0, 0))
            } else {
                NaiveDate::from_ymd_opt(3000, 12, 31).and_then(|day| day.and_hms_opt(23, 59, 59))
            };
            assert_eq!(stamp(time).as_deref(), Some(expected));
        }

        assert_eq!(stamp(None), None);
    }

    /// The window branches on one key, and a name that is taken crosses as a
    /// question: two sizes and two names, the first exactly as the entry holds
    /// it whatever is in it, and not a byte of either file.
    #[test]
    fn a_taken_name_crosses_as_a_question_with_no_bytes_in_it() {
        let hostile = "../../<b>scan</b>\u{202e}fdp.pdf";
        let taken = serde_json::to_value(Attached::Taken {
            clash: Clash::of(
                hostile.to_owned(),
                3,
                vault_core::Clash {
                    size: 7,
                    free: "scan 2.pdf".to_owned(),
                },
            ),
        })
        .expect("the payload serialises");
        assert_eq!(
            taken,
            serde_json::json!({
                "outcome": "taken",
                "clash": { "name": hostile, "size": 7, "chosen": 3, "free": "scan 2.pdf" }
            })
        );

        let added = serde_json::to_value(Attached::Added {
            entry: Entry::of(&entry_of(Vec::new())),
        })
        .expect("the payload serialises");
        assert_eq!(added["outcome"], "added");
        assert_eq!(added["entry"]["id"], uuid::Uuid::nil().to_string());
    }

    #[test]
    fn an_attachment_crosses_with_its_name_exactly_as_the_file_holds_it() {
        let mut entry = entry_of(Vec::new());
        entry.attachments = vec![
            Attachment {
                name: "../../escape.txt".to_owned(),
                size: 0,
            },
            Attachment {
                name: "nested/path/name.txt".to_owned(),
                size: 100 * 1024 * 1024,
            },
        ];

        let payload = json(&Entry::of(&entry));
        assert!(payload.contains(r#""name":"../../escape.txt","size":0"#));
        assert!(payload.contains(r#""name":"nested/path/name.txt","size":104857600"#));
    }

    /// The window is told what it may choose. A message that answered with a
    /// list of its own would be the window widening that, which is what the
    /// `skip_deserializing` on the lists is for - and what a new list could
    /// quietly miss.
    #[test]
    fn a_look_the_window_made_up_is_settled_like_a_number_nobody_offered() {
        let asked: Settings = serde_json::from_str(
            r#"{"idleSeconds":900,"clipboardSeconds":15,"lockOnSleep":false,
                "lockOnScreenLock":true,"theme":"neon","themeChoices":["neon"]}"#,
        )
        .expect("the window's message parses");

        assert!(
            asked.theme_choices.is_empty(),
            "the window sent a list back"
        );
        assert!(asked.idle_choices.is_empty() && asked.clipboard_choices.is_empty());

        let wanted = asked.wanted();
        assert_eq!(wanted.theme, settings::Theme::Dark);
        assert_eq!(wanted.idle_seconds, 900);
        assert_eq!(wanted.clipboard_seconds, 15);
        assert!(!wanted.lock_on_sleep && wanted.lock_on_screen_lock);
    }

    /// A deletion from the window names each entry with the answer it was
    /// shown, in the two words the window spells them with and no others: a
    /// word it made up is refused, not read as either of the two.
    #[test]
    fn a_deletion_reads_each_entry_with_what_the_window_showed() {
        let read: Vec<Deleting> = serde_json::from_str(
            r#"[{"entry":"00000000-0000-0000-0000-000000000000","deletion":"forever"},{"entry":"00000000-0000-0000-0000-000000000001","deletion":"bin"}]"#,
        )
        .expect("both are read");
        let shown: Vec<_> = read
            .iter()
            .map(|deleting| deleting.shown().expect("the ids parse"))
            .collect();
        assert_eq!(
            shown,
            [
                (
                    EntryId::from_uuid(uuid::Uuid::nil()),
                    model::Deletion::Forever
                ),
                (
                    EntryId::from_uuid(uuid::Uuid::from_u128(1)),
                    model::Deletion::Bin
                ),
            ]
        );

        for refused in [
            r#"{"entry":"00000000-0000-0000-0000-000000000000","deletion":"Bin"}"#,
            r#"{"entry":"00000000-0000-0000-0000-000000000000","deletion":"trash"}"#,
            r#"{"entry":"00000000-0000-0000-0000-000000000000"}"#,
            r#"{"deletion":"bin"}"#,
        ] {
            assert!(
                serde_json::from_str::<Deleting>(refused).is_err(),
                "{refused}"
            );
        }
    }

    /// One id in a batch that is not one refuses the batch, wherever it sits:
    /// the entries before it are not asked about either.
    #[test]
    fn an_identifier_in_a_batch_that_is_not_one_refuses_the_batch() {
        let good = "00000000-0000-0000-0000-000000000000".to_owned();
        assert_eq!(
            entry_ids(&[good.clone(), good.clone()])
                .expect("both parse")
                .len(),
            2
        );
        assert!(entry_ids(&[]).expect("nothing is nothing").is_empty());
        for bad in [
            "",
            "../../../etc/passwd",
            "00000000-0000-0000-0000-00000000000",
        ] {
            assert!(
                entry_ids(&[good.clone(), bad.to_owned()]).is_err(),
                "{bad:?}"
            );
            assert!(
                entry_ids(&[bad.to_owned(), good.clone()]).is_err(),
                "{bad:?}"
            );
            let deleting = Deleting {
                entry: bad.to_owned(),
                deletion: Deletion::Bin,
            };
            assert!(deleting.shown().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn an_identifier_that_is_not_one_names_no_entry() {
        assert!(entry_id("00000000-0000-0000-0000-000000000000").is_ok());
        for text in [
            "",
            "not a uuid",
            "00000000-0000-0000-0000-00000000000",
            "'; drop table entries; --",
            "../../../etc/passwd",
        ] {
            assert!(entry_id(text).is_err(), "{text:?}");
        }
    }

    /// What the screen is told about a copy and the vault it would go over is
    /// file names and times, never a path: the paths stay in Rust, which is
    /// where the move is made. The file it would go over is named as the
    /// snapshot it becomes, so the sentence that promises it is Rust's rule
    /// for snapshot names and not a second one in the window.
    #[test]
    fn a_copy_crosses_with_names_and_times_and_no_path() {
        let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_790_000_000);
        let vault = std::path::Path::new("/Users/someone/Vault/work <b>.kdbx");

        let copy = serde_json::to_value(CopyOf::of(
            vault,
            vault_core::storage::OnDisk::Written(Some(at)),
            vault_core::storage::OnDisk::Gone,
        ))
        .expect("the copy serialises");
        assert_eq!(copy["vault"], "work <b>.kdbx");
        assert_eq!(copy["keptAs"], "work <b>.kdbx.1.bak");
        assert_eq!(copy["saved"], "2026-09-21T14:13:20Z");
        assert_eq!(copy["vaultFile"]["there"], false);
        assert!(!copy.to_string().contains("/Users"), "{copy}");

        let gone = serde_json::to_value(CopyOf::of(
            vault,
            vault_core::storage::OnDisk::Gone,
            vault_core::storage::OnDisk::Written(Some(at)),
        ))
        .expect("the copy serialises");
        assert_eq!(gone["saved"], serde_json::Value::Null);
        assert_eq!(gone["vaultFile"]["written"], "2026-09-21T14:13:20Z");

        let there = serde_json::to_value(OnDisk::of(vault_core::storage::OnDisk::Written(None)))
            .expect("the file serialises");
        assert_eq!(there, serde_json::json!({ "there": true, "written": null }));
        let missing = serde_json::to_value(OnDisk::of(vault_core::storage::OnDisk::Gone))
            .expect("the file serialises");
        assert_eq!(
            missing,
            serde_json::json!({ "there": false, "written": null })
        );
    }

    /// A made password is a secret from the moment it exists. It crosses as the
    /// one string the window puts in its node, exactly as it was made whatever
    /// is in it, and not a second time anywhere else in the payload. Around it
    /// every key is spelled the way the window's types spell it.
    #[test]
    fn a_made_password_crosses_once_exactly_beside_the_generator_it_came_from() {
        // A value that would end the string early for anyone building the
        // payload by hand, and go on to claim that nothing is missing.
        let made = "q\"\\\u{e9}\u{1f512}\u{202e}\u{0}\",\"missing\":[],\"x\":\"";
        let recipe = making(
            vec![generate::Alphabet::Lower, generate::Alphabet::Digits],
            true,
            "\"'\\`",
        );

        let payload = json(&Generated::of(made, &recipe));
        let parsed: serde_json::Value = serde_json::from_str(&payload).expect("the payload parses");
        assert_eq!(
            parsed,
            serde_json::json!({
                "value": made,
                "missing": ["digits"],
                "generator": {
                    "recipe": {
                        "length": 12,
                        "alphabets": ["lower", "digits"],
                        "similar": true,
                        "avoid": "\"'\\`",
                    },
                    "shortest": 8,
                    "longest": 64,
                    "pin": false,
                    "symbols": generate::Alphabet::Symbols.characters(),
                    "lookAlikes": "0O1lI|",
                },
            })
        );

        let written = serde_json::to_string(made).expect("a string serialises");
        assert_eq!(payload.matches(written.as_str()).count(), 1, "{payload}");
        // Not inside another string either, where its own quotes would not be.
        let unquoted = written
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .expect("a string is written in quotes");
        assert_eq!(payload.matches(unquoted).count(), 1, "{payload}");
    }

    /// The window names the four kinds with four words of its own type, and a
    /// kind crosses under that word whichever way it goes. The kinds a password
    /// lacks come in the order the switches stand, not the order they were
    /// asked for in.
    #[test]
    fn every_kind_crosses_both_ways_under_the_word_the_window_spells_it_with() {
        let backwards = making(
            generate::Alphabet::ALL.into_iter().rev().collect(),
            true,
            "",
        );
        let lacking =
            serde_json::to_value(Generated::of("", &backwards)).expect("the payload serialises");
        assert_eq!(
            lacking["missing"],
            serde_json::json!(["lower", "upper", "digits", "symbols"])
        );

        for (word, kind) in [
            ("lower", generate::Alphabet::Lower),
            ("upper", generate::Alphabet::Upper),
            ("digits", generate::Alphabet::Digits),
            ("symbols", generate::Alphabet::Symbols),
        ] {
            assert_eq!(
                serde_json::to_value(Alphabet::of(kind)).expect("a kind serialises"),
                word
            );
            let read: Alphabet = serde_json::from_value(serde_json::json!(word))
                .expect("a kind the window spells parses");
            assert_eq!(read.alphabet(), kind);
        }
    }

    /// What the window sends is read as it was sent. Settling it is the
    /// command's to do, once, so a kind asked for twice or out of order is
    /// still that here, and a quote, a backslash, a null or a letter outside
    /// ASCII in the list to avoid arrives as itself.
    #[test]
    fn a_recipe_from_the_window_is_read_as_it_was_sent() {
        let asked: Recipe = serde_json::from_str(
            r#"{"length":12,"alphabets":["symbols","lower","symbols"],"similar":true,
                "avoid":"\"'\\`\u00e9\u0000"}"#,
        )
        .expect("the window's recipe parses");

        assert_eq!(
            asked.recipe(),
            generate::Recipe {
                length: 12,
                alphabets: vec![
                    generate::Alphabet::Symbols,
                    generate::Alphabet::Lower,
                    generate::Alphabet::Symbols,
                ],
                similar: true,
                avoid: "\"'\\`\u{e9}\u{0}".to_owned(),
            }
        );
    }

    /// A kind the window made up is refused, rather than read as the nearest
    /// kind there is or dropped from the list: either would make a password
    /// from a set nobody chose. A name serde does not know is refused as an
    /// unknown kind, and anything that is not a name at all, or a length or a
    /// switch that is not one, is refused before a recipe is made of it.
    #[test]
    fn a_kind_the_window_made_up_is_refused_rather_than_guessed_at() {
        for sent in [
            r#"{"alphabets":["emoji"]}"#,
            r#"{"alphabets":["Lower"]}"#,
            r#"{"alphabets":["LOWER"]}"#,
            r#"{"alphabets":[" lower"]}"#,
            r#"{"alphabets":["digit"]}"#,
            r#"{"alphabets":["Digits"]}"#,
            r#"{"alphabets":["look_alikes"]}"#,
            r#"{"alphabets":[""]}"#,
            r#"{"alphabets":["lower","emoji","digits"]}"#,
        ] {
            let refused = serde_json::from_str::<Recipe>(sent).err();
            assert_eq!(
                refused.as_ref().map(serde_json::Error::classify),
                Some(serde_json::error::Category::Data),
                "{sent}"
            );
            assert!(
                refused.is_some_and(|error| error.to_string().contains("unknown variant")),
                "{sent}"
            );
        }

        for sent in [
            r#"{"alphabets":[0]}"#,
            r#"{"alphabets":[null]}"#,
            r#"{"alphabets":[["lower"]]}"#,
            r#"{"alphabets":"lower"}"#,
            r#"{"length":-1}"#,
            r#"{"length":12.5}"#,
            r#"{"length":"12"}"#,
            r#"{"length":18446744073709551616}"#,
            r#"{"similar":"false"}"#,
            r#"{"avoid":["\""]}"#,
        ] {
            assert!(serde_json::from_str::<Recipe>(sent).is_err(), "{sent}");
        }
    }

    /// A recipe missing a field - one an older Coffer did not write, or one
    /// the window left out - takes the default for that field alone and keeps
    /// every other as it was sent.
    #[test]
    fn a_field_left_out_of_a_recipe_takes_its_default_and_costs_nothing_else() {
        let nothing: Recipe = serde_json::from_str("{}").expect("an empty recipe parses");
        assert_eq!(nothing.recipe(), generate::Recipe::default());

        // Every field away from its default, so that each one left out shows.
        let sent = generate::Recipe {
            length: 6,
            alphabets: vec![generate::Alphabet::Digits],
            similar: true,
            avoid: "7".to_owned(),
        };
        let fallback = generate::Recipe::default();
        for (left_out, expected) in [
            (
                "length",
                generate::Recipe {
                    length: fallback.length,
                    ..sent.clone()
                },
            ),
            (
                "alphabets",
                generate::Recipe {
                    alphabets: fallback.alphabets.clone(),
                    ..sent.clone()
                },
            ),
            (
                "similar",
                generate::Recipe {
                    similar: fallback.similar,
                    ..sent.clone()
                },
            ),
            (
                "avoid",
                generate::Recipe {
                    avoid: fallback.avoid.clone(),
                    ..sent.clone()
                },
            ),
        ] {
            let mut whole = serde_json::to_value(Recipe::of(&sent)).expect("the recipe serialises");
            let removed = whole.as_object_mut().and_then(|keys| keys.remove(left_out));
            assert!(removed.is_some(), "a recipe is not written with {left_out}");

            let read: Recipe = serde_json::from_value(whole).expect("the rest parses");
            assert_eq!(read.recipe(), expected, "{left_out}");
        }
    }

    /// The window gets the generator back with every password, and nothing it
    /// sends beside a recipe changes what the generator allows: a range, a PIN
    /// or a set of symbols sent back is not read, and a length the slider
    /// cannot show comes back as the one the engine settled on.
    #[test]
    fn a_range_the_window_sends_back_widens_nothing() {
        let asked: Recipe = serde_json::from_str(
            r#"{"length":2,"alphabets":["lower"],"similar":false,"avoid":"",
                "shortest":1,"longest":100000,"pin":true,"symbols":"a","lookAlikes":""}"#,
        )
        .expect("the window's recipe parses");

        let answer = serde_json::to_value(Generated::of("abcdefgh", &asked.recipe().settled()))
            .expect("the payload serialises");
        let drawn = &answer["generator"];
        assert_eq!(drawn["recipe"]["length"], 8);
        assert_eq!(drawn["shortest"], 8);
        assert_eq!(drawn["longest"], 64);
        assert_eq!(drawn["pin"], false);
        assert_eq!(drawn["symbols"], generate::Alphabet::Symbols.characters());
        assert_eq!(drawn["lookAlikes"], "0O1lI|");

        // Look-alikes sent back with a PIN do not take 0 and 1 out of it, and
        // none sent back with a password do not let them into it.
        for (sent, length, look_alikes) in [
            (
                r#"{"length":18446744073709551615,"alphabets":["digits"],"lookAlikes":"0O1lI|"}"#,
                64,
                "",
            ),
            (r#"{"length":0,"alphabets":["digits"]}"#, 4, ""),
            (r#"{"length":0,"lookAlikes":""}"#, 8, "0O1lI|"),
        ] {
            let asked: Recipe = serde_json::from_str(sent).expect("the window's recipe parses");
            let drawn = serde_json::to_value(Generator::of(&asked.recipe().settled()))
                .expect("the generator serialises");
            assert_eq!(drawn["recipe"]["length"], length, "{sent}");
            assert_eq!(drawn["lookAlikes"], look_alikes, "{sent}");
        }
    }

    /// Which generator is asking decides which recipe is remembered, so a
    /// purpose the window did not spell exactly is refused rather than read as
    /// the nearer one: a PIN made for a card's field, remembered as the
    /// password's recipe, is what the next account's password is made from.
    #[test]
    fn a_purpose_is_one_of_two_words_spelled_exactly_and_nothing_else() {
        let read = |sent: serde_json::Value| serde_json::from_value::<Purpose>(sent);
        assert!(matches!(
            read(serde_json::json!("password")),
            Ok(Purpose::Password)
        ));
        assert!(matches!(
            read(serde_json::json!("field")),
            Ok(Purpose::Field)
        ));

        for sent in [
            serde_json::json!("Password"),
            serde_json::json!("PASSWORD"),
            serde_json::json!("Field"),
            serde_json::json!(""),
            serde_json::json!(" password"),
            serde_json::json!("field "),
            serde_json::json!("passwords"),
            serde_json::json!("password\u{0}"),
            serde_json::json!("\u{200b}field"),
            serde_json::json!("p\u{430}ssword"),
            serde_json::json!("pin"),
            serde_json::json!(null),
            serde_json::json!(0),
            serde_json::json!(1),
            serde_json::json!(true),
            serde_json::json!([]),
            serde_json::json!(["password"]),
            serde_json::json!({}),
            serde_json::json!({ "password": "field" }),
            serde_json::json!({ "password": null, "field": null }),
        ] {
            assert!(read(sent.clone()).is_err(), "{sent}");
        }
    }

    /// One shape writes a recipe down, for the window and for the file the
    /// generator remembers it in, so whatever goes out through it comes back as
    /// it went: every set of kinds, in any order and twice over, either way the
    /// look-alike switch stands, and any length or list to avoid.
    #[test]
    fn every_recipe_comes_back_through_the_shape_it_is_written_in_as_it_went() {
        let mut sets: Vec<Vec<generate::Alphabet>> = (0..16).map(kinds).collect();
        sets.push(vec![
            generate::Alphabet::Symbols,
            generate::Alphabet::Lower,
            generate::Alphabet::Symbols,
        ]);

        for alphabets in sets {
            for similar in [false, true] {
                for avoid in ["", "\"'\\`", "\u{0}\u{e9}\u{1f512}\u{202e}", "0O1lI|"] {
                    for length in [0, 4, 24, usize::MAX] {
                        let recipe = generate::Recipe {
                            length,
                            alphabets: alphabets.clone(),
                            similar,
                            avoid: avoid.to_owned(),
                        };
                        assert_eq!(Recipe::of(&recipe).recipe(), recipe);

                        let written = json(&Recipe::of(&recipe));
                        let read: Recipe =
                            serde_json::from_str(&written).expect("a written recipe parses");
                        assert_eq!(read.recipe(), recipe, "{written}");
                    }
                }
            }
        }
    }

    /// The slider goes down to four only for digits and nothing else. Any
    /// other set - none at all, or digits with anything beside them - makes a
    /// password, and a password starts at eight.
    #[test]
    fn only_digits_alone_open_the_slider_to_a_pin() {
        let mut sets: Vec<Vec<generate::Alphabet>> = (0..16).map(kinds).collect();
        sets.push(vec![generate::Alphabet::Digits, generate::Alphabet::Digits]);
        sets.push(vec![
            generate::Alphabet::Digits,
            generate::Alphabet::Symbols,
            generate::Alphabet::Digits,
        ]);

        for alphabets in sets {
            let pin = matches!(
                alphabets.as_slice(),
                [generate::Alphabet::Digits]
                    | [generate::Alphabet::Digits, generate::Alphabet::Digits]
            );
            let drawn = serde_json::to_value(Generator::of(&making(alphabets.clone(), false, "")))
                .expect("the generator serialises");
            assert_eq!(drawn["pin"], pin, "{alphabets:?}");
            assert_eq!(drawn["shortest"], if pin { 4 } else { 8 }, "{alphabets:?}");
            assert_eq!(drawn["longest"], 64, "{alphabets:?}");
        }
    }

    /// The line under the switches says what "Symbols" draws from, and the
    /// look-alike switch says what it leaves out. Both come from the engine,
    /// and what they say has to be true of it: every printable ASCII character
    /// that is neither a letter, a digit nor a space, in order, and six
    /// characters each of which some switch would otherwise draw.
    #[test]
    fn the_generator_names_every_symbol_it_draws_and_every_look_alike_it_leaves_out() {
        let printable: String = (0x21u8..=0x7e)
            .map(char::from)
            .filter(|character| !character.is_ascii_alphanumeric())
            .collect();
        let drawn = serde_json::to_value(Generator::of(&generate::Recipe::default()))
            .expect("the generator serialises");
        let look_alikes = "0O1lI|";
        assert_eq!(drawn["symbols"], printable.as_str());
        assert_eq!(drawn["lookAlikes"], look_alikes);

        for look_alike in look_alikes.chars() {
            assert!(
                generate::Alphabet::ALL
                    .iter()
                    .any(|kind| kind.characters().contains(look_alike)),
                "{look_alike:?} is drawn by no switch"
            );
        }
    }

    /// The window draws the look-alike switch only when it has something to
    /// leave out, and what it is told it leaves out has to be what the engine
    /// leaves out. Digits alone are a PIN, which keeps 0 and 1: four digits
    /// without them would be 4,096 PINs where there are 10,000. Beside any
    /// other kind they are look-alikes again.
    #[test]
    fn a_pin_is_offered_no_look_alikes_to_avoid_and_keeps_every_digit() {
        let drawn = |alphabets: Vec<generate::Alphabet>| {
            serde_json::to_value(Generator::of(&making(alphabets, false, "")))
                .expect("the generator serialises")
        };
        let pin = drawn(vec![generate::Alphabet::Digits]);
        assert_eq!(pin["pin"], true);
        assert_eq!(pin["shortest"], 4);
        assert_eq!(pin["lookAlikes"], "");
        let password = drawn(vec![generate::Alphabet::Digits, generate::Alphabet::Lower]);
        assert_eq!(password["pin"], false);
        assert_eq!(password["shortest"], 8);
        assert_eq!(password["lookAlikes"], "0O1lI|");

        let mut sets: Vec<Vec<generate::Alphabet>> = (1..16).map(kinds).collect();
        sets.push(vec![generate::Alphabet::Digits, generate::Alphabet::Digits]);
        for alphabets in sets {
            let is_pin = alphabets
                .iter()
                .all(|kind| *kind == generate::Alphabet::Digits);
            // Everything the kinds hold but the look-alikes is avoided, so all
            // that is left to draw is what the switch lets through: every kind
            // holds at least one of them, and a password made with the switch
            // on is nothing else.
            let held: String = alphabets
                .iter()
                .flat_map(|kind| kind.characters().chars())
                .collect();
            let avoid: String = held
                .chars()
                .filter(|character| !"0O1lI|".contains(*character))
                .collect();

            for similar in [false, true] {
                let recipe = generate::Recipe {
                    length: 64,
                    alphabets: alphabets.clone(),
                    similar,
                    avoid: avoid.clone(),
                };
                let told =
                    serde_json::to_value(Generator::of(&recipe)).expect("the generator serialises");
                let left_out = if is_pin { "" } else { "0O1lI|" };
                assert_eq!(told["lookAlikes"], left_out, "{alphabets:?} {similar}");

                let made = generate::password(&recipe);
                if similar || is_pin {
                    let made = made.expect("a look-alike is left to draw from");
                    assert!(
                        made.chars()
                            .all(|character| "0O1lI|".contains(character)
                                && held.contains(character)),
                        "{alphabets:?} {similar}"
                    );
                    if is_pin {
                        // Sixty-four draws of two digits miss one of them one
                        // time in 2^63.
                        assert!(made.contains('0') && made.contains('1'), "{similar}");
                    }
                } else {
                    assert!(
                        matches!(made, Err(vault_core::VaultError::NothingToGenerateFrom)),
                        "{alphabets:?}"
                    );
                }
            }
        }
    }

    /// A kind the recipe left nothing of could not have been in the password,
    /// and the window is not told to make another one for its sake.
    #[test]
    fn a_kind_left_nothing_to_draw_from_is_never_said_to_be_missing() {
        let missing = |made: &str, recipe: &generate::Recipe| {
            serde_json::to_value(Generated::of(made, recipe)).expect("the payload serialises")
                ["missing"]
                .clone()
        };
        let none = serde_json::json!([]);

        // Two digits gone to the look-alike switch and the other eight to the
        // list; let the look-alikes back and a digit could have been there.
        let no_digits = making(
            vec![generate::Alphabet::Lower, generate::Alphabet::Digits],
            false,
            "23456789",
        );
        assert_eq!(missing("abcdefghjkmn", &no_digits), none);
        let similar = generate::Recipe {
            similar: true,
            ..no_digits.clone()
        };
        assert_eq!(
            missing("abcdefghjkmn", &similar),
            serde_json::json!(["digits"])
        );
        // The same eight digits on the list of a PIN leave 0 and 1, which a
        // PIN keeps, so a PIN without a digit in it is missing its digits.
        let zero_and_one = making(vec![generate::Alphabet::Digits], false, "23456789");
        assert_eq!(missing("", &zero_and_one), serde_json::json!(["digits"]));
        assert_eq!(missing("0110", &zero_and_one), none);

        // Every symbol but the bar on the list, and the bar a look-alike.
        let all_but_the_bar: String = generate::Alphabet::Symbols
            .characters()
            .chars()
            .filter(|character| *character != '|')
            .collect();
        let no_symbols = making(
            vec![generate::Alphabet::Upper, generate::Alphabet::Symbols],
            false,
            &all_but_the_bar,
        );
        assert_eq!(missing("ABCDEFGH", &no_symbols), none);
        let similar = generate::Recipe {
            similar: true,
            ..no_symbols.clone()
        };
        assert_eq!(
            missing("ABCDEFGH", &similar),
            serde_json::json!(["symbols"])
        );

        // Nothing at all to draw from is nothing missing.
        let nothing = making(vec![generate::Alphabet::Digits], true, "0123456789");
        assert_eq!(missing("", &nothing), none);
        // A kind nobody asked for is never missing, however absent it is.
        let letters = making(vec![generate::Alphabet::Lower], false, "");
        assert_eq!(missing("abcdefgh", &letters), none);
    }
}
