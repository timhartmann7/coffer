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

/** Where a new vault would go, as the creation screen draws it. */
export interface Target extends Database {
	/** The path the way its owner would write it: under the home folder it
	 * starts with `~`. For showing, never for opening. */
	shown: string;
	/** Whether something already sits at the name. Nothing is ever made over
	 * one, so the screen says so before anybody types a password. */
	taken: boolean;
}

/** A vault sitting in Coffer's own folder, offered when nothing is remembered.
 * The path does not cross: opening it asks Rust to find it again. */
export interface Found {
	/** The file name, which is what the offer says. */
	name: string;
	/** The folder in the home folder it was found in. */
	folder: string;
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
	 * Whether the database keeps this value protected. It goes back with an
	 * edit: rewriting a protected field as an open one would put a password
	 * into the file as plain text.
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
}

export interface Group {
	id: string;
	name: string;
	isRecycleBin: boolean;
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
