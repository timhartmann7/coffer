import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Place } from '$lib/drafts';
import type { Stubbed } from '$lib/stubbed';
import Editable from './Editable.svelte';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('$lib/ipc'), async (real) =>
	Object.assign(ipc, (await import('$lib/stubbed')).stubbed(await real()))
);

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.draft.mockResolvedValue(undefined);
});

afterEach(() => host.remove());

/** Where a field's typing goes. Its own entry each time, because what is typed
 * is kept for the life of the window and the window here is the whole file. */
let made = 0;
function place(): Place {
	made += 1;
	return { entry: `entry-${made}`, field: 'UserName', protect: false };
}

function show(props: {
	value?: string;
	multiline?: boolean;
	breaks?: boolean;
	draft?: Place;
	onCommit: (value: string) => Promise<boolean>;
}) {
	return mount(Editable, {
		target: host,
		props: { value: '', label: 'Login', draft: place(), ...props }
	});
}

/** A commit the vault took. */
function taken() {
	return vi.fn().mockResolvedValue(true);
}

function field(): HTMLInputElement | HTMLTextAreaElement {
	const found = host.querySelector('input, textarea');
	if (!found) throw new Error('there is no field');
	return found as HTMLInputElement | HTMLTextAreaElement;
}

/** What a reader typing does: the value changes, and the field hears it. */
function type(value: string) {
	field().value = value;
	field().dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
}

function key(key: string, over: KeyboardEventInit = {}): KeyboardEvent {
	const pressed = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...over });
	field().dispatchEvent(pressed);
	return pressed;
}

const CODES = ['1111-aaaa', '2222-bbbb', '3333-cccc', '4444-dddd'].join('\n');

/**
 * Every edit that reaches Rust writes a version and moves a modification time.
 * Clicking through an entry and clicking away again is not an edit, and a
 * screen that reported one would fill the history with nothing.
 */
it('writes nothing when the value comes back unchanged', () => {
	const onCommit = taken();
	const component = show({ value: 'deploy', onCommit });
	flushSync();

	field().dispatchEvent(new Event('blur'));
	expect(onCommit).not.toHaveBeenCalled();

	type('deploy2');
	field().dispatchEvent(new Event('blur'));
	expect(onCommit).toHaveBeenCalledWith('deploy2');

	return unmount(component);
});

/**
 * The defect itself. A one-line input handed a value with line breaks keeps it
 * without them, and a commit that compared what the field held with what the
 * vault holds saw a difference and wrote it: ten recovery codes clicked on and
 * clicked away from went back to the file as one line. What the field made of
 * the value on its own is not something the reader wrote.
 */
it('writes nothing the reader did not type, whatever the field made of the value', () => {
	const onCommit = taken();
	const component = show({ value: 'deploy', onCommit });
	flushSync();

	// What WebKit does to a value it cannot hold: the field now differs from
	// the vault, and nobody typed anything.
	field().value = 'depl oy';
	field().dispatchEvent(new Event('focus'));
	field().dispatchEvent(new Event('blur'));
	expect(onCommit).not.toHaveBeenCalled();

	return unmount(component);
});

/** The attack for the same defect from the other side: a field of the reader's
 * own holding lines, clicked into and out of again. */
it('keeps a field of lines as it is when it is only clicked through', () => {
	for (const value of [CODES, CODES.replaceAll('\n', '\r\n'), 'one\rtwo', '\n\n']) {
		const onCommit = taken();
		const component = show({ value, breaks: true, onCommit });
		flushSync();

		expect(field(), 'a value with lines is drawn where they cannot survive').toBeInstanceOf(
			HTMLTextAreaElement
		);
		// What WebKit's text area reads back: every CRLF and lone CR as a line
		// feed. The field differs from the vault, and nobody typed anything.
		field().value = value.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
		field().dispatchEvent(new Event('focus'));
		field().dispatchEvent(new Event('blur'));
		expect(onCommit, JSON.stringify(value)).not.toHaveBeenCalled();

		unmount(component);
	}
});

/** A value from another client may carry lines in a field Coffer writes on one
 * line, a login or an address. It is drawn in lines rather than flattened, so
 * an edit to it is an edit to what is really there. */
it('draws any value with a line break where the break survives', () => {
	const onCommit = taken();
	const component = show({ value: 'first\nsecond', onCommit });
	flushSync();

	expect(field()).toBeInstanceOf(HTMLTextAreaElement);
	expect(field().value).toBe('first\nsecond');

	type('first\nsecond\nthird');
	field().dispatchEvent(new Event('blur'));
	expect(onCommit).toHaveBeenCalledWith('first\nsecond\nthird');

	return unmount(component);
});

/**
 * A field of lines takes Return as the next line, and Cmd+Return finishes it.
 * An edit of one code out of ten writes all ten, each on its own line.
 */
it('edits one line of several and writes back every line', () => {
	const onCommit = taken();
	const component = show({ value: CODES, breaks: true, onCommit });
	flushSync();

	expect(key('Enter').defaultPrevented, 'Return finished a value in lines').toBe(false);

	const fixed = CODES.replace('3333-cccc', '3333-cccd');
	type(fixed);
	expect(key('Enter', { metaKey: true }).defaultPrevented).toBe(true);
	field().dispatchEvent(new Event('blur'));

	expect(onCommit).toHaveBeenCalledTimes(1);
	expect(onCommit).toHaveBeenCalledWith(fixed);
	expect((onCommit.mock.calls[0][0] as string).split('\n')).toHaveLength(4);

	return unmount(component);
});

/**
 * A field of the reader's own on one line is still a field that can take
 * lines. A paste of several keeps them - an input would have run them into one
 * - and from then on the field is one of lines, growing with them.
 */
it("keeps the breaks of what is pasted into a one-line field of the reader's own", () => {
	const onCommit = taken();
	const component = show({ value: '', breaks: true, onCommit });
	flushSync();

	expect(field()).toBeInstanceOf(HTMLTextAreaElement);
	expect(field().getAttribute('rows')).toBe('1');
	expect(field().getAttribute('wrap')).toBe('off');

	// What a paste is to the field: the text arrives, with its breaks.
	type(CODES);
	expect(field().getAttribute('rows')).toBe('4');
	expect(field().getAttribute('wrap')).toBe('soft');
	expect(key('Enter').defaultPrevented, 'Return finished a pasted list').toBe(false);

	field().dispatchEvent(new Event('blur'));
	expect(onCommit).toHaveBeenCalledWith(CODES);

	return unmount(component);
});

/** On one line Return finishes, the way every one-line field on a Mac does,
 * and Option+Return is the Mac's way of putting a break into one. A Return
 * that ends an input method's composition belongs to the input method. */
it('finishes one line on Return, and leaves Option+Return and a composition alone', () => {
	const onCommit = taken();
	const component = show({ value: 'one', breaks: true, onCommit });
	flushSync();

	expect(key('Enter', { altKey: true }).defaultPrevented).toBe(false);
	expect(key('Enter', { isComposing: true }).defaultPrevented).toBe(false);
	expect(key('Enter', { keyCode: 229 }).defaultPrevented).toBe(false);

	type('one line');
	expect(key('Enter').defaultPrevented).toBe(true);
	field().dispatchEvent(new Event('blur'));
	expect(onCommit).toHaveBeenCalledWith('one line');

	return unmount(component);
});

/** A field grows with its lines and stops somewhere: a megabyte of them would
 * otherwise be a field the pane has to be scrolled a megabyte past. */
it('grows with its lines up to a point, and takes all of them', () => {
	const onCommit = taken();
	const many = Array.from({ length: 50_000 }, (_, at) => `code ${at}`).join('\n');
	const component = show({ value: many, breaks: true, onCommit });
	flushSync();

	expect(Number(field().getAttribute('rows'))).toBe(20);
	expect(field().value).toHaveLength(many.length);

	return unmount(component);
});

it('puts back what was there when the reader presses escape', () => {
	const onCommit = taken();
	const component = show({ value: 'deploy', onCommit });
	flushSync();

	type('something else');
	key('Escape');
	flushSync();

	expect(field().value).toBe('deploy');
	field().dispatchEvent(new Event('blur'));
	expect(onCommit).not.toHaveBeenCalled();

	return unmount(component);
});

/** A value out of a database can be anything at all, newlines included. A
 * single-line field finishes on Enter; a note keeps the line break. */
it('finishes a line on enter and keeps one in a note', () => {
	const onCommit = taken();
	const line = show({ value: '', onCommit });
	flushSync();

	type('one line');
	key('Enter');
	field().dispatchEvent(new Event('blur'));
	expect(onCommit).toHaveBeenCalledWith('one line');
	unmount(line);

	const note = show({ value: '', multiline: true, onCommit });
	flushSync();
	expect(host.querySelector('textarea')).not.toBeNull();
	expect(field().getAttribute('rows'), 'a note is a place to write several lines').toBe('4');

	expect(key('Enter').defaultPrevented).toBe(false);
	type('first\nsecond');
	field().dispatchEvent(new Event('blur'));
	expect(onCommit).toHaveBeenLastCalledWith('first\nsecond');

	return unmount(note);
});

/** A value is written as text wherever it is drawn. A field that put it in the
 * markup would run it. */
it('holds a value that is markup as a value and not as markup', () => {
	const component = show({ value: '<img src=x onerror="alert(1)">', onCommit: taken() });
	flushSync();

	expect(host.querySelector('img')).toBeNull();
	expect(field().value).toBe('<img src=x onerror="alert(1)">');
	expect(host.innerHTML).not.toContain('onerror');

	return unmount(component);
});

/**
 * A value the vault refused is a value the vault does not have. Leaving it in
 * the field would leave the reader looking at something that is not in their
 * database and believing it is.
 */
it('puts back a value the vault would not take', async () => {
	const onCommit = vi.fn().mockResolvedValue(false);
	const component = show({ value: 'deploy', onCommit });
	flushSync();

	type('no\u{b}');
	field().dispatchEvent(new Event('blur'));

	await vi.waitFor(() => expect(onCommit).toHaveBeenCalledWith('no\u{b}'));
	await vi.waitFor(() => expect(field().value).toBe('deploy'));

	// And the put-back value is not an edit of the reader's either.
	field().dispatchEvent(new Event('blur'));
	expect(onCommit).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/**
 * The Return that confirms an input method's conversion is the input
 * method's. WebKit commonly sends it with `isComposing` already false and the
 * key code 229, and a login typed through one was finished, and written,
 * half converted.
 */
it('leaves a Return that ends a composition to the input method in a field on one line', () => {
	const onCommit = taken();
	const component = show({ value: 'deploy', onCommit });
	flushSync();
	expect(field().tagName).toBe('INPUT');
	field().focus();

	type('deploy-2');
	for (const composition of [{ isComposing: true }, { keyCode: 229 }]) {
		expect(key('Enter', composition).defaultPrevented, JSON.stringify(composition)).toBe(false);
		expect(document.activeElement, 'the composition finished the field').toBe(field());
	}
	expect(onCommit).not.toHaveBeenCalled();

	expect(key('Enter').defaultPrevented).toBe(true);

	return unmount(component);
});

/** The Escape that cancels a conversion is the input method's as well, and
 * put the field back to what the vault holds with the edit thrown away. */
it('leaves an Escape that cancels a composition to the input method', () => {
	const onCommit = taken();
	const component = show({ value: 'deploy', onCommit });
	flushSync();
	field().focus();

	type('deploy-2');
	for (const composition of [{ isComposing: true }, { keyCode: 229 }]) {
		key('Escape', composition);
		flushSync();
		expect(field().value, JSON.stringify(composition)).toBe('deploy-2');
		expect(document.activeElement, 'the composition put the field away').toBe(field());
	}

	key('Escape');
	flushSync();
	expect(field().value).toBe('deploy');
	expect(onCommit).not.toHaveBeenCalled();

	return unmount(component);
});

/** What is typed is told to Rust as a draft of the place the field was given,
 * and Escape lets go of it. */
it('tells what is typed to Rust as a draft of its field, and lets go of it on Escape', async () => {
	const at = place();
	const component = show({ value: 'deploy', draft: at, onCommit: taken() });
	flushSync();

	type('deploy-2');
	await vi.waitFor(() =>
		expect(ipc.draft).toHaveBeenCalledWith(
			at.entry,
			'UserName',
			'deploy-2',
			false,
			false,
			expect.any(Number)
		)
	);
	key('Escape');
	expect(ipc.draft).toHaveBeenLastCalledWith(
		at.entry,
		'UserName',
		null,
		false,
		false,
		expect.any(Number)
	);

	return unmount(component);
});

/** A database Coffer will not write back offers nothing to change. */
it('draws a value it cannot change as a value and not as a field', () => {
	const component = mount(Editable, {
		target: host,
		props: { value: 'deploy', label: 'Login', readonly: true, draft: place(), onCommit: taken() }
	});
	flushSync();

	expect(host.querySelector('input')).toBeNull();
	expect(host.textContent).toContain('deploy');

	return unmount(component);
});
