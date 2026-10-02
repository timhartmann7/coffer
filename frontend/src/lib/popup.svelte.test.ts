import { afterEach, expect, it } from 'vitest';
import { leaves, step } from './popup';

afterEach(() => {
	document.body.replaceChildren();
});

/** A button that opens a list of three options with a heading among them,
 * the way the settings chip and "+ Entry" draw theirs. */
function drawn() {
	const opener = document.createElement('button');
	const list = document.createElement('div');
	const heading = document.createElement('p');
	heading.textContent = 'Heading';
	heading.tabIndex = -1;
	const lines = ['First', 'Second', 'Third'].map((name) => {
		const line = document.createElement('button');
		line.setAttribute('role', 'option');
		line.textContent = name;
		return line;
	});
	list.append(lines[0], heading, lines[1], lines[2]);
	document.body.append(opener, list);
	return { opener, list, heading, lines };
}

const at = () => document.activeElement?.textContent ?? null;

/** From the button that opened the list, down lands on the first line and up
 * on the last. Where the focus is not a line, its place in the list is
 * nowhere, and stepping from that as if it were a line lands up on the line
 * before the last. */
it('steps into the list from outside, down to the first line and up to the last', () => {
	const { opener, list } = drawn();
	opener.focus();
	step(list, 'option', 'ArrowDown');
	expect(at()).toBe('First');

	opener.focus();
	step(list, 'option', 'ArrowUp');
	expect(at(), 'up from the button skipped the last line').toBe('Third');
});

/** Round from either end, and past whatever in the list is not a line. */
it('steps round from either end, over a heading', () => {
	const { list, lines } = drawn();
	lines[0].focus();
	step(list, 'option', 'ArrowDown');
	expect(at(), 'the heading took a step').toBe('Second');
	step(list, 'option', 'ArrowDown');
	expect(at()).toBe('Third');
	step(list, 'option', 'ArrowDown');
	expect(at(), 'down from the last line').toBe('First');
	step(list, 'option', 'ArrowUp');
	expect(at(), 'up from the first line').toBe('Third');
	step(list, 'option', 'ArrowUp');
	expect(at()).toBe('Second');
});

/** Lines of another role are not this list's, a list with no line leaves the
 * focus where it is, and so does a list not drawn yet. */
it('leaves the focus alone for a list with no line of the role, or no list', () => {
	const { opener, list } = drawn();
	opener.focus();
	step(list, 'menuitem', 'ArrowDown');
	expect(document.activeElement).toBe(opener);
	step(undefined, 'option', 'ArrowDown');
	expect(document.activeElement).toBe(opener);

	const empty = document.createElement('div');
	document.body.append(empty);
	step(empty, 'option', 'ArrowUp');
	expect(document.activeElement).toBe(opener);
});

/** The focus moving between the parts of a control is not the focus leaving
 * it; going anywhere else, or to nothing, is. */
it('tells the focus leaving the control from the focus moving inside it', () => {
	const { opener, list, lines } = drawn();
	const control = document.createElement('span');
	control.append(opener, list);
	document.body.append(control);
	const elsewhere = document.createElement('button');
	document.body.append(elsewhere);

	const moving = (to: EventTarget | null) => new FocusEvent('focusout', { relatedTarget: to });
	expect(leaves(moving(lines[1]), control)).toBe(false);
	expect(leaves(moving(opener), control)).toBe(false);
	expect(leaves(moving(elsewhere), control)).toBe(true);
	expect(leaves(moving(null), control), 'a press on nothing kept the list').toBe(true);
	expect(leaves(moving(lines[1]), undefined)).toBe(true);
});
