/**
 * Putting a tag on several entries at once, and taking it off again.
 *
 * One call, one save and one notice for the lot. A tag is an edit, so every
 * entry it goes on keeps its previous state as a version - and only the ones
 * that did not have it yet: those are what Rust answers with, and what the
 * undo takes it off again, so an entry that had the tag before keeps it.
 */

import { counted, quoted } from './format';
import { tagEntries, untagEntries } from './ipc';
import type { EntryRow, Group } from './model';
import type { Notices } from './notices.svelte';
import { overtaken } from './overtaken';
import { rowsOf } from './tree';

/** The vault screen, as a tag needs it. Read through functions, because each
 * can have changed by the time Rust answers. */
export interface Screen {
	/** The entry in the pane, read or not. */
	showing(): string | null;
	/** Whether the last save failed. */
	unsaved(): boolean;
	/** Takes every value of the entry revealed on the screen off it. */
	conceal(id: string): void;
	/** Takes the versions of `id` off the screen until they are read again. */
	unread(id: string): void;
	/** Reads the entry in the pane again, if it is still `id`. */
	read(id: string): Promise<void>;
	/** Reads the tree again and draws it, writing nothing. */
	redraw(): Promise<void>;
	/** Draws the tree a change came back with, and writes it to the file. */
	reshaped(tree: Group): Promise<void>;
	/** Says why something was refused. */
	failed(thrown: unknown): void;
	notices: Notices;
}

/**
 * Draws a tree a tag came back with. The entry in the pane, when the tag went
 * on or came off it, is read again first, its revealed values and its
 * versions taken off the screen the way every change to an entry takes them:
 * the chip on it, and the version the tag wrote, are part of what it is now.
 */
async function landed(screen: Screen, changed: readonly string[], tree: Group): Promise<void> {
	const showing = screen.showing();
	if (showing !== null && changed.includes(showing)) {
		screen.conceal(showing);
		screen.unread(showing);
		await screen.read(showing);
	}
	await screen.reshaped(tree);
}

/**
 * Puts a tag on the rows, as it was typed less the space around it, and offers
 * to take it off again.
 *
 * Nothing typed is no tag. A tag every one of them has already writes nothing,
 * and the notice says so rather than offering to undo nothing. A tag the file
 * would split or trim is Rust's to refuse, with its own sentence, as on one
 * entry's chip. The undo is offered only once the tag is in the file.
 */
export async function tag(screen: Screen, rows: readonly EntryRow[], typed: string): Promise<void> {
	const named = typed.trim();
	if (named === '' || rows.length === 0) return;

	let changed: string[];
	let tree: Group;
	try {
		({ changed, tree } = await tagEntries(
			rows.map((row) => row.id),
			named
		));
	} catch (thrown) {
		screen.failed(thrown);
		return;
	}
	if (changed.length === 0) {
		screen.notices.tell({
			message: `${counted(rows)} already ${rows.length === 1 ? 'has' : 'have'} ${quoted(named)}`,
			kind: 'changed'
		});
		return;
	}

	await landed(screen, changed, tree);
	if (screen.unsaved()) return;
	screen.notices.offer(
		`Added ${quoted(named)} to ${counted(rowsOf(tree, changed))}`,
		async () => {
			let back: Group;
			try {
				back = await untagEntries(changed, named);
			} catch (thrown) {
				// One of them gone out of the file since.
				await overtaken(thrown, screen, ['noSuchEntry']);
				return;
			}
			await landed(screen, changed, back);
		},
		'changed'
	);
}
