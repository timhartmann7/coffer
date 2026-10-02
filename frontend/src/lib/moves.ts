/**
 * Moving entries and folders between folders, into the recycle bin, out of it
 * and out of the file, and saying what became of each.
 */

import { erased, moved } from './bin';
import { release } from './drafts';
import { called, calledRow, quoted } from './format';
import {
	asFailure,
	deleteEntry,
	deleteGroup,
	emptyRecycleBin,
	moveEntries,
	moveEntriesBack,
	moveGroup,
	moveGroupBack,
	putBackEntry,
	putBackGroup,
	tree as loadTree
} from './ipc';
import type { Deletion, Entry, EntryRow, Group, Moved } from './model';
import type { Notices } from './notices.svelte';
import { Once } from './once';
import { movedTo, placeOf } from './places';
import { find, pathTo, rowOf } from './tree';

/** What an undo says when Rust finds the file has moved on since the move. */
const MOVED_SINCE = 'Something has moved since, so that can no longer be undone.';

/** The vault screen, as a move needs it. Read through functions, because
 * every one of them can have changed by the time Rust answers. */
export interface Screen {
	/** The tree as the screen last drew it. */
	root(): Group;
	/** The entry in the pane, read or not. */
	showing(): string | null;
	/** Whether the last save failed. */
	unsaved(): boolean;
	/** Puts the pane away. */
	close(): void;
	/** Shows a folder, or everything for `null`. */
	select(id: string | null): void;
	/** Reads the tree again and draws it, writing nothing. */
	redraw(): Promise<void>;
	/** Puts an entry in the pane and reads it. */
	open(row: EntryRow): Promise<void>;
	/** Reads the entry in the pane again, if it is still `id`. */
	read(id: string): Promise<void>;
	/** Takes the versions of `id` off the screen until they are read again. */
	unread(id: string): void;
	/** Opens a folder in the tree. */
	expand(id: string): void;
	/** Draws the tree a change came back with, and writes it to the file. */
	reshaped(tree: Group): Promise<void>;
	/** Says why something was refused. */
	failed(thrown: unknown): void;
	notices: Notices;
}

export class Moves {
	readonly #screen: Screen;

	/**
	 * The entries and folders a move has been asked for and Rust has not yet
	 * answered about: between folders, into the bin, out of it or out of the
	 * file.
	 *
	 * The button that asked stays on the screen until Rust answers, and a
	 * second press of "Move to Recycle Bin" in that time would find the entry
	 * already in the bin. Rust refuses that rather than erase it, since every
	 * deletion carries the one the reader was shown, but the reader asked for
	 * nothing new and is owed no refusal. So a press is dropped while the same
	 * thing is on its way, and only then: one on another entry or folder is a
	 * choice of its own. The save that follows a move is not part of it - by
	 * then the screen has moved on from the button that asked.
	 */
	readonly #once = new Once();

	constructor(screen: Screen) {
		this.#screen = screen;
	}

	/**
	 * Whether a folder, or one above it, is on its way somewhere.
	 *
	 * What is inside such a folder goes with it, and a move or a deletion of
	 * its own may reach Rust after the folder's and be refused, for a choice
	 * the folder's move has already made for it.
	 */
	#inFlight(root: Group, group: string): boolean {
		return pathTo(root, group)?.some((step) => this.#once.busy(step.id)) ?? false;
	}

	/**
	 * Reads the entry in the pane again when it is one of `ids`: the line above
	 * its title names the folder it is in.
	 *
	 * Its versions go first. A move is a change like any other to the
	 * positions a list was read at, and the list from before it would stay
	 * pressable for the whole save, every press refused as though the versions
	 * had changed.
	 */
	async #reread(ids: readonly string[]): Promise<void> {
		const showing = this.#screen.showing();
		if (showing === null || !ids.includes(showing)) return;
		this.#screen.unread(showing);
		await this.#screen.read(showing);
	}

	/**
	 * Sends the undo of a move, unless something it is about is on its way:
	 * `null` is a press that was dropped. What it is about is the things that
	 * moved, the folder they are in and the folders they go back to. A move of
	 * the same thing still on its way is the reader's later choice, and the
	 * undo and that move would reach Rust, and answer, in no fixed order.
	 *
	 * Rust's refusal of a move the file says has been moved on from is said
	 * in the window's own words.
	 */
	async #takeBack(
		things: readonly string[],
		folders: readonly string[],
		back: () => Promise<Group>
	): Promise<Group | null> {
		const root = this.#screen.root();
		if (folders.some((folder) => this.#inFlight(root, folder))) return null;
		try {
			return await this.#once.run(things, back);
		} catch (thrown) {
			if (asFailure(thrown).code !== 'superseded') throw thrown;
			this.#screen.notices.warn(MOVED_SINCE);
			return null;
		}
	}

	/**
	 * Says why a deletion failed. Rust refuses one that would no longer do what
	 * the window showed: two deletions behind one save reach it in either order,
	 * and a folder that went into the bin first makes the move of something in
	 * it an erasure. Nothing was deleted, so the reader is told plainly, and the
	 * tree and the pane are read again to show what deleting each does now.
	 */
	async #refused(thrown: unknown): Promise<void> {
		const screen = this.#screen;
		if (asFailure(thrown).code !== 'deletionChanged') {
			screen.failed(thrown);
			return;
		}
		screen.notices.warn(
			'That changed while you were choosing, so nothing was deleted. Choose again from the vault as it is now.'
		);
		await screen.redraw();
		const showing = screen.showing();
		if (showing !== null) await screen.read(showing);
	}

	/** Says that something went out of the file, where nothing can put it back. */
	#erased(name: string): void {
		this.#screen.notices.tell({ message: erased(name), kind: 'removed' });
	}

	/**
	 * Deletes an entry, and says what became of it.
	 *
	 * What became of it is read off the tree that comes back, not off what the
	 * pane expected: an entry still in the file went to the bin and is offered
	 * back, and one that is not went for good. A move whose save failed has a
	 * notice of its own already, and an offer over it would push the one
	 * sentence that matters off the screen.
	 *
	 * Rust may answer a second later, behind a save, and a reader who opened
	 * another entry in that second is reading it: the pane is put away only if
	 * it is still on the entry that went. The move is offered back either way.
	 * Its undo puts the entry back by its id, and opens it again only into a
	 * pane that is still empty, never over whatever the reader chose instead.
	 */
	async removeEntry(entry: Entry): Promise<void> {
		const screen = this.#screen;
		if (this.#inFlight(screen.root(), entry.group)) return;
		const id = entry.id;
		const name = called(entry);
		let tree: Group | null;
		try {
			tree = await this.#once.run([id], () => deleteEntry(id, entry.deletion, release(id)));
		} catch (thrown) {
			await this.#refused(thrown);
			return;
		}
		if (tree === null) return;
		if (screen.showing() === id) screen.close();
		await screen.reshaped(tree);
		if (screen.unsaved()) return;

		if (rowOf(tree, id) === null) {
			this.#erased(name);
			return;
		}
		screen.notices.offer(moved(name), async () => {
			const back = await putBackEntry(id);
			await screen.reshaped(back);
			const row = rowOf(back, id);
			// An entry the reader opened since, or during the save, is their
			// later choice.
			if (row && screen.showing() === null) await screen.open(row);
		});
	}

	/**
	 * Moves entries into a folder, or to the top of the vault, and offers them
	 * back. Every way of moving an entry comes here: the line above its title,
	 * a drag, and whatever chooses more than one.
	 *
	 * Read off the tree as it is drawn: an entry already there is left out, and
	 * a press is dropped while any of them, the folder one of them is in, or the
	 * folder they go into is on its way somewhere - a folder that went into the
	 * bin first would make Rust refuse a move the reader made into it. The pane
	 * stays on the entry it shows, wherever that entry goes, and reads it again
	 * before the save so the line above its title names the new folder.
	 *
	 * Offered back only once the move is in the file. The undo is one call,
	 * every entry back to the folder it left or none of them, and Rust refuses
	 * it once the file says any of them has moved since: the undo acts by id
	 * and opens nothing.
	 */
	async moveEntries(ids: readonly string[], into: string): Promise<void> {
		const screen = this.#screen;
		const root = screen.root();
		const rows = ids
			.map((id) => rowOf(root, id))
			.filter((row): row is EntryRow => row !== null && row.group !== into);
		if (rows.length === 0) return;
		if (this.#inFlight(root, into) || rows.some((row) => this.#inFlight(root, row.group))) return;
		const moving = rows.map((row) => row.id);

		let answer: Moved | null;
		try {
			answer = await this.#once.run(moving, () => moveEntries(moving, into));
		} catch (thrown) {
			screen.failed(thrown);
			return;
		}
		if (answer === null) return;
		const { tree, moved } = answer;
		if (moved.length === 0) {
			// Every one of them was there already: the tree on the screen was
			// behind, and there is nothing to write or to offer back.
			await screen.redraw();
			return;
		}
		await this.#reread(moving);
		await screen.reshaped(tree);
		if (screen.unsaved()) return;
		// Said of what Rust moved, which is what the undo takes back.
		const one = moved.length === 1 ? rowOf(tree, moved[0].entry) : null;
		const what = one ? calledRow(one) : `${moved.length} entries`;
		const place = placeOf(tree, into);
		// A tree that no longer holds the folder has nothing true to say.
		if (place === null) return;

		const back = moved.map((each) => each.entry);
		const folders = [into, ...new Set(moved.map((each) => each.from))];
		screen.notices.offer(
			movedTo(what, place),
			async () => {
				const tree = await this.#takeBack(back, folders, () => moveEntriesBack(moved, into));
				if (tree === null) return;
				await this.#reread(back);
				await screen.reshaped(tree);
			},
			'moved'
		);
	}

	/**
	 * Moves a folder with everything in it, opens the folders above it where it
	 * landed, and offers it back to the folder it came from.
	 *
	 * The undo is Rust's, as an entry's is: it moves the folder back only while
	 * the file says it is where this move put it, having come from where this
	 * move took it. One moved on since is the reader's later choice.
	 */
	async moveFolder(id: string, into: string): Promise<void> {
		const screen = this.#screen;
		const root = screen.root();
		const path = pathTo(root, id);
		const folder = path?.at(-1);
		const from = path?.at(-2)?.id;
		if (folder === undefined || from === undefined || from === into) return;
		if (this.#inFlight(root, id) || this.#inFlight(root, into)) return;
		const name = quoted(folder.name);

		let tree: Group | null;
		try {
			tree = await this.#once.run([id], () => moveGroup(id, into));
		} catch (thrown) {
			screen.failed(thrown);
			return;
		}
		if (tree === null) return;
		for (const step of pathTo(tree, id)?.slice(0, -1) ?? []) screen.expand(step.id);
		await screen.reshaped(tree);
		if (screen.unsaved()) return;
		const place = placeOf(tree, into);
		if (place === null) return;

		screen.notices.offer(
			movedTo(name, place),
			async () => {
				const back = await this.#takeBack([id], [id, from], () => moveGroupBack(id, from, into));
				if (back !== null) await screen.reshaped(back);
			},
			'moved'
		);
	}

	/**
	 * Takes an entry out of the bin. The pane stays on it and reads it again:
	 * back out of the bin it is an entry like any other, and the line above its
	 * title says where it went.
	 *
	 * Read again before the save rather than after it, so the pane does not go
	 * on offering Put back, and a deletion for good, on an entry that has
	 * already left the bin for the length of the save.
	 */
	async putBackEntry(id: string): Promise<void> {
		const screen = this.#screen;
		try {
			const tree = await this.#once.run([id], () => putBackEntry(id));
			if (tree === null) return;
			screen.unread(id);
			await screen.read(id);
			await screen.reshaped(tree);
		} catch (thrown) {
			screen.failed(thrown);
		}
	}

	/** Deletes a folder, with `name` as a sentence calls it and `deletion` as
	 * the question before it said it would go, and offers it back when it went
	 * to the bin. */
	async removeFolder(id: string, name: string, deletion: Deletion): Promise<void> {
		const screen = this.#screen;
		try {
			const tree = await this.#once.run([id], () => deleteGroup(id, deletion));
			if (tree === null) return;
			screen.select(null);
			await screen.reshaped(tree);
			if (screen.unsaved()) return;
			if (find(tree, id) === null) {
				this.#erased(name);
				return;
			}
			screen.notices.offer(moved(name), async () => {
				await screen.reshaped(await putBackGroup(id));
			});
		} catch (thrown) {
			await this.#refused(thrown);
		}
	}

	/**
	 * Takes a folder out of the bin with everything in it.
	 *
	 * The list stays on it, and the folders above it are opened in the tree, so
	 * that the place it went back to is on the screen rather than folded away.
	 */
	async putBackFolder(id: string): Promise<void> {
		const screen = this.#screen;
		// An entry open inside the folder went back with it, and is drawn read
		// only with a banner about a bin it has left until it is read again.
		const reading = screen.showing();
		try {
			const tree = await this.#once.run([id], () => putBackGroup(id));
			if (tree === null) return;
			for (const step of pathTo(tree, id)?.slice(0, -1) ?? []) screen.expand(step.id);
			if (reading !== null) await screen.read(reading);
			await screen.reshaped(tree);
		} catch (thrown) {
			screen.failed(thrown);
		}
	}

	/** Deletes a folder in the bin for good, and goes up to the folder it was
	 * in. */
	async eraseFolder(id: string, name: string): Promise<void> {
		const screen = this.#screen;
		const above = pathTo(screen.root(), id)?.at(-2)?.id ?? null;
		try {
			const tree = await this.#once.run([id], () => deleteGroup(id, 'forever'));
			if (tree === null) return;
			screen.select(above);
			await screen.reshaped(tree);
			if (!screen.unsaved()) this.#erased(name);
		} catch (thrown) {
			await this.#refused(thrown);
		}
	}

	/**
	 * Empties the bin, and says what to do about anything that would not go.
	 *
	 * A file that a previous version of some other entry names has to keep the
	 * number it has, and the pool of files has to stay an unbroken run from
	 * zero, so now and then an entry cannot be erased until those versions go.
	 * The message has to name the way out, because nothing on this screen shows
	 * which entry is in the way: open the one still in the bin and take its file
	 * off, which clears the versions holding it wherever they are.
	 *
	 * Whatever could go has gone by the time this is read, so pressing it again
	 * after that is a shorter list every time and never a longer one.
	 */
	async empty(): Promise<void> {
		const screen = this.#screen;
		let tree: Group;
		let stayed = false;
		try {
			tree = await emptyRecycleBin();
		} catch (thrown) {
			if (asFailure(thrown).code !== 'attachmentInHistory') {
				screen.failed(thrown);
				return;
			}
			// The refusal is about what stayed, not about what went: emptying
			// the bin is all-or-nothing per entry and the ones that could go
			// are already out of the vault in memory. So this is read back and
			// written like any other change - a screen that only reported the
			// refusal drew a bin that was emptier than the file, and lost the
			// erasures at the next lock.
			tree = await loadTree().catch(() => screen.root());
			stayed = true;
		}
		// The pane goes only with the entry in it. Rust may answer a second
		// later, behind a save, and an entry the reader opened from outside the
		// bin in that second was never in it.
		const showing = screen.showing();
		if (showing !== null && rowOf(tree, showing) === null) screen.close();
		await screen.reshaped(tree);
		if (stayed) {
			screen.notices.warn(
				'Some of it stayed: open what is left in the bin and remove its file first.'
			);
		}
	}
}
