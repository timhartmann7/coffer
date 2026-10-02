/**
 * Making entries: of a kind, from one of the vault's templates, and as a copy
 * of the entry in the pane - and opening what was made, ready to be named.
 */

import { flush } from './drafts';
import { createEntry, createFromTemplate, duplicateEntry } from './ipc';
import type { EntryRow, Group, Made, Offer } from './model';
import { Once } from './once';
import { rowOf } from './tree';

/** The vault screen, as making an entry needs it. Read through functions,
 * because every one of them can have changed by the time Rust answers. */
export interface Screen {
	/** The entry in the pane, read or not. */
	showing(): string | null;
	/** Whether the pane has to stay where it is (`holding.ts`). */
	held(): boolean;
	/** Draws the tree a creation came back with, as a change to the file. */
	drawn(tree: Group): void;
	/** Puts the entry made in the pane, chosen on its own, and reads it.
	 * `lined` is the fields its kind writes in lines. */
	open(row: EntryRow, lined: readonly string[]): Promise<void>;
	/** Writes the file. */
	persist(): Promise<void>;
	/** Says why something was refused. */
	failed(thrown: unknown): void;
}

/** What every making is about while it is on its way: one at a time, since a
 * second press of "+ Entry" during a save would make a second entry nobody
 * asked for. A copy is about the entry it copies instead. */
const MAKING = 'make';

export class Making {
	readonly #screen: Screen;
	readonly #once = new Once();

	constructor(screen: Screen) {
		this.#screen = screen;
	}

	/** Makes an entry of the kind offered in a folder, and answers whether it
	 * was made. */
	kind(offer: Offer, into: string): Promise<boolean> {
		return this.#make([MAKING], () => createEntry(into, offer.kind), offer.lined);
	}

	/** Makes an entry from one of the vault's templates in a folder, and
	 * answers whether it was made. */
	template(id: string, into: string): Promise<boolean> {
		return this.#make([MAKING], () => createFromTemplate(into, id), []);
	}

	/**
	 * Makes a copy of an entry beside it, holding what the entry holds once
	 * every value on its way to Rust has arrived.
	 *
	 * The press has already left the field being written in, which is what
	 * writes it: the pill's press does, and so does every item of the menu
	 * bar (`run` in `menu.svelte.ts`). What that sent is waited for, because
	 * Tauri answers the copy beside a write rather than after it. The wait is
	 * part of the copy's own trip, so a second press during it is dropped like
	 * one during the copy, and the pane is taken as it was at the press.
	 */
	copy(entry: string): Promise<boolean> {
		return this.#make(
			[entry],
			async () => {
				await flush();
				return duplicateEntry(entry);
			},
			[]
		);
	}

	/**
	 * Asks Rust for an entry, draws the tree it comes back with, opens it and
	 * writes the file.
	 *
	 * Opened only when the pane is where it was when the reader pressed - read
	 * here, before anything is waited for: an entry they chose while Rust was
	 * making this one, or while what it waited on arrived, is their later
	 * choice, and the new entry waits in the list rather than taking the pane
	 * from it. And only when the pane may go: a new password typed into a
	 * Change field meanwhile keeps the pane, and the new entry is neither
	 * opened nor chosen.
	 *
	 * The new entry's name takes the focus as it is drawn, which it may only
	 * when the focus is nowhere. So the press is let go of first - a button
	 * the reader pressed, a line of a list - unless they have put the focus
	 * somewhere else since, which is where they are typing now.
	 */
	async #make(
		keys: readonly string[],
		ask: () => Promise<Made>,
		lined: readonly string[]
	): Promise<boolean> {
		const screen = this.#screen;
		const pressed = document.activeElement;
		const at = screen.showing();
		let made: Made | null;
		try {
			made = await this.#once.run(keys, ask);
		} catch (thrown) {
			screen.failed(thrown);
			return false;
		}
		if (made === null) return false;
		screen.drawn(made.tree);
		const row = rowOf(made.tree, made.entry);
		if (row && screen.showing() === at && !screen.held()) {
			if (pressed instanceof HTMLElement && document.activeElement === pressed) pressed.blur();
			await screen.open(row, lined);
		}
		await screen.persist();
		return true;
	}
}
