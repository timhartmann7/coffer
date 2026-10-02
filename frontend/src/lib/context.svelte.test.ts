import { flushSync } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { about, chosen, forget, marked, offer, plain } from './context.svelte';
import type { Chosen, Subject } from './model';
import type { Stubbed } from './stubbed';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('./ipc'), async (real) =>
	Object.assign(ipc, (await import('./stubbed')).stubbed(await real()))
);

const SECRET = 'correct horse battery staple';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.contextMenu.mockResolvedValue(undefined);
});

afterEach(() => {
	forget();
	host.remove();
});

/** A right-click at a place in the window, on `target`. */
function rightClick(target: EventTarget, x = 12, y = 34): MouseEvent {
	const event = new MouseEvent('contextmenu', {
		bubbles: true,
		cancelable: true,
		clientX: x,
		clientY: y
	});
	target.dispatchEvent(event);
	return event;
}

/** An element on the screen that asks for a menu about `subject`. */
function asking(
	subject: Subject,
	answer: (item: Chosen) => void = vi.fn(),
	failed: (thrown: unknown) => void = vi.fn(),
	tag = 'button'
): HTMLElement {
	const element = document.createElement(tag);
	element.addEventListener('contextmenu', (event) => offer(event, subject, answer, failed));
	host.appendChild(element);
	return element;
}

/** The number Rust was given for the menu asked for last. */
function lastSerial(): number {
	return ipc.contextMenu.mock.calls.at(-1)?.[0] as number;
}

const ROW: Subject = { kind: 'entry', entry: 'entry-1', places: [] };
const COPY: Chosen = { item: 'copyField', entry: 'entry-1', field: 'Password' };

/** What crosses is what the window already holds: ids, a field's name, where
 * the pointer was and the positions of the part selected. A revealed value on
 * the screen is not read, and nothing of it is in the message. */
it('asks Rust with ids and positions and nothing of any value', () => {
	const value = document.createElement('span');
	value.textContent = SECRET;
	host.appendChild(value);
	value.addEventListener('contextmenu', (event) =>
		offer(
			event,
			{ kind: 'value', entry: 'entry-1', field: 'Password', range: { from: 2, to: 6 } },
			vi.fn(),
			vi.fn()
		)
	);

	const event = rightClick(value, 40, 50);

	expect(event.defaultPrevented).toBe(true);
	expect(ipc.contextMenu).toHaveBeenCalledWith(
		expect.any(Number),
		{ kind: 'value', entry: 'entry-1', field: 'Password', range: { from: 2, to: 6 } },
		{ x: 40, y: 50 }
	);
	expect(JSON.stringify(ipc.contextMenu.mock.calls)).not.toContain(SECRET);
});

/** In a field the reader can type into, WebKit's own menu is about their
 * typing - Cut, Paste, spelling - and stays. Anything they cannot type into
 * is not that field, a read-only or disabled one included. */
it('leaves WebKit’s menu to a field the reader is typing in', () => {
	const fields: HTMLElement[] = [];
	for (const type of ['text', 'search', 'password', 'email', 'url', 'tel', 'number']) {
		const input = document.createElement('input');
		input.type = type;
		fields.push(input);
	}
	fields.push(document.createElement('textarea'));
	const editable = document.createElement('div');
	editable.contentEditable = 'true';
	fields.push(editable);

	for (const field of fields) {
		field.addEventListener('contextmenu', (event) => offer(event, ROW, vi.fn(), vi.fn()));
		host.appendChild(field);
		const event = rightClick(field);
		expect(event.defaultPrevented, field.outerHTML).toBe(false);
	}
	expect(ipc.contextMenu).not.toHaveBeenCalled();

	const readOnly = document.createElement('input');
	readOnly.readOnly = true;
	const disabled = document.createElement('textarea');
	disabled.disabled = true;
	const slider = document.createElement('input');
	slider.type = 'range';
	const box = document.createElement('input');
	box.type = 'checkbox';
	const others: HTMLElement[] = [readOnly, disabled, slider, box, document.createElement('button')];
	for (const other of others) {
		other.addEventListener('contextmenu', (event) => offer(event, ROW, vi.fn(), vi.fn()));
		host.appendChild(other);
		expect(rightClick(other).defaultPrevented, other.outerHTML).toBe(true);
	}
	expect(ipc.contextMenu).toHaveBeenCalledTimes(others.length);
});

/** A field inside a row is the field's menu: the row around it never asks
 * as well. */
it('asks for the menu of the innermost thing only', () => {
	const row = asking(ROW, vi.fn(), vi.fn(), 'div');
	const field = document.createElement('button');
	field.addEventListener('contextmenu', (event) =>
		offer(event, { kind: 'field', entry: 'entry-1', field: 'PIN', shown: false }, vi.fn(), vi.fn())
	);
	row.appendChild(field);

	rightClick(field);

	expect(ipc.contextMenu).toHaveBeenCalledTimes(1);
	expect(ipc.contextMenu.mock.calls[0][1]).toMatchObject({ kind: 'field' });
});

/** Two right-clicks, the second while the first menu was waiting: only the
 * menu asked for last runs what was chosen from it, and only once. */
it('runs what was chosen only for the menu asked for last, and once', () => {
	const first = vi.fn();
	const second = vi.fn();
	rightClick(asking(ROW, first));
	const earlier = lastSerial();
	rightClick(asking(ROW, second));
	const later = lastSerial();
	expect(later).not.toBe(earlier);

	chosen(earlier, COPY);
	expect(first).not.toHaveBeenCalled();
	expect(second).not.toHaveBeenCalled();

	chosen(later, COPY);
	chosen(later, COPY);
	expect(second).toHaveBeenCalledTimes(1);
	expect(second).toHaveBeenCalledWith(COPY);
	expect(first).not.toHaveBeenCalled();
});

/** AppKit hands the choice on while the menu is still up, and Rust answers
 * that it has closed once it has: the two reach the page in either order. The
 * thing the menu was about is marked until Rust answers, either way. */
it('runs what was chosen whether it arrives before or after the menu says it closed', async () => {
	for (const order of ['chosen first', 'closed first']) {
		let close = () => {};
		ipc.contextMenu.mockReturnValueOnce(
			new Promise<void>((resolve) => {
				close = resolve;
			})
		);
		const answer = vi.fn();
		rightClick(asking({ kind: 'folder', group: 'group-1', places: [] }, answer));
		flushSync();
		expect(marked('folder', 'group-1'), order).toBe(true);
		expect(marked('folder', 'group-2'), order).toBe(false);

		if (order === 'chosen first') chosen(lastSerial(), { item: 'renameFolder', group: 'group-1' });
		close();
		await vi.waitFor(() => expect(marked('folder', 'group-1'), order).toBe(false));
		if (order === 'closed first') chosen(lastSerial(), { item: 'renameFolder', group: 'group-1' });

		expect(answer, order).toHaveBeenCalledTimes(1);
	}
});

/** The thing the menu was about went from the screen before the item came -
 * the pane moved to another entry, and its field's row with it - so the item
 * is about nothing there, and runs nothing. */
it('runs nothing for a thing that has left the screen', () => {
	const answer = vi.fn();
	const row = asking(ROW, answer);
	rightClick(row);
	row.remove();

	chosen(lastSerial(), COPY);

	expect(answer).not.toHaveBeenCalled();
});

/** The rows of a choice are all marked, and so is a file by its entry and its
 * name together: the same name on another entry is another file. */
it('marks every row of a choice, and a file by its entry and name', async () => {
	let close = () => {};
	ipc.contextMenu.mockReturnValue(
		new Promise<void>((resolve) => {
			close = resolve;
		})
	);
	rightClick(asking({ kind: 'entries', entries: ['a', 'b', 'c'], places: [] }));
	flushSync();
	expect(['a', 'b', 'c', 'd'].map((id) => marked('entry', id))).toEqual([true, true, true, false]);

	rightClick(asking({ kind: 'file', entry: 'a', name: 'scan.pdf' }));
	flushSync();
	expect(marked('entry', 'a'), 'the menu before').toBe(false);
	expect(marked('file', 'a', 'scan.pdf')).toBe(true);
	expect(marked('file', 'b', 'scan.pdf')).toBe(false);
	expect(marked('file', 'a', 'scan')).toBe(false);

	rightClick(asking({ kind: 'bin' }));
	flushSync();
	expect(marked('bin')).toBe(true);
	close();
	await vi.waitFor(() => expect(marked('bin')).toBe(false));
});

/** A menu Rust would not draw - the entry went a moment before - is said, and
 * nothing stays marked for it. */
it('says why a menu was refused and takes the mark away', async () => {
	const refusal = { code: 'noSuchEntry', message: 'there is no such entry in this database' };
	ipc.contextMenu.mockRejectedValueOnce(refusal);
	const failed = vi.fn();
	rightClick(asking(ROW, vi.fn(), failed));

	await vi.waitFor(() => expect(failed).toHaveBeenCalledWith(refusal));
	expect(marked('entry', 'entry-1')).toBe(false);
});

/** The screen that asked went - a lock, the settings over it - and an item
 * that arrives afterwards finds nobody to run it. */
it('forgets everything when the screen that asked goes', () => {
	let close = () => {};
	ipc.contextMenu.mockReturnValueOnce(
		new Promise<void>((resolve) => {
			close = resolve;
		})
	);
	const answer = vi.fn();
	rightClick(asking(ROW, answer));
	flushSync();

	forget();
	flushSync();
	chosen(lastSerial(), COPY);

	expect(answer).not.toHaveBeenCalled();
	expect(marked('entry', 'entry-1')).toBe(false);
	close();
});

/** Anywhere the window does not know what was clicked, there is no menu at
 * all; in a field the reader types into, WebKit's. */
it('draws no menu on anything else, and WebKit’s in a field', () => {
	const chrome = document.createElement('p');
	const field = document.createElement('input');
	host.append(chrome, field);
	host.addEventListener('contextmenu', plain);

	expect(rightClick(chrome).defaultPrevented).toBe(true);
	expect(rightClick(field).defaultPrevented).toBe(false);
	expect(ipc.contextMenu).not.toHaveBeenCalled();
});

/** A field's row and a value run an item only while it names them: the same
 * field of another entry, another field of the same entry, and an item about
 * no field at all - a row's, a folder's, a file's with the field's name - are
 * someone else's. */
it('knows an item about a field only by its entry and its name', () => {
	expect(about(COPY, 'entry-1', 'Password')).toBe(true);
	expect(about({ ...COPY, item: 'hideField' }, 'entry-1', 'Password')).toBe(true);
	for (const [item, entry, field] of [
		[COPY, 'entry-2', 'Password'],
		[COPY, 'entry-1', 'password'],
		[COPY, 'entry-1', 'Password '],
		[{ item: 'openAddress', entry: 'entry-1' }, 'entry-1', 'Password'],
		[{ item: 'removeFile', entry: 'entry-1', name: 'Password' }, 'entry-1', 'Password'],
		[{ item: 'emptyBin' }, 'entry-1', 'Password']
	] as [Chosen, string, string][]) {
		expect(about(item, entry, field), JSON.stringify([item, entry, field])).toBe(false);
	}
});
