/**
 * The only module that knows Coffer is a Tauri application.
 *
 * Everything the screens know about the database comes through here, and the
 * one value that is a secret - a revealed field - is handed straight to the DOM
 * node that shows it and is never kept.
 */

import { invoke } from '@tauri-apps/api/core';
import type {
	Alphabet,
	Calibration,
	Database,
	Entry,
	Failure,
	Group,
	Made,
	Rival,
	Settings,
	Snapshot,
	Status,
	Version
} from './model';

export function status(): Promise<Status> {
	return invoke('status');
}

/** Opens the system's file picker. `null` when the user closed it. */
export function chooseDatabase(): Promise<Database | null> {
	return invoke('choose_database');
}

/**
 * Sends the master password to Rust as bytes.
 *
 * The bytes are the whole body of the message: not a JSON string, not a field
 * of an object, and nothing that a JavaScript string has to hold on the way.
 * The buffer is wiped once the call has finished with it.
 */
export async function unlock(password: Uint8Array): Promise<void> {
	try {
		await invoke('unlock', password);
	} finally {
		password.fill(0);
	}
}

/**
 * Opens the vault even though somebody's lock file sits beside it.
 *
 * Only ever after `unlock` rejected with `heldByAnother` and the reader was
 * shown who holds it. A lock left by a Mac that lost power cannot be told from
 * one held right now, so the answer is a person's rather than Coffer's.
 */
export async function unlockTakingOver(password: Uint8Array): Promise<void> {
	try {
		await invoke('unlock_over', password);
	} finally {
		password.fill(0);
	}
}

/** Opens the system's file picker for the key file some databases need beside
 * the password, and keeps it for the next unlock. `null` when the reader closed
 * the picker. */
export function chooseKeyFile(): Promise<Database | null> {
	return invoke('choose_key_file');
}

/** Takes the key file back off, for a reader who picked the wrong one. */
export function forgetKeyFile(): Promise<void> {
	return invoke('forget_key_file');
}

/** Where a vault goes when the reader has not said. Chosen without a panel, so
 * that making the first one is a password and nothing else. */
export function defaultNewDatabase(): Promise<Database> {
	return invoke('default_new_database');
}

/** Opens the system's save panel and keeps where the reader wants the new vault.
 * `null` when they closed it. */
export function chooseNewDatabase(): Promise<Database | null> {
	return invoke('choose_new_database');
}

/**
 * Measures what a one-second unlock costs on this machine.
 *
 * Several seconds of work with every core busy, and the number it answers with
 * is the only one the screen has to show: it does not exist until the measuring
 * is over.
 */
export function calibrate(): Promise<Calibration> {
	return invoke('calibrate');
}

/**
 * Makes the chosen file into a vault and opens it.
 *
 * The password goes the same way an unlock's does - the whole body of the
 * message, as bytes - which is why where the vault goes and what it costs to
 * open were settled by the two calls above.
 */
export async function createDatabase(password: Uint8Array): Promise<void> {
	try {
		await invoke('create_database', password);
	} finally {
		password.fill(0);
	}
}

export function lock(): Promise<void> {
	return invoke('lock');
}

/**
 * Says the reader is there, so that the idle timer starts again.
 *
 * Sent on real input and no more than a few times a minute. What comes back is
 * the seconds the open vault has left, which nothing draws: a status bar that
 * asked once a second would be an idle timer resetting itself, and a vault that
 * never locks.
 */
export function stirred(): Promise<number | null> {
	return invoke('stirred');
}

/** What the reader chose, and the values they may choose instead. Both come
 * from Rust: a screen holding its own list would be a second place the answer
 * lives. */
export function settings(): Promise<Settings> {
	return invoke('settings');
}

/**
 * Puts a choice into effect.
 *
 * Answers with what was actually stored, which is not always what was sent: a
 * value Rust does not offer is settled onto one it does. The screen draws what
 * came back.
 */
export function setSettings(settings: Settings): Promise<Settings> {
	return invoke('set_settings', { settings });
}

export function tree(): Promise<Group> {
	return invoke('tree');
}

export function entry(id: string): Promise<Entry> {
	return invoke('entry', { id });
}

/**
 * One field of one entry, once. The caller writes what comes back straight into
 * the DOM: it does not belong in a variable, a store or an attribute.
 */
export function reveal(entry: string, field: string): Promise<string> {
	return invoke('reveal', { entry, field });
}

/**
 * Copies a field to the clipboard without the value passing through here.
 * Answers with the number of seconds until Coffer takes it off again.
 */
export function copy(entry: string, field: string): Promise<number> {
	return invoke('copy', { entry, field });
}

/** Opens an entry's address, if Coffer opens addresses of that kind. */
export function openUrl(entry: string): Promise<void> {
	return invoke('open_url', { entry });
}

/** The snapshots beside the chosen database, most recent first. */
export function snapshots(): Promise<Snapshot[]> {
	return invoke('snapshots');
}

/** Points Coffer at one of those snapshots instead. */
export function chooseSnapshot(index: number): Promise<Database> {
	return invoke('choose_snapshot', { index });
}

/** Points the session at the unsaved copy a lock left beside the vault. It takes
 * nothing: the path is built in Rust from the database the reader chose, so
 * nothing this window sends can name a file. */
export function chooseRescue(): Promise<Database> {
	return invoke('choose_rescue');
}

/** Takes that copy off the disk, at the reader's word. Coffer never removes it
 * on its own: it holds the one version of work the vault has not got. */
export function discardRescue(): Promise<void> {
	return invoke('discard_rescue');
}

/** Makes an entry in a folder and answers with the tree it changed. */
export function createEntry(group: string): Promise<Made> {
	return invoke('create_entry', { group });
}

export function deleteEntry(entry: string): Promise<Group> {
	return invoke('delete_entry', { entry });
}

export function createGroup(parent: string, name: string): Promise<Group> {
	return invoke('create_group', { parent, name });
}

export function renameGroup(group: string, name: string): Promise<Group> {
	return invoke('rename_group', { group, name });
}

export function deleteGroup(group: string): Promise<Group> {
	return invoke('delete_group', { group });
}

export function emptyRecycleBin(): Promise<Group> {
	return invoke('empty_recycle_bin');
}

/**
 * Writes one field.
 *
 * `protect` is what the screen read off the field it is editing. Sending it
 * back is what keeps a value the database protects from being written into the
 * file as plain text.
 */
export function setField(
	entry: string,
	field: string,
	value: string,
	protect: boolean
): Promise<Entry> {
	return invoke('set_field', { entry, field, value, protect });
}

export function removeField(entry: string, field: string): Promise<Entry> {
	return invoke('remove_field', { entry, field });
}

export function setTags(entry: string, tags: string[]): Promise<Entry> {
	return invoke('set_tags', { entry, tags });
}

/** Opens the system's file picker and puts what it chose on the entry. The
 * bytes are read in Rust and never come near here. */
/** Nothing comes back when the reader closed the panel without choosing a file. */
export function addAttachment(entry: string): Promise<Entry | null> {
	return invoke('add_attachment', { entry });
}

/** Opens the system's save panel and writes the file out from Rust. */
export function exportAttachment(entry: string, name: string): Promise<void> {
	return invoke('export_attachment', { entry, name });
}

export function removeAttachment(entry: string, name: string): Promise<Entry> {
	return invoke('remove_attachment', { entry, name });
}

/**
 * Takes a file off along with the previous versions that are holding it back.
 *
 * One call rather than two: the versions go only if the file then goes, so a
 * reader who asked to be rid of a file is not left having lost the history and
 * still having the file.
 */
export function removeAttachmentAndVersions(entry: string, name: string): Promise<Entry> {
	return invoke('remove_attachment_and_versions', { entry, name });
}

export function versions(entry: string): Promise<Version[]> {
	return invoke('versions', { entry });
}

/** One previous version, read the way an entry is read. */
export function version(entry: string, index: number): Promise<Entry> {
	return invoke('version', { entry, index });
}

/** One field of one version, once - the same terms as a reveal. */
export function revealVersion(entry: string, index: number, field: string): Promise<string> {
	return invoke('reveal_version', { entry, index, field });
}

export function restoreVersion(entry: string, index: number): Promise<Entry> {
	return invoke('restore_version', { entry, index });
}

export function deleteVersion(entry: string, index: number): Promise<Version[]> {
	return invoke('delete_version', { entry, index });
}

export function clearHistory(entry: string): Promise<Version[]> {
	return invoke('clear_history', { entry });
}

/**
 * Makes a password.
 *
 * It comes back the way a revealed value does, and it is treated the same way:
 * it goes into the node that shows it and into the field it was made for, and
 * it is not kept anywhere else.
 */
export function generatePassword(
	length: number,
	alphabets: Alphabet[],
	similar: boolean
): Promise<string> {
	return invoke('generate_password', { length, alphabets, similar });
}

/** Writes the database back. Rejects with `externalChange` when the file is not
 * the one the window opened. */
export function save(): Promise<void> {
	return invoke('save');
}

/** Writes over a file somebody else changed. What was there goes into the first
 * snapshot on the way. */
export function saveOver(): Promise<void> {
	return invoke('save_over');
}

/** Writes what is in the window to a file of its own. */
export function saveCopy(): Promise<Database | null> {
	return invoke('save_copy');
}

/** Throws away what is in the window and reads the file again. */
export function reload(): Promise<Group> {
	return invoke('reload');
}

/** What the file on disk holds. Reading it means decrypting it, so it is asked
 * for when there is a decision to make and never on a timer. */
export function rival(): Promise<Rival> {
	return invoke('rival');
}

/**
 * What a rejected command said.
 *
 * A command rejects with the value Rust serialised, which is a plain object and
 * never an `Error`, so `instanceof` and `.message` are both useless on it.
 */
export function asFailure(thrown: unknown): Failure {
	const failed = thrown as Partial<Failure> | null;
	if (failed && typeof failed.message === 'string' && typeof failed.code === 'string') {
		return failed as Failure;
	}
	return { code: 'other', message: 'Coffer could not finish that.' };
}
