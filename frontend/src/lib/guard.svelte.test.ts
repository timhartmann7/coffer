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
	const detach = sealed(copy)(node);
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

/** A node with nothing shown in it has nothing to count. */
it('names nothing in a node that holds no value', () => {
	node.textContent = '';
	expect(selected()).toBeNull();
});

/** What the attachment does to the system's own handling of the node, and
 * that it lets go of the node again. */
it('takes the copy, the cut, the menu and the drag, and lets go when detached', () => {
	const copy = vi.fn();
	const detach = sealed(copy)(node);
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

	detach?.();
	const after = new ClipboardEvent('copy', { bubbles: true, cancelable: true });
	node.dispatchEvent(after);
	expect(after.defaultPrevented).toBe(false);
	expect(copy).toHaveBeenCalledTimes(2);
});
