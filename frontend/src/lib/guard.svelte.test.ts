import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Span } from './model';
import { sealed } from './guard';

let host: HTMLElement;
let node: HTMLElement;

const CODES = ['1111-aaaa', '2222-bbbb', '3333-cccc'].join('\n');

beforeEach(() => {
	host = document.createElement('div');
	host.append('before ');
	node = document.createElement('span');
	node.textContent = CODES;
	host.append(node, ' after');
	document.body.appendChild(host);
});

afterEach(() => {
	document.getSelection()?.removeAllRanges();
	host.remove();
});

function select(start: [Node, number], end: [Node, number]) {
	const range = document.createRange();
	range.setStart(...start);
	range.setEnd(...end);
	const selection = document.getSelection();
	if (!selection) throw new Error('this document has no selection');
	selection.removeAllRanges();
	selection.addRange(range);
}

/** What a copy on the node asks Rust for, with the selection as it stands. */
function selected(): Span | null {
	const copy = vi.fn();
	const detach = sealed(copy, vi.fn())(node);
	node.dispatchEvent(new ClipboardEvent('copy', { bubbles: true, cancelable: true }));
	detach?.();
	expect(copy).toHaveBeenCalledTimes(1);
	return copy.mock.calls[0][0] as Span | null;
}

/** The same, for a selection from `start` to `end`. */
function across(start: [Node, number], end: [Node, number]): Span | null {
	select(start, end);
	return selected();
}

const text = () => node.firstChild as Text;

it('counts a selection inside the value from the start of the value', () => {
	expect(across([text(), 20], [text(), 29])).toEqual({ from: 20, to: 29 });
	expect(across([text(), 0], [text(), 1])).toEqual({ from: 0, to: 1 });
	expect(across([text(), 9], [text(), 10]), 'the line break alone').toEqual({
		from: 9,
		to: 10
	});
});

/** All of it is the whole field, and nothing selected is the whole field too:
 * a copy with no selection is a copy of the value. */
it('asks for the whole value when all of it or none of it is selected', () => {
	expect(across([text(), 0], [text(), CODES.length])).toBeNull();
	expect(across([node, 0], [node, 1]), 'the node itself, end to end').toBeNull();
	expect(across([text(), 4], [text(), 4]), 'a collapsed selection').toBeNull();
	document.getSelection()?.removeAllRanges();
	expect(selected(), 'nothing selected').toBeNull();
});

/** A selection that runs out of the value into the chrome around it is cut at
 * the value's edges: the chrome is not part of any field. */
it('cuts a selection that runs past the value at its edges', () => {
	const before = host.firstChild as Text;
	const after = host.lastChild as Text;

	expect(across([before, 2], [text(), 9])).toEqual({ from: 0, to: 9 });
	expect(across([text(), 20], [after, 3])).toEqual({ from: 20, to: CODES.length });
	expect(across([node, 0], [text(), 4])).toEqual({ from: 0, to: 4 });
	expect(across([text(), 4], [node, 1])).toEqual({ from: 4, to: CODES.length });
});

/** A selection somewhere else entirely names no part of this value, so a copy
 * that reaches it anyway is a copy of the whole, as its Copy button would be. */
it('asks for the whole value when the selection does not reach it', () => {
	const before = host.firstChild as Text;
	expect(across([before, 0], [before, 3])).toBeNull();
});

/** The positions are the text node's, in UTF-16 code units, which is what
 * Rust counts in: an emoji is two of them. */
it('counts in the units the text node counts in', () => {
	node.textContent = 'a🔐b';
	expect(across([text(), 1], [text(), 3])).toEqual({ from: 1, to: 3 });
	expect(across([text(), 3], [text(), 4])).toEqual({ from: 3, to: 4 });
});

/** A node with nothing shown in it holds nothing to protect: it is every
 * row's node while its value is hidden, and the system's copy of the chrome
 * around it is the reader's. */
it('leaves a node that holds no value to the system', () => {
	node.textContent = '';
	const copy = vi.fn();
	const detach = sealed(copy, vi.fn())(node);
	select([host.firstChild as Text, 0], [host.lastChild as Text, 3]);

	for (const kind of ['copy', 'contextmenu']) {
		const event = new MouseEvent(kind, { bubbles: true, cancelable: true });
		node.dispatchEvent(event);
		expect(event.defaultPrevented, kind).toBe(false);
	}
	detach?.();
	expect(copy).not.toHaveBeenCalled();
});

/** What the guard does to the system's own handling of the node, and that it
 * lets go of the document again. The menu is Coffer's, about the part
 * selected, and nothing under it hears the right-click: the row around the
 * value would draw a menu of its own. */
it('takes the copy, the cut, the menu and the drag, and lets go when detached', () => {
	const copy = vi.fn();
	const menu = vi.fn();
	const detach = sealed(copy, vi.fn(), menu)(node);
	const heard = vi.fn();
	host.addEventListener('contextmenu', heard);
	select([text(), 10], [text(), 19]);

	for (const kind of ['copy', 'cut']) {
		const event = new ClipboardEvent(kind, { bubbles: true, cancelable: true });
		node.dispatchEvent(event);
		expect(event.defaultPrevented, kind).toBe(true);
	}
	expect(copy).toHaveBeenCalledTimes(2);
	expect(copy).toHaveBeenCalledWith({ from: 10, to: 19 });

	for (const kind of ['contextmenu', 'dragstart']) {
		const event = new MouseEvent(kind, { bubbles: true, cancelable: true });
		node.dispatchEvent(event);
		expect(event.defaultPrevented, kind).toBe(true);
	}
	expect(menu).toHaveBeenCalledTimes(1);
	expect(menu.mock.calls[0][1]).toEqual({ from: 10, to: 19 });
	expect(heard, 'the row under the value drew a menu as well').not.toHaveBeenCalled();

	detach?.();
	const after = new ClipboardEvent('copy', { bubbles: true, cancelable: true });
	node.dispatchEvent(after);
	expect(after.defaultPrevented).toBe(false);
	expect(copy).toHaveBeenCalledTimes(2);
});

/** The selection here starts in the label before the value and ends in the
 * chrome after it, so every event lands on the chrome and none on the value's
 * node. What is on the pasteboard afterwards is the value's part, from Rust,
 * and nothing else. */
it('takes a copy that lands on the chrome around a selected value', () => {
	const copy = vi.fn();
	const detach = sealed(copy, vi.fn())(node);
	select([host.firstChild as Text, 2], [text(), 9]);

	for (const kind of ['copy', 'cut']) {
		const event = new ClipboardEvent(kind, { bubbles: true, cancelable: true });
		host.dispatchEvent(event);
		expect(event.defaultPrevented, kind).toBe(true);
	}
	select([host.firstChild as Text, 2], [host.lastChild as Text, 3]);
	host.dispatchEvent(new ClipboardEvent('copy', { bubbles: true, cancelable: true }));
	detach?.();
	expect(copy.mock.calls).toEqual([[{ from: 0, to: 9 }], [{ from: 0, to: 9 }], [null]]);
});

/** Right-click on the label inside a selection that reaches into the value:
 * WebKit's menu there is about the whole selection, Look Up and Share
 * included, so it is Coffer's about the value's part instead. And a drag
 * started on the label carries the whole selection. */
it("draws Coffer's menu, not WebKit's, on chrome inside a selection that reaches a value", () => {
	const label = document.createElement('b');
	label.textContent = 'Codes ';
	host.prepend(label);
	const menu = vi.fn();
	const detach = sealed(vi.fn(), vi.fn(), menu)(node);
	select([label.firstChild as Text, 0], [text(), 4]);

	const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
	const heard = vi.fn();
	host.addEventListener('contextmenu', heard);
	label.dispatchEvent(event);
	expect(event.defaultPrevented).toBe(true);
	expect(heard).not.toHaveBeenCalled();
	expect(menu.mock.calls).toEqual([[event, { from: 0, to: 4 }]]);

	const drag = new MouseEvent('dragstart', { bubbles: true, cancelable: true });
	host.dispatchEvent(drag);
	expect(drag.defaultPrevented).toBe(true);
	detach?.();
});

/** A press of the secondary button, and then a right-click: what WebKit does
 * to the selection in between, and the menu it ends in. */
function rightClick(
	on: Node,
	between: () => void = () => {},
	init: MouseEventInit = { button: 2 }
) {
	on.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true, ...init }));
	between();
	const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
	on.dispatchEvent(event);
	return event;
}

/** Where a selection is, in the value's own positions, or nothing. */
function where(): [number, number] | null {
	const selection = document.getSelection();
	if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null;
	const range = selection.getRangeAt(0);
	return [range.startOffset, range.endOffset];
}

/** WebKit selects the word under a right-click before the page hears of the
 * menu. The reader chose nothing, so the menu is about the value whole, and
 * the word WebKit chose is let go. */
it('copies the whole value when the right-click made the only selection', () => {
	const menu = vi.fn();
	const detach = sealed(vi.fn(), vi.fn(), menu)(node);

	const event = rightClick(node, () => select([text(), 10], [text(), 14]));

	expect(event.defaultPrevented).toBe(true);
	expect(menu.mock.calls).toEqual([[event, null]]);
	expect(where(), 'the word WebKit chose is still selected').toBeNull();
	detach?.();
});

it('is about the reader’s selection when the right-click lands on it', () => {
	const menu = vi.fn();
	const detach = sealed(vi.fn(), vi.fn(), menu)(node);
	select([text(), 4], [text(), 9]);

	const event = rightClick(node);

	expect(menu.mock.calls).toEqual([[event, { from: 4, to: 9 }]]);
	expect(where()).toEqual([4, 9]);
	detach?.();
});

/** The pointer was beside the reader's selection, and WebKit moved it to the
 * word under the pointer. The menu is about the value whole, and the reader's
 * selection is where they left it. */
it('is about the whole value, and keeps the reader’s selection, when the right-click lands beside it', () => {
	const menu = vi.fn();
	const detach = sealed(vi.fn(), vi.fn(), menu)(node);
	select([text(), 0], [text(), 4]);

	const event = rightClick(node, () => select([text(), 20], [text(), 24]));

	expect(menu.mock.calls).toEqual([[event, null]]);
	expect(where()).toEqual([0, 4]);
	detach?.();
});

/** Control and a click is a right-click on a Mac. A plain press in between is
 * not, and leaves the menu to the selection as it stands. */
it('treats a Ctrl+click as a right-click, and a plain press as none', () => {
	const menu = vi.fn();
	const detach = sealed(vi.fn(), vi.fn(), menu)(node);

	rightClick(node, () => select([text(), 10], [text(), 14]), { button: 0, ctrlKey: true });
	expect(menu.mock.calls.at(-1)?.[1]).toBeNull();
	expect(where()).toBeNull();

	rightClick(node, () => select([text(), 10], [text(), 14]), { button: 0 });
	expect(menu.mock.calls.at(-1)?.[1]).toEqual({ from: 10, to: 14 });
	detach?.();
});

/** A value selected, and a right-click on something else altogether: the
 * guard leaves the event to whatever is there, which draws its own menu, and
 * the selection is not touched. WebKit's menu is drawn by nobody: it would be
 * about the selection, and the selection is on the value. */
it('leaves a right-click elsewhere to what was clicked while a value is selected', () => {
	const other = document.createElement('button');
	other.textContent = 'a row';
	document.body.appendChild(other);
	const menu = vi.fn();
	const detach = sealed(vi.fn(), vi.fn(), menu)(node);
	const heard = vi.fn();
	document.body.addEventListener('contextmenu', heard);
	select([text(), 4], [text(), 9]);

	const event = rightClick(other);

	expect(event.defaultPrevented, 'WebKit’s menu over the selected value').toBe(true);
	expect(heard).toHaveBeenCalledTimes(1);
	expect(menu).not.toHaveBeenCalled();
	expect(where()).toEqual([4, 9]);
	document.body.removeEventListener('contextmenu', heard);
	other.remove();
	detach?.();
});

/** The search field, or any field being typed in, keeps WebKit's menu for its
 * Cut, Copy and Paste. Not while part of a value is still selected and the
 * right-click left the selection where it was: WebKit's menu would be about
 * the value then, Look Up and Share included. Whether a button went down
 * first or VoiceOver opened the menu makes no difference. */
it('draws no menu in a field being typed in while a value is still selected', () => {
	const search = document.createElement('input');
	search.type = 'search';
	document.body.appendChild(search);
	const menu = vi.fn();
	const detach = sealed(vi.fn(), vi.fn(), menu)(node);
	select([text(), 4], [text(), 9]);

	const clicked = rightClick(search);
	expect(clicked.defaultPrevented, 'WebKit’s menu over the selected value').toBe(true);
	const spoken = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
	search.dispatchEvent(spoken);
	expect(spoken.defaultPrevented, 'a menu VoiceOver opened').toBe(true);
	expect(menu).not.toHaveBeenCalled();
	expect(where(), 'the reader’s selection').toEqual([4, 9]);

	// The selection went into the field with the press: WebKit's menu is
	// about what is typed there, and stays.
	const moved = rightClick(search, () => document.getSelection()?.removeAllRanges());
	expect(moved.defaultPrevented).toBe(false);
	const plain = rightClick(search);
	expect(plain.defaultPrevented, 'nothing selected').toBe(false);
	expect(menu).not.toHaveBeenCalled();

	search.remove();
	detach?.();
});

/** A right-click on the row around a selection that runs from its label into
 * the value is not on the selection: the pointer is on the pane around it. The
 * row draws its own menu, the value's is not drawn, and WebKit's is not
 * either, the selection being on the value. */
it('leaves a right-click on the row around a selected value to the row', () => {
	const label = document.createElement('b');
	label.textContent = 'Codes ';
	host.prepend(label);
	const menu = vi.fn();
	const detach = sealed(vi.fn(), vi.fn(), menu)(node);
	const heard = vi.fn();
	host.addEventListener('contextmenu', heard);
	select([label.firstChild as Text, 0], [text(), 4]);

	const event = rightClick(host);

	expect(heard, 'the row did not hear it').toHaveBeenCalledTimes(1);
	expect(menu).not.toHaveBeenCalled();
	expect(event.defaultPrevented).toBe(true);
	const selection = document.getSelection()?.getRangeAt(0);
	expect([selection?.startContainer, selection?.endOffset]).toEqual([label.firstChild, 4]);
	host.removeEventListener('contextmenu', heard);
	detach?.();
});

/** A selection that ends exactly at the value's first character covers none
 * of it, and one elsewhere on the page has nothing to do with it: the page's
 * own copy and menu are left alone, and Rust is not asked for anything. */
it('leaves a selection that covers none of a value to the system', () => {
	const copy = vi.fn();
	const detach = sealed(copy, vi.fn())(node);

	for (const [start, end] of [
		[
			[host.firstChild as Text, 0],
			[node, 0]
		],
		[
			[host.firstChild as Text, 0],
			[host.firstChild as Text, 6]
		],
		[
			[node, 1],
			[host.lastChild as Text, 3]
		]
	] as [[Node, number], [Node, number]][]) {
		select(start, end);
		for (const kind of ['copy', 'contextmenu', 'dragstart']) {
			const event = new MouseEvent(kind, { bubbles: true, cancelable: true });
			host.dispatchEvent(event);
			expect(event.defaultPrevented, kind).toBe(false);
		}
	}
	detach?.();
	expect(copy).not.toHaveBeenCalled();
});

/** Two values in one selection - a username and a password, read across the
 * rows of an entry - are two fields, and Rust copies one field at a time. So
 * nothing is copied at all: half of it through Rust would be a copy the reader
 * did not make, and all of it through the system is the copy this stops. The
 * reader is told, or they paste what the pasteboard held before. */
it('copies nothing from a selection that reaches two values, and says so', () => {
	const second = document.createElement('span');
	second.textContent = 'hunter2';
	host.append(second, ' end');

	const between = document.createElement('i');
	between.textContent = ' and ';
	second.before(between);

	const first = vi.fn();
	const other = vi.fn();
	const refused = vi.fn();
	const menu = vi.fn();
	const detachFirst = sealed(first, refused, menu)(node);
	const detachOther = sealed(other, refused, menu)(second);
	select([text(), 20], [second.firstChild as Text, 3]);

	for (const kind of ['copy', 'cut', 'dragstart']) {
		const event = new MouseEvent(kind, { bubbles: true, cancelable: true });
		host.dispatchEvent(event);
		expect(event.defaultPrevented, kind).toBe(true);
	}
	// A right-click on the chrome between the two, inside the selection: no
	// menu of WebKit's, and none of Coffer's, which copies one value.
	const pointed = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
	between.dispatchEvent(pointed);
	expect(pointed.defaultPrevented).toBe(true);
	expect(menu).not.toHaveBeenCalled();
	expect(first).not.toHaveBeenCalled();
	expect(other).not.toHaveBeenCalled();
	// Said once for each copy and cut, and never for a menu or a drag, which
	// the reader sees did not happen.
	expect(refused.mock.calls).toEqual([
		[{ code: 'refused', message: 'Select one value at a time to copy it.' }],
		[{ code: 'refused', message: 'Select one value at a time to copy it.' }]
	]);

	// One value going does not take the other's guard with it.
	detachFirst?.();
	const copy = new ClipboardEvent('copy', { bubbles: true, cancelable: true });
	host.dispatchEvent(copy);
	expect(copy.defaultPrevented).toBe(true);
	expect(other).toHaveBeenCalledWith({ from: 0, to: 3 });
	expect(refused).toHaveBeenCalledTimes(2);

	detachOther?.();
});
