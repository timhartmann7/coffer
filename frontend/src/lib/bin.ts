/** What the recycle bin says about what is in it, in the words the window uses. */

import { ago, day } from './format';
import type { Binned, Group } from './model';
import { find } from './tree';

/**
 * What a sentence calls the top of the vault. The file gives the top group a
 * name, and the window never shows it: the folders pane calls everything in
 * the vault "All entries", so a sentence that named the top group would be
 * naming a folder the reader has never seen.
 */
const TOP = 'the top of the vault';

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
	const folder = find(root, binned.from);
	return folder ? `“${folder.name}”` : null;
}

/**
 * The sentence over an entry or a folder opened in the bin: since when, and
 * where Put back takes it.
 *
 * "Was in" only when the bin knows. A thing another client binned without
 * saying where from, or whose folder has gone or is in the bin too, is not
 * given a past it did not have: the sentence says where it is going instead.
 */
export function standing(binned: Binned, root: Group, now: Date): string {
	const since = day(binned.since, now);
	const went = since === '' ? 'In the Recycle Bin' : `In the Recycle Bin since ${since}`;
	const from = whence(binned, root);
	if (from === null) return `${went} · goes back to ${TOP}`;
	return `${went} · was ${from === TOP ? 'at' : 'in'} ${from}`;
}

/** The line under a row in the bin: when it was deleted, and where from when
 * that is known. */
export function deleted(binned: Binned, root: Group, now: Date): string {
	const when = ago(binned.since, now);
	const from = whence(binned, root);
	if (when === '') return from === null ? 'Deleted' : `Deleted from ${from}`;
	return from === null ? `Deleted ${when}` : `Deleted ${when} · from ${from}`;
}
