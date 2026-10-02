/**
 * Which rows of the list the reader has chosen to act on together.
 *
 * Only ids, and only of rows the list draws: what is chosen is cut back to
 * what is on the screen every time the list changes, so nothing acted on is
 * out of sight. Replaced whole on every change rather than changed in place,
 * because both lists read it once per row, and a set of fifty thousand
 * watched key by key costs more than one set read fifty thousand times.
 */

import type { EntryRow } from './model';

/** What a press on a row does to the choice: Cmd adds the row or takes it
 * out, Shift reaches from the last row pressed to this one. */
export type Press = 'toggle' | 'reach';

const NOTHING: ReadonlySet<string> = new Set();

/** The ids of a set together with `more`, as a new set: a choice is replaced,
 * never changed in place, so nothing reading the old one sees it move. */
function joined(ids: ReadonlySet<string>, more: Iterable<string>): ReadonlySet<string> {
	return new Set([...ids, ...more]);
}

/** The ids of a set that pass `test`, as a new set. */
function kept(ids: ReadonlySet<string>, test: (id: string) => boolean): ReadonlySet<string> {
	return new Set([...ids].filter(test));
}

/**
 * How a press on a row chooses it, or `null` for a press that opens the row.
 *
 * Control is not Cmd. On a Mac a Control-click is the menu under the pointer,
 * and a reader who reached for that has not chosen anything.
 */
function pressed(event: MouseEvent): Press | null {
	if (event.shiftKey) return 'reach';
	if (event.metaKey) return 'toggle';
	return null;
}

/**
 * A press on a row of either list: Cmd and Shift choose the row, and anything
 * else opens it. Where nothing may be chosen there is no `onChoose`, and every
 * press opens, whatever keys were held.
 */
export function pressOn(
	event: MouseEvent,
	row: EntryRow,
	onOpen: (row: EntryRow) => void,
	onChoose?: (row: EntryRow, press: Press) => void
): void {
	const how = pressed(event);
	if (how !== null && onChoose) onChoose(row, how);
	else onOpen(row);
}

export class Selection {
	/** The ids chosen. */
	ids = $state.raw<ReadonlySet<string>>(NOTHING);
	/**
	 * How many times the choice has changed. Whatever was opened about one
	 * choice - a tag being typed, a list of folders, a question before
	 * deleting for good - is about rows that may no longer be the ones
	 * chosen, and closes on the next.
	 */
	changes = $state(0);
	/** The row Shift reaches from: the last one pressed. */
	#anchor: string | null = null;
	/** What was chosen when the anchor was set, which a run from it is added
	 * to: a second Shift-click replaces the run, it does not add another. */
	#base: ReadonlySet<string> = NOTHING;

	has(id: string): boolean {
		return this.ids.has(id);
	}

	/** Chooses the one row a plain press opened, or nothing at all. */
	only(id: string | null): void {
		this.#anchor = id;
		this.#base = NOTHING;
		this.#set(id === null ? NOTHING : joined(NOTHING, [id]));
	}

	/** Adds a row to the choice, or takes it out. */
	toggle(id: string): void {
		const next = this.ids.has(id) ? kept(this.ids, (each) => each !== id) : joined(this.ids, [id]);
		this.#anchor = id;
		this.#base = next;
		this.#set(next);
	}

	/**
	 * Chooses every row from the last one pressed to this one, in the order
	 * the list draws them, on top of what was chosen when that one was
	 * pressed. With nothing pressed before, or a row the list no longer draws,
	 * there is nowhere to reach from, and it chooses this row alone the way a
	 * Cmd-click would.
	 */
	reach(id: string, rows: readonly EntryRow[]): void {
		const from = this.#anchor === null ? -1 : rows.findIndex((row) => row.id === this.#anchor);
		const to = rows.findIndex((row) => row.id === id);
		if (from === -1 || to === -1) {
			this.toggle(id);
			return;
		}
		const run = rows.slice(Math.min(from, to), Math.max(from, to) + 1);
		this.#set(
			joined(
				this.#base,
				run.map((row) => row.id)
			)
		);
	}

	/** Chooses every row the list draws: Cmd+A. */
	all(rows: readonly EntryRow[]): void {
		const next = joined(
			NOTHING,
			rows.map((row) => row.id)
		);
		if (this.#anchor !== null && !next.has(this.#anchor)) this.#anchor = null;
		this.#base = NOTHING;
		this.#set(next);
	}

	/**
	 * Lets go, for good, of every row the list no longer draws: one a search
	 * hides, one deleted or moved out of the folder shown. Clearing the search
	 * does not bring it back.
	 */
	keep(rows: readonly EntryRow[]): void {
		const drawn = joined(
			NOTHING,
			rows.map((row) => row.id)
		);
		const shown = (id: string) => drawn.has(id);
		if (this.#anchor !== null && !shown(this.#anchor)) this.#anchor = null;
		this.#base = kept(this.#base, shown);
		this.#set(kept(this.ids, shown));
	}

	/** The rows chosen, in the order the list draws them rather than the order
	 * they were pressed in. */
	of(rows: readonly EntryRow[]): EntryRow[] {
		const ids = this.ids;
		if (ids.size === 0) return [];
		return rows.filter((row) => ids.has(row.id));
	}

	/** Replaces the choice, and counts it as a change only when it is one. */
	#set(next: ReadonlySet<string>): void {
		const same = next.size === this.ids.size && [...next].every((id) => this.ids.has(id));
		if (same) return;
		this.ids = next;
		this.changes += 1;
	}
}
