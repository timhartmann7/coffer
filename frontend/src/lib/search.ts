/**
 * The instant filter over the entry list.
 *
 * It runs on what the frontend already has, which is title, username, URL and
 * tags. Notes and custom field values are not searched because they never leave
 * `vault-core`, and the empty-search screen says so rather than letting the
 * user conclude their note is gone.
 */

import type { EntryRow } from './model';

/** A row with the text a search runs against, folded once when the list
 * arrives rather than on every keystroke. */
export interface Indexed {
	row: EntryRow;
	haystack: string;
}

/**
 * Case and composition are folded the same way on both sides, so that a title
 * written with a combining accent is found by a query written with a composed
 * one. A protected field contributes nothing: its value is not here to search.
 */
function fold(text: string): string {
	return text.normalize('NFC').toLowerCase();
}

export function index(rows: EntryRow[]): Indexed[] {
	return rows.map((row) => ({
		row,
		haystack: fold([row.title ?? '', row.username ?? '', row.url ?? '', ...row.tags].join('\n'))
	}));
}

export function search(rows: Indexed[], query: string): EntryRow[] {
	if (query === '') return rows.map((indexed) => indexed.row);
	const needle = fold(query);
	return rows.filter((indexed) => indexed.haystack.includes(needle)).map((indexed) => indexed.row);
}
