import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import Editable from './Editable.svelte';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => host.remove());

function show(props: {
	value?: string;
	multiline?: boolean;
	onCommit: (value: string) => Promise<boolean>;
}) {
	return mount(Editable, {
		target: host,
		props: { value: '', label: 'Login', ...props }
	});
}

/** A commit the vault took. */
function taken() {
	return vi.fn().mockResolvedValue(true);
}

function field(): HTMLInputElement {
	const found = host.querySelector('input, textarea');
	if (!found) throw new Error('there is no field');
	return found as HTMLInputElement;
}

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

	field().value = 'deploy2';
	field().dispatchEvent(new Event('blur'));
	expect(onCommit).toHaveBeenCalledWith('deploy2');

	return unmount(component);
});

it('puts back what was there when the reader presses escape', () => {
	const onCommit = taken();
	const component = show({ value: 'deploy', onCommit });
	flushSync();

	field().value = 'something else';
	field().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
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

	field().value = 'one line';
	field().dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
	field().dispatchEvent(new Event('blur'));
	expect(onCommit).toHaveBeenCalledWith('one line');
	unmount(line);

	const note = show({ value: '', multiline: true, onCommit });
	flushSync();
	expect(host.querySelector('textarea')).not.toBeNull();

	field().value = 'first\nsecond';
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

	field().value = 'no\u{b}';
	field().dispatchEvent(new Event('blur'));

	await vi.waitFor(() => expect(onCommit).toHaveBeenCalledWith('no\u{b}'));
	await vi.waitFor(() => expect(field().value).toBe('deploy'));

	return unmount(component);
});

/** A database Coffer will not write back offers nothing to change. */
it('draws a value it cannot change as a value and not as a field', () => {
	const component = mount(Editable, {
		target: host,
		props: { value: 'deploy', label: 'Login', readonly: true, onCommit: taken() }
	});
	flushSync();

	expect(host.querySelector('input')).toBeNull();
	expect(host.textContent).toContain('deploy');

	return unmount(component);
});
