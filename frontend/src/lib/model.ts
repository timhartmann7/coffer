/**
 * The shapes that cross the IPC boundary, as `docs/ipc.md` describes them.
 *
 * One rule runs through all of them: a field's `value` is `null` exactly when
 * the database protects it. A protected value is not here, has never been here,
 * and arrives only through an explicit reveal.
 */

/** The database Coffer will open, or has open. */
export interface Database {
	/** Absolute, with symbolic links already followed. */
	path: string;
	/** The file name without its extension, which is what the window says. */
	name: string;
}

/**
 * What is at the name where a new vault would go. Nothing is ever made over
 * anything, so the screen says so before anybody types a password, and says
 * what it is, because only a vault can be opened instead:
 * - `free`: nothing, so a vault can go here;
 * - `vault`: a file with something in it;
 * - `copy`: nothing, but the copy a lock left of a vault by this name is
 *   beside it, and that name's unlock screen puts it back;
 * - `empty`: an empty file, which is what a creation killed half way leaves;
 * - `other`: a folder, or a link to one or to nothing.
 */
type Standing = 'free' | 'vault' | 'copy' | 'empty' | 'other';

/** Where a new vault would go, as the creation screen draws it. The place
 * itself stays in Rust. */
export interface Target {
	/** The path the way its owner would write it: under the home folder it
	 * starts with `~`. For showing, never for opening. */
	shown: string;
	standing: Standing;
}

/** A vault sitting in Coffer's own folder, offered when nothing is remembered.
 * The path does not cross: Rust keeps the one it named, and opening it asks
 * Rust whether that file is still a vault. */
export interface Found {
	/** The file name, which is what the offer says. */
	name: string;
	/** The folder in the home folder it was found in. */
	folder: string;
	/** Whether nothing is at the name any more, and what was found is the copy
	 * a lock left of the vault beside it. The offer must not say it found a
	 * file that is not there. */
	copy: boolean;
}

export interface Status {
	database: Database | null;
	/** A vault in Coffer's own folder, when nothing is remembered to open. */
	found: Found | null;
	/** The key file the next unlock will use, when the reader has chosen one.
	 * It comes from Rust rather than from this window because a lock destroys
	 * the window and the vault it is about is still the same one. */
	keyFile: Database | null;
	unlocked: boolean;
	/** How many entries the vault holds, the recycle bin's included. */
	entries: number;
	/** Whether this database can be written back at all. */
	readOnly: boolean;
	/** Which of the reasons it cannot be, while one that cannot is open. */
	readOnlyBecause: ReadOnlyBecause | null;
	/** Whether what is open can be written to a file somewhere else with
	 * `saveCopy`: anything but a format Coffer will not write. */
	copyable: boolean;
	/** What is known of copies of the open vault kept on another disk. Null
	 * while no vault is open - the unlock screen says nothing of copies - and
	 * for a backup, the copy a lock left, or a format Coffer will not write
	 * anywhere. */
	elsewhere: Elsewhere | null;
	/** The unsaved copy sitting beside the database Coffer will open next, when
	 * a lock had to write one. Read off the disk rather than remembered, so a
	 * copy left by a run that has since quit is still offered. */
	rescue: Rescued | null;
	/** Whether the last lock in this run found work the file had not got and
	 * could not put it anywhere at all. There is no file to point at, which is
	 * why this is a flag and not a path. */
	lost: boolean;
	/** The chosen database's own file as it stands. Read off the disk each
	 * time, because it is what a lock left and what the reader may since have
	 * moved. */
	file: OnDisk | null;
	/** The vault the chosen database was copied from, when it is the copy a
	 * lock left. */
	copy: CopyOf | null;
	/** The vault the chosen database was taken beside, when it is one of its
	 * backups. */
	snapshot: SnapshotOf | null;
	/** Whether the last lock found text the reader was still typing and saved
	 * it into the vault with everything else. It does not say which entry: after
	 * a lock nothing of the vault is left to say it with. */
	typed: boolean;
	/** Whether some of what it saved was a new value typed in a Change field and
	 * kept in a field of its own beside the value it was for, which the field
	 * still holds. Which field is not said either. */
	typedBeside: boolean;
	/** Whether the vault the last lock closed was given a new master password
	 * while it was open. The window that said so went with the lock, and a
	 * change that finished as the vault locked may never have been said. */
	rekeyed: boolean;
	/** What became of the vault's file, when the vault the last lock closed had
	 * just been made from one of its backups and nothing had happened in it
	 * since. The notice that said so went with the window, and a lock that
	 * landed while the press ran took it before it could. */
	adopted: Adopted | null;
	/** Whether the backup the reader asked to look at from the settings was
	 * pushed out of the chain by the save of the lock on the way to it, which
	 * left the vault chosen. */
	backupGone: boolean;
	/** Why the vault that was open is not open any more, when it is worth
	 * saying. A lock the reader asked for has nothing to explain. */
	lockedBy: 'idle' | 'sleeping' | 'screenLocked' | 'sessionSwitched' | null;
	/** Seconds until the open vault locks itself. */
	locksIn: number | null;
}

/** The unsaved copy a lock left beside the vault. */
export interface Rescued {
	name: string;
	written: string | null;
}

/** A file as the disk has it now. */
export interface OnDisk {
	/** Whether anything at all is at the name. */
	there: boolean;
	/** When it was last written, when it is there and the filesystem keeps the
	 * time. */
	written: string | null;
}

/** The vault a chosen database was copied from, when it is the copy a lock
 * left. Names and times only: the paths stay in Rust. */
export interface CopyOf {
	/** The vault's file name, which is the file the copy would go over. */
	vault: string;
	/** When the copy was last written. */
	saved: string | null;
	/** What the vault's file is called once the copy has gone over it: the
	 * newest snapshot. */
	keptAs: string;
	/** The vault's file as it stands, which the copy would go over or, when it
	 * has gone, take the name of. */
	vaultFile: OnDisk;
}

/**
 * Why a database cannot be written back:
 * - `snapshot`: one of the backups Coffer keeps beside a vault;
 * - `place`: kept somewhere that will not take a file;
 * - `kdb`: the older format Coffer reads and does not write;
 * - `kdbx3`: KDBX 3 holding files, which Coffer cannot write back whole.
 * Only the last two refuse a copy somewhere else.
 */
export type ReadOnlyBecause = 'snapshot' | 'place' | 'kdb' | 'kdbx3';

/** The vault a chosen database was taken beside, when it is one of its
 * backups. Names and times only: the paths stay in Rust. */
export interface SnapshotOf {
	/** The vault's file name, which is the file the backup would go over. */
	vault: string;
	/** When the backup's file was written, which is when the vault was as the
	 * backup holds it. */
	taken: string | null;
	/** What the vault's file is called once the backup has gone over it, when
	 * it opens with the backup's password: the newest snapshot. */
	keptAs: string;
	/** The vault's file as it stands, which the backup would go over or, when
	 * it has gone, take the name of. */
	vaultFile: OnDisk;
	/** Why the backup is open: the vault's file would not open or was not
	 * there, or the reader asked to look. */
	because: 'unopened' | 'asked';
}

/** What Coffer knows of copies of the open vault kept on another disk: dates
 * and a disk's name. No path crosses, not even the folder a copy went to. */
export interface Elsewhere {
	/** The newest copy saved through Coffer on a disk other than the vault's,
	 * or null when none was. */
	otherDisk: OtherDisk | null;
	/** When the newest copy was saved, while that one went to the vault's own
	 * disk: it goes with the vault the day that disk fails. */
	sameDiskAt: string | null;
	/** Whole days the vault has gone without a copy on another disk, once that
	 * is thirty or more and it holds entries. Rust's rule, so the window holds
	 * no copy of it. */
	overdue: number | null;
}

/** A copy on another disk: when, and what the disk is called. */
interface OtherDisk {
	at: string;
	/** The name the Finder shows for the disk, when it is mounted in
	 * `/Volumes`. */
	volume: string | null;
}

/** A copy `copyVault` has just saved: whether it went to the vault's own disk
 * and what that disk is called, for the notice to say, and what is known of
 * copies of the vault now, for everything else. */
export interface SavedCopy {
	sameDisk: boolean;
	volume: string | null;
	elsewhere: Elsewhere;
}

/** A backup made the vault: the vault, which is what is open now, and what
 * became of the file it replaced, by name. */
export interface Adopted {
	database: Database;
	/** The file it replaced opened with the backup's password, and is the
	 * newest snapshot under this name. */
	keptAs: string | null;
	/** The file it replaced would not open with the backup's password, and is
	 * kept beside the vault under this name, which nothing removes. */
	setAside: string | null;
}

/** A snapshot Coffer took before one of its own saves. */
export interface Snapshot {
	name: string;
	/** Which slot it sits in, counting from 1 for the most recent. */
	index: number;
	taken: string | null;
}

export interface Field {
	/** The name the file holds, and the name a reveal asks for. */
	name: string;
	/**
	 * Whether the database keeps this value protected: the field's lock. An
	 * edit sends it back as `protect`, which decides only how a field the entry
	 * does not have yet is made; a field it has keeps its own protection, and
	 * only `setProtection` changes it.
	 */
	protected: boolean;
	/** Which of an entry's fields this is. Everything the format does not name
	 * is a custom field, whatever it is called. */
	kind: 'title' | 'username' | 'password' | 'url' | 'notes' | 'custom';
	/**
	 * The value, or `null` when it does not cross: the database protects it, or
	 * it is the password. `empty` says whether there is anything to reveal.
	 */
	value: string | null;
	empty: boolean;
	/** True only for a url field Coffer is willing to hand to the system. */
	openable: boolean;
	/**
	 * Whether the value has a line break in it: one fact about a value that
	 * does not cross, the way `empty` is. A new value for one in lines - ten
	 * recovery codes - is written in lines, where Return starts the next line
	 * rather than saving the first over all ten.
	 */
	lines: boolean;
}

/**
 * Whether a value is drawn as the mask: the database protects it and there is
 * something to reveal. One rule for every row, because the title once drew a
 * protected empty title as the mask forever, there being nothing to reveal and
 * no field to type one into.
 */
export function masked(field: Field | undefined): field is Field & { value: null } {
	return field !== undefined && field.value === null && !field.empty;
}

/**
 * What deleting something does, known before anybody asks for it: it moves to
 * the recycle bin and can be put back, or it goes out of the file for good.
 * Rust answers it for every entry and every folder, from the file as it is
 * now, so a reload that brought in a vault which stopped keeping a bin is
 * answered by the next tree. A deletion sends back the one the reader was
 * shown, and Rust refuses it when it would now do the other.
 */
export type Deletion = 'bin' | 'forever';

/** What is known about something in the recycle bin. */
export interface Binned {
	/** When it went in: its own move, or that of the folder it went in with. */
	since: string | null;
	/** The id of the deleted folder it went in with, or `null` for something
	 * deleted on its own. */
	within: string | null;
	/**
	 * The id of the folder putting it back takes it to: the one it was deleted
	 * from, or the one the folder it went in with was deleted from. `null` when
	 * that is not known, has gone, or is in the bin too, and putting it back
	 * takes it to the top of the vault instead.
	 */
	from: string | null;
}

/** What a list row may know. Notes and custom fields are not here. */
export interface EntryRow {
	id: string;
	group: string;
	title: string | null;
	username: string | null;
	url: string | null;
	tags: string[];
	modified: string | null;
	hasPassword: boolean;
	attachments: number;
	/** When the entry is in the recycle bin, when it went in and where from. */
	binned: Binned | null;
	/** What deleting the entry would do, so that a list knows before a press
	 * which of several chosen entries go to the bin and which go for good. */
	deletion: Deletion;
}

export interface Group {
	id: string;
	name: string;
	isRecycleBin: boolean;
	/** The group the vault keeps its entry templates in, while anything can be
	 * made from what it holds. One at most in a tree, and none in a vault that
	 * names none. */
	isTemplates: boolean;
	/** When the folder is in the recycle bin, when it went in and where it goes
	 * back to. `null` for the bin itself and for everything outside it. */
	binned: Binned | null;
	deletion: Deletion;
	sections: Group[];
	entries: EntryRow[];
}

export interface Attachment {
	/**
	 * As the file holds it. It may be anything at all, `/` and `..` included,
	 * so nothing here builds a path out of it.
	 */
	name: string;
	size: number;
	/**
	 * What the save panel will call it. Worked out in Rust, so the name on the
	 * button is the name the reader gets.
	 */
	fileName: string;
}

/**
 * A name the entry already gives a file, offered again with another file.
 * Sizes and names: neither file's bytes are here, and the one just chosen is
 * held in Rust until the reader answers.
 */
export interface Clash {
	/** The name both files go by, exactly as the entry holds it. */
	name: string;
	/** How large the file already there is. */
	size: number;
	/** How large the file just chosen is. */
	chosen: number;
	/** What the file just chosen is called if both are kept. */
	free: string;
}

/** What came of a file the reader chose: it went on, or its name is taken and
 * nothing has happened yet. */
export type Attached = { outcome: 'added'; entry: Entry } | { outcome: 'taken'; clash: Clash };

export interface Entry {
	id: string;
	group: string;
	/** How many previous versions the entry keeps. */
	versions: number;
	fields: Field[];
	attachments: Attachment[];
	tags: string[];
	created: string | null;
	modified: string | null;
	/** When the entry is in the recycle bin, when it went in and where it goes
	 * back to. Such an entry is shown read only. */
	binned: Binned | null;
	deletion: Deletion;
}

/**
 * Whether nothing in an entry may be changed: a database Coffer will not write
 * back, or an entry in the recycle bin. What is in the bin is there to be put
 * back or let go, and an edit to it would be a change nobody sees until it
 * comes back. The pane draws it read only, and the menu bar offers nothing that
 * would change it.
 */
export function untouchable(entry: Pick<Entry, 'binned'>, readOnly: boolean): boolean {
	return readOnly || entry.binned !== null;
}

/** One previous version of an entry, as the versions block lists them. */
export interface Version {
	/**
	 * Its position in the entry's history, which is how it is addressed. Two
	 * versions written in the same second share a date, so nothing else
	 * identifies one.
	 */
	index: number;
	modified: string | null;
}

/**
 * An entry's previous versions, with the entry they are the versions of.
 *
 * The commands answer with the list alone, and a position means nothing
 * without the entry it is a position in: a list that landed under the entry
 * opened after the one it was asked about sent that entry's Restore and Delete
 * to a version the reader never saw. So `ipc.ts` pairs each answer with the
 * entry it asked about before anything else holds it, and nothing draws or
 * acts on a list under any other entry.
 */
export interface History {
	entry: string;
	/** The revision of the vault the list was read at, which every position
	 * taken from it is sent back with (see `Position`). */
	revision: number;
	/** Oldest first, as Rust keeps them. */
	versions: Version[];
}

/**
 * A version as a command names it: by its position, and the revision of the
 * vault the position was read at.
 *
 * A position is an answer about the vault as it stood. An edit adds a version,
 * a drop renumbers the rest, and a save prunes and re-sorts every entry's
 * history, and Rust answers one command at a time behind a save that holds it
 * for a key derivation - so a press made during a save reached the vault after
 * it, and dropped or restored the version that had moved into the place. Rust
 * refuses a revision the vault has moved on from with `versionsChanged`, and
 * nothing is done. The number says nothing about what the vault holds.
 */
export interface Position {
	index: number;
	revision: number;
}

/**
 * The part of a revealed value a reader selected, from one position to another,
 * counted the way the text node showing it counts: in UTF-16 code units. Two
 * numbers and nothing of the value, and Rust decides whether they name a part
 * of it at all.
 */
export interface Span {
	from: number;
	to: number;
}

/** What a command that changed the shape of the vault hands back. */
export interface Made {
	tree: Group;
	entry: string;
}

/**
 * One kind of entry "+ Entry" offers, as Rust offers it: the word to send back
 * to make one, what to call it, and the fields it writes in lines, which are
 * drawn as text areas before anything is in them. The word is Rust's and only
 * ever sent back as it came.
 */
export interface Offer {
	kind: string;
	name: string;
	lined: string[];
}

/** A name offered for a field of the reader's own, and whether a field made
 * under it is hidden. */
export interface Suggestion {
	name: string;
	protect: boolean;
}

/** What a new entry can start as, a login first, and the names offered for a
 * field of the reader's own. Rust's, and the same for every vault. */
export interface Kinds {
	offered: Offer[];
	suggested: Suggestion[];
}

/** One entry a move between folders took somewhere else, and the folder it
 * left: the top of the vault is the root's id. */
export interface Move {
	entry: string;
	from: string;
}

/** What a move of entries hands back: the tree, and each entry that changed
 * folder, which is what taking the move back sends again. */
export interface Moved {
	tree: Group;
	moved: Move[];
}

/** One entry a deletion names, with what the window showed deleting it would
 * do: Rust refuses the deletion when that is no longer what happens. */
export interface Deleting {
	entry: string;
	deletion: Deletion;
}

/** What putting a tag on entries hands back: the tree, and the entries the tag
 * went on, which are the ones that did not have it. Only those are what taking
 * it off again sends. */
export interface Tagged {
	tree: Group;
	changed: string[];
}

/** What the file on disk holds, for the dialog that asks which version to keep. */
export interface Rival {
	modified: string | null;
	/**
	 * Nothing when the file will not open with the password this window used:
	 * whoever wrote it may have changed that too.
	 */
	entries: number | null;
}

/** What removing the automatic backups that open with an old master password
 * came to. */
export interface Removed {
	gone: number;
	/** How many would not go. Each still opens with the old password, so the
	 * question about them stays. */
	left: number;
	/** Why the first of those would not go. Nothing when none is left. */
	refused: Failure | null;
}

/** What key derivation a new vault will ask for, measured on this machine. */
export interface Calibration {
	iterations: number;
	/** What that measured, in seconds. The one number the creation screen shows,
	 * and it only exists once the measuring is over. */
	seconds: number;
}

/** Which look the window is drawn in. `system` is the reader saying the Mac
 * decides; it never reaches the document, because the window resolves it into
 * one of the other two first. */
export type Theme = 'system' | 'dark' | 'light';

/** What the reader chose about locking, the clipboard and the look.
 *
 * The three `Choices` lists are what the screen may offer. They arrive with the
 * values rather than being written down here, so there is one place that
 * decides what a reader is allowed to pick. */
export interface Settings {
	/** How long an untouched vault stays open. */
	idleSeconds: number;
	/** How long a copied password stays on the clipboard. */
	clipboardSeconds: number;
	lockOnSleep: boolean;
	lockOnScreenLock: boolean;
	theme: Theme;
	idleChoices: number[];
	clipboardChoices: number[];
	themeChoices: Theme[];
}

/** The kinds of character the generator draws from. */
export type Alphabet = 'lower' | 'upper' | 'digits' | 'symbols';

/** Which generator is asking: the password's, or one under a field of the
 * reader's own. Each remembers its own recipe, so a PIN made for a card is not
 * what the password's generator opens with next. */
export type Purpose = 'password' | 'field';

/** What the generator is asked to make: its slider and its switches. */
export interface Recipe {
	length: number;
	alphabets: Alphabet[];
	/** Whether look-alike characters may appear. The switch says the opposite,
	 * "Avoid look-alikes", and is on while this is off. */
	similar: boolean;
	/** Characters to leave out, whichever kind they are. */
	avoid: string;
}

/**
 * The generator as Rust draws it for a recipe: the recipe as Rust settled it,
 * the lengths the slider runs between, and the characters two switches stand
 * for. Sent rather than written down here, so the screen never offers a length
 * or names a character the engine does not mean.
 */
export interface Generator {
	recipe: Recipe;
	shortest: number;
	longest: number;
	/** Digits and nothing else: the slider is a PIN's. */
	pin: boolean;
	symbols: string;
	lookAlikes: string;
}

/**
 * A password made. `value` goes into the node that shows it and nowhere else;
 * `missing` is the kinds that were asked for and happen not to be in it.
 */
export interface Generated {
	value: string;
	missing: Alphabet[];
	generator: Generator;
}

/** Everything a command can fail with. The message is already the sentence the
 * screen shows; the code is what the screen branches on. */
export interface Failure {
	code:
		| 'wrongCredentials'
		| 'notADatabase'
		| 'unsupportedFormat'
		| 'damaged'
		| 'heldByAnother'
		| 'externalChange'
		| 'attachmentInHistory'
		| 'versionsChanged'
		| 'forGood'
		| 'superseded'
		| 'deletionChanged'
		| 'needsOpening'
		| 'taken'
		| 'readOnly'
		| 'gone'
		| 'tooLarge'
		| 'noVault'
		| 'noSuchEntry'
		| 'refused'
		| 'io'
		| 'other';
	message: string;
}

/**
 * Coffer's own items of the menu bar, in the bar's order, by the word Rust and
 * the page both know them by. `menu.rs` draws them, and `shortcuts.test.ts`
 * reads it to keep this the same list.
 */
export const COMMANDS = [
	'settings',
	'lock',
	'newEntry',
	'newFolder',
	'duplicate',
	'openVault',
	'saveCopy',
	'showInFinder',
	'find',
	'copyLogin',
	'copyPassword',
	'moveToBin',
	'shortcuts'
] as const;

export type Command = (typeof COMMANDS)[number];

/** What Rust tells the window the reader chose outside it: an item of the menu
 * bar, the window's own close button, or an item of a menu under the pointer
 * with the number the window gave that menu. */
export type Action =
	| { action: 'command'; command: Command }
	| { action: 'closing' }
	| { action: 'context'; serial: number; chosen: Chosen };

/**
 * A place a menu's Move to offers, in the folder list's order: the top of the
 * vault, then every folder outside the bin, each `depth` folders down from the
 * top. The name is already set apart and cut to length the way a sentence sets
 * one apart, and `open` says whether a move there would be taken and change
 * something. Rust draws the menu from these and trusts none of them: the move
 * itself is checked again.
 */
export interface Place {
	id: string;
	name: string;
	open: boolean;
	depth: number;
}

/**
 * What a right-click was on, as the window tells Rust: ids, names, whether a
 * value is on the screen, and for a revealed value the positions of the part
 * selected. Nothing of any value.
 */
export type Subject =
	| { kind: 'entry'; entry: string; places: Place[] }
	| { kind: 'entries'; entries: string[]; places: Place[] }
	| { kind: 'folder'; group: string; places: Place[] }
	| { kind: 'bin' }
	| { kind: 'field'; entry: string; field: string; shown: boolean }
	| { kind: 'file'; entry: string; name: string }
	| { kind: 'value'; entry: string; field: string; range: Span | null };

/** Where the pointer was, in the window's own points from its top-left corner. */
export interface Point {
	x: number;
	y: number;
}

/**
 * An item chosen from one of Coffer's menus under the pointer, with the ids it
 * is about. The window runs the function the item's button runs, on those
 * ids, and nothing when they are not what it shows any more.
 */
export type Chosen =
	| { item: 'copyField'; entry: string; field: string }
	| { item: 'copyValue'; entry: string; field: string; range: Span | null }
	| { item: 'showField'; entry: string; field: string }
	| { item: 'hideField'; entry: string; field: string }
	| { item: 'changeField'; entry: string; field: string }
	| { item: 'makeOne'; entry: string; field: string }
	| { item: 'removeField'; entry: string; field: string }
	| { item: 'openAddress'; entry: string }
	| { item: 'duplicate'; entry: string }
	| { item: 'moveEntries'; entries: string[]; into: string }
	| { item: 'deleteEntries'; entries: Deleting[] }
	| { item: 'putBackEntries'; entries: string[] }
	| { item: 'newEntryIn'; group: string }
	| { item: 'newFolderIn'; group: string }
	| { item: 'renameFolder'; group: string }
	| { item: 'moveFolder'; group: string; into: string }
	| { item: 'deleteFolder'; group: string; deletion: Deletion }
	| { item: 'putBackFolder'; group: string }
	| { item: 'emptyBin' }
	| { item: 'saveFile'; entry: string; name: string }
	| { item: 'removeFile'; entry: string; name: string };
