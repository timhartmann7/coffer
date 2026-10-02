import { createRawSnippet, flushSync, mount, unmount, type Snippet } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { row } from '$lib/fixtures';
import type { EntryRow } from '$lib/model';
import type { Press } from '$lib/selection.svelte';
import EntryListCompact from './EntryListCompact.svelte';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => {
	host.remove();
});

function draw(
	rows: EntryRow[],
	over: {
		open?: string | null;
		onOpen?: (row: EntryRow) => void;
		onChoose?: (row: EntryRow, press: Press) => void;
		chosen?: ReadonlySet<string>;
		bar?: Snippet;
		onMenu?: (event: MouseEvent, row: EntryRow) => void;
	} = {}
) {
	return mount(EntryListCompact, {
		target: host,
		props: {
			rows,
			open: null,
			onDismiss: vi.fn(),
			...over,
			onOpen: over.onOpen ?? vi.fn(),
			onMenu: over.onMenu ?? vi.fn()
		}
	});
}

/** The card a row is drawn as, which is the button it is pressed through. */
function card(at: number): HTMLButtonElement {
	const found = host.querySelectorAll<HTMLButtonElement>('button')[at];
	if (!found) throw new Error('there is no such card');
	return found;
}

function click(target: HTMLElement, keys: MouseEventInit = {}) {
	target.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, ...keys }));
	flushSync();
}

/** The accent bar down the side of the card being read. */
const accent = (target: HTMLElement) => target.querySelector('.bg-accent');

it('opens on a plain press and chooses on Cmd or Shift, never both', () => {
	const onOpen = vi.fn();
	const onChoose = vi.fn();
	const rows = [row({ title: 'one' }), row({ title: 'two' })];
	const component = draw(rows, { onOpen, onChoose });
	flushSync();

	click(card(1), { metaKey: true });
	click(card(0), { shiftKey: true });
	click(card(1));
	click(card(0), { ctrlKey: true });
	expect(onChoose.mock.calls).toEqual([
		[rows[1], 'toggle'],
		[rows[0], 'reach']
	]);
	expect(onOpen.mock.calls).toEqual([[rows[1]], [rows[0]]]);

	return unmount(component);
});

it('opens on every press when nothing may be chosen', () => {
	const onOpen = vi.fn();
	const only = row({ title: 'one' });
	const component = draw([only], { onOpen });
	flushSync();

	click(card(0), { metaKey: true });
	click(card(0), { shiftKey: true });
	expect(onOpen.mock.calls).toEqual([[only], [only]]);

	return unmount(component);
});

it('draws a chosen card on the selection plane and says so to a screen reader', () => {
	const rows = [row({ title: 'one', username: 'me' }), row({ title: 'two' })];
	const component = draw(rows, { chosen: new Set([rows[0].id]) });
	flushSync();

	expect(card(0).className).toContain('bg-selection');
	expect(card(0).className).toContain('border-transparent');
	expect(card(1).className).not.toContain('bg-selection');
	expect(card(0).textContent).toContain(', selected');
	expect(card(1).textContent).not.toContain('selected');
	for (const word of card(0).querySelectorAll('span')) {
		expect(word.className, word.textContent ?? '').not.toMatch(/text-txt[34]\b/);
	}

	return unmount(component);
});

/** The entry being read keeps what marks it as the one being read - the
 * accent bar and the hairline - when it is chosen with others as well. */
it('keeps the accent bar on the open card when it is chosen too', () => {
	const rows = [row({ title: 'one' }), row({ title: 'two' })];
	const component = draw(rows, { open: rows[0].id, chosen: new Set([rows[0].id, rows[1].id]) });
	flushSync();

	expect(accent(card(0))).not.toBeNull();
	expect(card(0).className).toContain('border-hairline');
	expect(card(0).className).toContain('bg-selection');
	expect(accent(card(1))).toBeNull();
	expect(card(1).className).toContain('border-transparent');

	return unmount(component);
});

/** Beside an open entry the list has no column names, and the bar is a line
 * of its own above the cards, outside what scrolls. */
it('puts the bar above the cards, outside what scrolls', () => {
	const bar = createRawSnippet(() => ({ render: () => '<p data-bar>2 selected</p>' }));
	const component = draw([row(), row()], { bar });
	flushSync();

	const drawn = host.querySelector('[data-bar]');
	expect(drawn).not.toBeNull();
	expect(drawn?.closest('.overflow-y-auto'), 'the bar scrolls away with the cards').toBeNull();
	expect(host.firstElementChild).toBe(drawn);

	return unmount(component);
});
