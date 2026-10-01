/** What the recycle bin says about what is in it, and what a deletion asks
 * before it, in the words the window uses. */

import { ago, day, quoted } from './format';
import type { Binned, Group } from './model';
import { find } from './tree';

/**
 * What a sentence calls the top of the vault. The file gives the top group a
 * name, and the window never shows it: the folders pane calls everything in
 * the vault "All entries", so a sentence that named the top group would be
 * naming a folder the reader has never seen.
 */
const TOP = 'the top of the vault';

/** A folder of the tree as a sentence names it, or `null` when the tree does
 * not hold it. */
function named(root: Group, id: string): string | null {
	const folder = find(root, id);
	return folder ? quoted(folder.name) : null;
}

/**
 * The folder something goes back to, as a sentence names it, or `null` when
 * nothing says where it came from.
 *
 * Rust sends the folder's id and the tree gives its name, so a folder renamed
 * since the deletion is called what it is called now.
 */
function whence(binned: Binned, root: Group): string | null {
	if (binned.from === null) return null;
	if (binned.from === root.id) return TOP;
	return named(root, binned.from);
}

/**
 * The deleted folder something went into the bin with, as a sentence names
 * it, or `null` for something deleted on its own.
 *
 * Said instead of where it came from. The deletion moved the folder and not
 * what was in it, so the folder is the one thing the reader deleted, and the
 * place it goes back to is the folder's.
 */
function companion(binned: Binned, root: Group): string | null {
	return binned.within === null ? null : named(root, binned.within);
}

/**
 * The sentence over an entry or a folder opened in the bin: since when, and
 * where Put back takes it.
 *
 * "Was in" only when the bin knows. A thing another client binned without
 * saying where from, or whose folder has gone or is in the bin too, is not
 * given a past it did not have: the sentence says where it is going instead,
 * and so does one that went in with a folder, which was in that folder.
 */
export function standing(binned: Binned, root: Group, now: Date): string {
	const since = day(binned.since, now);
	const went = since === '' ? 'In the Recycle Bin' : `In the Recycle Bin since ${since}`;
	const from = whence(binned, root);
	const goes = `goes back to ${from ?? TOP}`;
	const along = companion(binned, root);
	if (along !== null) return `${went} · deleted with ${along} · ${goes}`;
	if (from === null) return `${went} · ${goes}`;
	return `${went} · was ${from === TOP ? 'at' : 'in'} ${from}`;
}

/** The line under a row in the bin: when it was deleted, and with which folder
 * or from where, when that is known. */
export function deleted(binned: Binned, root: Group, now: Date): string {
	const when = ago(binned.since, now);
	const along = companion(binned, root);
	const from = whence(binned, root);
	const tail = along !== null ? `with ${along}` : from !== null ? `from ${from}` : null;
	if (when === '') return tail === null ? 'Deleted' : `Deleted ${tail}`;
	return tail === null ? `Deleted ${when}` : `Deleted ${when} · ${tail}`;
}

/**
 * The question before something goes out of the file for good: an entry, a
 * folder, or a folder in the bin, asked from wherever it was pressed. `name` is
 * what a sentence calls it, quotes included. A folder takes what is in it along,
 * and says so.
 */
export function eraseQuestion(name: string, folder = false): string {
	return `Delete ${name}${folder ? ' and everything in it' : ''} forever? This can’t be undone.`;
}

/** What a move into the bin says, for an entry and for a folder alike. */
export function moved(name: string): string {
	return `Moved ${name} to the Recycle Bin`;
}

/** What is said once something has gone out of the file, where nothing can
 * put it back. */
export function erased(name: string): string {
	return `Deleted ${name} forever`;
}
