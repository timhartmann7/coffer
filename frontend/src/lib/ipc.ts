/**
 * The only module that knows Coffer is a Tauri application.
 *
 * Everything the screens know about the database comes through here, and the
 * one value that is a secret - a revealed field - is handed straight to the DOM
 * node that shows it and is never kept.
 */

import { invoke } from '@tauri-apps/api/core';
import type { Database, Entry, Failure, Group, Snapshot, Status } from './model';

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

export function lock(): Promise<void> {
	return invoke('lock');
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
