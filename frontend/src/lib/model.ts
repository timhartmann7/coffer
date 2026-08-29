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

export interface Status {
	database: Database | null;
	unlocked: boolean;
}

/** A snapshot Coffer took before one of its own saves. */
export interface Snapshot {
	path: string;
	name: string;
	/** Which slot it sits in, counting from 1 for the most recent. */
	index: number;
	taken: string | null;
}

export interface Field {
	/** The name the file holds, and the name a reveal asks for. */
	name: string;
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

interface Attachment {
	name: string;
	size: number;
}

export interface Entry {
	id: string;
	group: string;
	fields: Field[];
	attachments: Attachment[];
	tags: string[];
	created: string | null;
	modified: string | null;
	expires: string | null;
	versions: number;
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
		| 'gone'
		| 'tooLarge'
		| 'noVault'
		| 'noSuchEntry'
		| 'refused'
		| 'io'
		| 'other';
	message: string;
}
