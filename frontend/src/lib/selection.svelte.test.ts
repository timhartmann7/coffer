import { describe, expect, it, vi } from 'vitest';
import { row } from './fixtures';
import type { EntryRow } from './model';
import { pressOn, Selection } from './selection.svelte';

/** Ten rows, as a list draws them. */
function list(length = 10): EntryRow[] {
	return Array.from({ length }, (_, at) => row({ title: `entry ${at}` }));
}

/** The ids chosen, in the order the list draws them. */
function chosen(selection: Selection, rows: EntryRow[]): string[] {
	return selection.of(rows).map((each) => each.id);
}

/** The ids of rows, by position in the list. */
function at(rows: EntryRow[], ...positions: number[]): string[] {
	return positions.map((position) => rows[position].id);
}

describe('what a press does', () => {
	/** What a press with these keys held did: opened the row, or chose it
	 * and how. */
	function pressing(over: MouseEventInit, choosing = true): string {
		const pressedRow = row();
		const onOpen = vi.fn();
		const onChoose = vi.fn();
		pressOn(new MouseEvent('click', over), pressedRow, onOpen, choosing ? onChoose : undefined);
		expect(onOpen.mock.calls.length + onChoose.mock.calls.length, 'one press, one answer').toBe(1);
		const [chose] = onChoose.mock.calls;
		if (chose) expect(chose[0]).toBe(pressedRow);
		else expect(onOpen).toHaveBeenCalledWith(pressedRow);
		return chose ? chose[1] : 'open';
	}

	/** Control is how a one-button mouse asks for the menu under the pointer,
	 * and a reader who reached for that has chosen nothing. */
	it('chooses on Cmd and Shift, never on Control, and opens on anything else', () => {
		expect(pressing({})).toBe('open');
		expect(pressing({ metaKey: true })).toBe('toggle');
		expect(pressing({ shiftKey: true })).toBe('reach');
		expect(pressing({ shiftKey: true, metaKey: true })).toBe('reach');
		expect(pressing({ ctrlKey: true })).toBe('open');
		expect(pressing({ altKey: true })).toBe('open');
	});

	/** Where nothing may be chosen, a press with Cmd or Shift is a press like
	 * any other. */
	it('opens on every press where nothing may be chosen', () => {
		for (const keys of [{}, { metaKey: true }, { shiftKey: true }]) {
			expect(pressing(keys, false)).toBe('open');
		}
	});
});

describe('choosing rows', () => {
	it('chooses only the row a plain press opens', () => {
		const rows = list();
		const selection = new Selection();
		selection.toggle(rows[2].id);
		selection.toggle(rows[5].id);

		selection.only(rows[7].id);
		expect(chosen(selection, rows)).toEqual(at(rows, 7));
		selection.only(null);
		expect(chosen(selection, rows)).toEqual([]);
	});

	it('adds and takes away one row on Cmd, and leaves the others', () => {
		const rows = list();
		const selection = new Selection();
		selection.only(rows[1].id);
		selection.toggle(rows[4].id);
		selection.toggle(rows[6].id);
		expect(chosen(selection, rows)).toEqual(at(rows, 1, 4, 6));

		selection.toggle(rows[4].id);
		expect(chosen(selection, rows)).toEqual(at(rows, 1, 6));
		expect(selection.has(rows[4].id)).toBe(false);
		expect(selection.has(rows[6].id)).toBe(true);
	});

	it('reaches from the last row pressed in the order drawn, either way', () => {
		const rows = list();
		const down = new Selection();
		down.only(rows[2].id);
		down.reach(rows[5].id, rows);
		expect(chosen(down, rows)).toEqual(at(rows, 2, 3, 4, 5));

		const up = new Selection();
		up.only(rows[7].id);
		up.reach(rows[4].id, rows);
		expect(chosen(up, rows)).toEqual(at(rows, 4, 5, 6, 7));
	});

	/** Finder's rule. A second Shift-click moves the end of the run; it does
	 * not stack a second run on the first. What Cmd chose before the anchor
	 * was set stays. */
	it('replaces the run on a second Shift press and keeps what Cmd chose before it', () => {
		const rows = list();
		const selection = new Selection();
		selection.toggle(rows[0].id);
		selection.toggle(rows[5].id);
		selection.reach(rows[8].id, rows);
		expect(chosen(selection, rows)).toEqual(at(rows, 0, 5, 6, 7, 8));

		selection.reach(rows[3].id, rows);
		expect(chosen(selection, rows)).toEqual(at(rows, 0, 3, 4, 5));
	});

	it('acts as Cmd when there is nothing to reach from, or the row it was is not drawn', () => {
		const rows = list();
		const fresh = new Selection();
		fresh.reach(rows[4].id, rows);
		expect(chosen(fresh, rows)).toEqual(at(rows, 4));

		const hidden = new Selection();
		hidden.only(rows[1].id);
		const narrowed = rows.slice(3);
		hidden.reach(rows[6].id, narrowed);
		expect(chosen(hidden, rows)).toEqual(at(rows, 1, 6));
	});

	it('chooses every row drawn on Cmd+A and nothing else', () => {
		const rows = list();
		const drawn = rows.slice(2, 6);
		const selection = new Selection();
		selection.toggle(rows[9].id);
		selection.all(drawn);
		expect(chosen(selection, rows)).toEqual(at(rows, 2, 3, 4, 5));

		// The run after it starts where the last press was, while that is drawn.
		selection.only(rows[3].id);
		selection.all(drawn);
		selection.reach(rows[5].id, drawn);
		expect(chosen(selection, rows)).toEqual(at(rows, 3, 4, 5));
	});

	/** What is chosen is what is drawn. A row a search hid is not brought back
	 * by clearing the search, and the anchor it was goes with it. */
	it('lets go for good of rows no longer drawn, the anchor and the run included', () => {
		const rows = list();
		const selection = new Selection();
		selection.toggle(rows[1].id);
		selection.toggle(rows[2].id);
		selection.toggle(rows[8].id);

		selection.keep(rows.slice(0, 5));
		expect(chosen(selection, rows)).toEqual(at(rows, 1, 2));
		selection.keep(rows);
		expect(chosen(selection, rows), 'a row came back with the search cleared').toEqual(
			at(rows, 1, 2)
		);

		// The anchor was the eighth row, which went: Shift has nothing to
		// reach from, and chooses one row as Cmd would.
		selection.reach(rows[5].id, rows);
		expect(chosen(selection, rows)).toEqual(at(rows, 1, 2, 5));
	});

	it('gives the rows chosen back in the order drawn, not the order pressed', () => {
		const rows = list();
		const selection = new Selection();
		for (const position of [7, 0, 4]) selection.toggle(rows[position].id);
		expect(chosen(selection, rows)).toEqual(at(rows, 0, 4, 7));
	});

	/** Whatever the bar has open is about the rows chosen, and closes on a
	 * change. A press that changes nothing must not close it. */
	it('counts a change only when the choice changed', () => {
		const rows = list();
		const selection = new Selection();
		selection.only(rows[1].id);
		const before = selection.changes;

		selection.only(rows[1].id);
		selection.keep(rows);
		selection.reach(rows[1].id, rows);
		expect(selection.changes).toBe(before);

		selection.toggle(rows[2].id);
		expect(selection.changes).toBe(before + 1);
		selection.all(rows);
		selection.all(rows);
		expect(selection.changes).toBe(before + 2);
	});

	it('chooses fifty thousand rows and reads them back in order', () => {
		const rows = list(50_000);
		const selection = new Selection();
		selection.all(rows);
		expect(chosen(selection, rows)).toEqual(rows.map((each) => each.id));

		selection.only(rows[49_999].id);
		selection.reach(rows[0].id, rows);
		expect(selection.of(rows)).toHaveLength(50_000);
		selection.keep(rows.slice(0, 10));
		expect(chosen(selection, rows)).toEqual(rows.slice(0, 10).map((each) => each.id));
	});
});
