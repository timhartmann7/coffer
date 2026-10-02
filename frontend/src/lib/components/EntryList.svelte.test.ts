import { createRawSnippet, flushSync, mount, unmount, type Snippet } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { row } from '$lib/fixtures';
import type { EntryRow } from '$lib/model';
import type { Press } from '$lib/selection.svelte';
import EntryList from './EntryList.svelte';

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
	handlers: {
		onOpen?: (row: EntryRow) => void;
		onCopy?: (row: EntryRow, field: 'UserName' | 'Password') => void;
		onChoose?: (row: EntryRow, press: Press) => void;
		chosen?: ReadonlySet<string>;
		bar?: Snippet;
		note?: (row: EntryRow) => string;
		onMenu?: (event: MouseEvent, row: EntryRow) => void;
	}
) {
	return mount(EntryList, {
		target: host,
		props: {
			rows,
			now: new Date('2026-08-29T14:30:00Z'),
			...handlers,
			onOpen: handlers.onOpen ?? vi.fn(),
			onCopy: handlers.onCopy ?? vi.fn(),
			onMenu: handlers.onMenu ?? vi.fn()
		}
	});
}

/** The button a row is pressed through. */
function opener(at = 0): HTMLButtonElement {
	const found = [...host.querySelectorAll<HTMLButtonElement>('button')].filter(
		(each) => !each.getAttribute('aria-label')?.startsWith('Copy')
	)[at];
	if (!found) throw new Error('there is no such row');
	return found;
}

/** A press on a row, with whatever keys were held. */
function click(target: HTMLElement, keys: MouseEventInit = {}) {
	target.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, ...keys }));
	flushSync();
}

/** A title the database protects has no value to draw, and an empty cell would
 * read as an entry without a name. */
it('draws the mask where a value is protected', () => {
	const component = draw([row({ title: null, username: null })], {});
	flushSync();

	expect(host.querySelectorAll('use[href="#redact"]')).toHaveLength(2);

	return unmount(component);
});

it('says how many tags it did not draw rather than cutting one in half', () => {
	const component = draw([row({ tags: ['prod', 'ssh', 'database', 'eu'] })], {});
	flushSync();

	const tags = [...host.querySelectorAll('em')].map((each) => each.textContent?.trim());
	expect(tags).toEqual(['prod', 'ssh', '+2']);

	return unmount(component);
});

it('offers a copy only for a field the entry has', () => {
	const component = draw(
		[row({ username: '', hasPassword: false }), row({ username: 'deploy', hasPassword: true })],
		{}
	);
	flushSync();

	const copies = [...host.querySelectorAll('button')].filter((each) =>
		each.getAttribute('aria-label')?.startsWith('Copy')
	);
	expect(copies.map((each) => each.disabled)).toEqual([true, true, false, false]);

	return unmount(component);
});

it('copies the field the button names, without the value passing through', () => {
	const onCopy = vi.fn();
	const only = row({ username: 'deploy', hasPassword: true });
	const component = draw([only], { onCopy });
	flushSync();

	const copies = [...host.querySelectorAll('button')].filter((each) =>
		each.getAttribute('aria-label')?.startsWith('Copy')
	);
	copies[0].click();
	copies[1].click();
	flushSync();

	expect(onCopy.mock.calls).toEqual([
		[only, 'UserName'],
		[only, 'Password']
	]);

	return unmount(component);
});

/**
 * The row lights up under the pointer and the row is what opens the entry.
 *
 * Only the name used to be the way in, so the top and the bottom of a lit row
 * answered nothing at all: the list said one thing with its highlight and did
 * another with the press, which a reader reads as the application being broken.
 */
it('makes the whole row the way in, not the name in it', () => {
	const onOpen = vi.fn();
	const only = row({ title: 'node-3', username: 'deploy', tags: ['prod'], hasPassword: true });
	const component = draw([only], { onOpen });
	flushSync();

	const opener = host.querySelector('button');
	if (!opener) throw new Error('the row has nothing to click');

	for (const drawn of ['node-3', 'deploy', 'prod']) {
		expect(opener.textContent, `${drawn} sits outside the row's own button`).toContain(drawn);
	}

	const lit = opener.parentElement;
	expect(lit?.className, 'the row lights up somewhere other than where it opens').toContain(
		'hover:bg-raised'
	);

	// The two copy buttons are the one thing outside it, because a button cannot
	// hold a button. They are laid over the column left empty for them.
	const copies = [...host.querySelectorAll('button')].filter((each) =>
		each.getAttribute('aria-label')?.startsWith('Copy')
	);
	expect(copies).toHaveLength(2);
	for (const copy of copies) {
		expect(opener.contains(copy), 'a copy button is inside the button that opens the row').toBe(
			false
		);
		expect(lit?.contains(copy), 'a copy button left the row it belongs to').toBe(true);
	}

	opener.click();
	expect(onOpen).toHaveBeenCalledWith(only);

	return unmount(component);
});

/** An entry with no title is still an entry, and the row still has to be
 * something a reader can hit. */
it('opens an entry that has no title at all', () => {
	const onOpen = vi.fn();
	const untitled = row({ title: '' });
	const component = draw([untitled], { onOpen });
	flushSync();

	const opener = host.querySelector('button');
	if (!opener) throw new Error('the row has nothing to click');
	opener.click();
	flushSync();

	expect(onOpen).toHaveBeenCalledWith(untitled);

	return unmount(component);
});

/**
 * The last column of the row is sixty-six pixels wide and holds two buttons
 * twenty-four pixels across. What is left of it, and the whole of it when both
 * buttons are disabled, has to answer the press the lit row promises - which is
 * the defect this row was rebuilt for, turned on its side.
 */
it('lets the strip the copy buttons sit in pass a press to the row', () => {
	const component = draw([row({ username: '', hasPassword: false })], {});
	flushSync();

	const strip = host.querySelector('.absolute');
	expect(strip?.className, 'the strip swallows presses meant for the row').toContain(
		'pointer-events-none'
	);

	const copies = [...host.querySelectorAll('button')].filter((each) =>
		each.getAttribute('aria-label')?.startsWith('Copy')
	);
	expect(copies).toHaveLength(2);
	for (const copy of copies) {
		expect(copy.disabled, 'this entry has something to copy after all').toBe(true);
		expect(copy.className, 'a button with nothing to copy still swallows the press').toContain(
			'disabled:pointer-events-none'
		);
	}

	return unmount(component);
});

it('draws a row whose tags repeat', () => {
	const component = draw([row({ tags: ['prod', 'prod', '', ''] })], {});
	flushSync();

	expect([...host.querySelectorAll('em')].map((each) => each.textContent?.trim())).toEqual([
		'prod',
		'prod',
		'+2'
	]);

	return unmount(component);
});

/** A login the database protects is masked in the row, and copying it still
 * works: the copy happens in Rust, by field name, whether or not the value ever
 * crossed. */
it('still offers to copy a login it is only allowed to mask', () => {
	const component = draw([row({ username: null, hasPassword: true })], {});
	flushSync();

	const copies = [...host.querySelectorAll('button')].filter((each) =>
		each.getAttribute('aria-label')?.startsWith('Copy')
	);
	expect(copies.map((each) => each.disabled)).toEqual([false, false]);

	return unmount(component);
});

it('writes a value that is markup as text', () => {
	const component = draw([row({ title: '<script>alert(1)</script>', username: '<b>x</b>' })], {});
	flushSync();

	expect(host.querySelector('script')).toBeNull();
	expect(host.querySelector('b')).toBeNull();
	expect(host.textContent).toContain('<script>alert(1)</script>');

	return unmount(component);
});

/** A press with Cmd or Shift chooses the row and opens nothing, and a plain
 * press opens it and chooses nothing here: the window decides what opening
 * does to the choice. Control is the menu under the pointer, which opens. */
it('opens on a plain press and chooses on Cmd or Shift, never both', () => {
	const onOpen = vi.fn();
	const onChoose = vi.fn();
	const rows = [row({ title: 'one' }), row({ title: 'two' })];
	const component = draw(rows, { onOpen, onChoose });
	flushSync();

	click(opener(1), { metaKey: true });
	click(opener(0), { shiftKey: true });
	expect(onChoose.mock.calls).toEqual([
		[rows[1], 'toggle'],
		[rows[0], 'reach']
	]);
	expect(onOpen).not.toHaveBeenCalled();

	click(opener(0));
	click(opener(1), { ctrlKey: true });
	expect(onOpen.mock.calls).toEqual([[rows[0]], [rows[1]]]);
	expect(onChoose).toHaveBeenCalledTimes(2);

	return unmount(component);
});

/** A vault Coffer will not write offers nothing a choice could be acted on
 * with, so a press with Cmd is a press like any other. */
it('opens on every press when nothing may be chosen', () => {
	const onOpen = vi.fn();
	const only = row({ title: 'one' });
	const component = draw([only], { onOpen });
	flushSync();

	click(opener(), { metaKey: true });
	click(opener(), { shiftKey: true });
	expect(onOpen.mock.calls).toEqual([[only], [only]]);

	return unmount(component);
});

/** The selection colour, with every line on it lifted a step so that it stays
 * readable there, and the word a screen reader cannot see in a colour. */
it('draws a chosen row on the selection plane and says so to a screen reader', () => {
	const rows = [row({ title: 'one', username: 'me', tags: ['work'] }), row({ title: 'two' })];
	const component = draw(rows, {
		chosen: new Set([rows[0].id]),
		note: () => 'In the Recycle Bin since 28 Aug'
	});
	flushSync();

	const [picked, plain] = [opener(0), opener(1)].map((each) => each.parentElement);
	expect(picked?.className).toContain('bg-selection');
	expect(picked?.className).not.toContain('hover:bg-raised');
	expect(plain?.className).not.toContain('bg-selection');
	expect(opener(0).textContent).toContain(', selected');
	expect(opener(1).textContent).not.toContain('selected');

	// Nothing on the plane is written in the two faintest steps of text, which
	// fall under 4.5 : 1 on it: not the name with its icon, not the note under
	// it, not the login or the date. A tag keeps its own chip.
	const words = [...opener(0).querySelectorAll('span')].filter((each) => !each.closest('em'));
	const read = (text: string) => words.find((each) => each.textContent?.trim().startsWith(text));
	const icon = opener(0).querySelector('svg');
	expect(icon?.parentElement?.className, 'the name').toMatch(/(^| )text-txt( |$)/);
	expect(read('In the Recycle Bin')?.className, 'the note').toContain('text-txt2');
	expect(read('me')?.className, 'the login').toContain('text-txt2');
	for (const word of words) {
		expect(word.className, word.textContent ?? '').not.toMatch(/text-txt[34](?!\d)/);
	}
	expect(icon?.getAttribute('class')).toContain('text-txt3');
	expect(opener(1).querySelector('svg')?.getAttribute('class')).toContain('text-txt4');

	return unmount(component);
});

/** The bar takes the column names' place, and the rows under it stay where
 * they were. */
it('puts the bar where the column names were', () => {
	const bar = createRawSnippet(() => ({ render: () => '<p data-bar>2 selected</p>' }));
	const rows = [row(), row()];
	const component = draw(rows, { bar });
	flushSync();

	expect(host.querySelector('[data-bar]')).not.toBeNull();
	expect(host.textContent).not.toContain('Changed');
	const header = host.firstElementChild;
	expect(header?.hasAttribute('data-bar'), 'the bar is not where the header was').toBe(true);

	return unmount(component);
});

/** A copy button lies over the row, not inside it, so a Cmd-click on one is a
 * copy, as it always was, and chooses nothing. */
it('copies on a Cmd-click of a copy button rather than choosing the row', () => {
	const onCopy = vi.fn();
	const onChoose = vi.fn();
	const only = row({ username: 'deploy', hasPassword: true });
	const component = draw([only], { onCopy, onChoose });
	flushSync();

	const copy = host.querySelector<HTMLButtonElement>('[aria-label="Copy password"]');
	if (!copy) throw new Error('there is no copy button');
	click(copy, { metaKey: true });
	expect(onCopy).toHaveBeenCalledWith(only, 'Password');
	expect(onChoose).not.toHaveBeenCalled();

	return unmount(component);
});
