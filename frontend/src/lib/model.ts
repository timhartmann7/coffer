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
	/** Whether the last lock found text the reader was still typing and saved
	 * it into the vault with everything else. It does not say which entry: after
	 * a lock nothing of the vault is left to say it with. */
	typed: boolean;
	/** Whether some of what it saved was a new value typed in a Change field and
	 * kept in a field of its own beside the value it was for, which the field
	 * still holds. Which field is not said either. */
	typedBeside: boolean;
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
}

export interface Group {
	id: string;
	name: string;
	isRecycleBin: boolean;
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

/** What the file on disk holds, for the dialog that asks which version to keep. */
export interface Rival {
	modified: string | null;
	/**
	 * Nothing when the file will not open with the password this window used:
	 * whoever wrote it may have changed that too.
	 */
	entries: number | null;
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
	'openVault',
	'find',
	'copyLogin',
	'copyPassword',
	'moveToBin',
	'shortcuts'
] as const;

export type Command = (typeof COMMANDS)[number];

/** What Rust tells the window the reader chose outside it: an item of the menu
 * bar, or the window's own close button. */
export type Action = { action: 'command'; command: Command } | { action: 'closing' };
