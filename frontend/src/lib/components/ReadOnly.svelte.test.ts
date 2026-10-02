import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { ReadOnlyBecause } from '$lib/model';
import ReadOnly from './ReadOnly.svelte';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => host.remove());

function draw(
	because: ReadOnlyBecause,
	copyable = true,
	onCopy = vi.fn().mockResolvedValue(undefined)
) {
	const component = mount(ReadOnly, { target: host, props: { because, copyable, onCopy } });
	flushSync();
	return component;
}

function word(): HTMLButtonElement {
	const found = host.querySelector<HTMLButtonElement>('button[aria-controls="read-only-note"]');
	if (!found) throw new Error('there is no Read only button');
	return found;
}

function note(): HTMLElement | null {
	return host.querySelector('#read-only-note');
}

function pressed() {
	word().click();
	flushSync();
}

/** Each reason has its own sentence, in words a reader can act on, and none
 * of them names another product: the formats have names of their own. */
it('says why for every reason', () => {
	const said = new Map<ReadOnlyBecause, string>([
		['snapshot', 'This is a backup, opened to look at.'],
		['place', 'This vault is kept somewhere that will not take a file'],
		['kdb', 'This vault is in KDB, an older format Coffer reads and does not write'],
		['kdbx3', 'This vault is in KDBX 3 and holds files']
	]);

	for (const [because, sentence] of said) {
		const component = draw(because);
		expect(note(), 'the note is open before it is asked for').toBeNull();
		expect(word().getAttribute('aria-expanded')).toBe('false');

		pressed();
		expect(note()?.textContent).toContain(sentence);
		expect(note()?.textContent).not.toMatch(/keepass|1password|bitwarden/i);
		expect(word().getAttribute('aria-expanded')).toBe('true');

		pressed();
		expect(note(), 'the button does not close what it opened').toBeNull();
		unmount(component);
	}
});

/** A copy is offered only where Rust says one can be written: never for a
 * format whose bytes a copy would carry. Pressed, it closes the note and writes
 * the copy. */
it('offers a copy only where one can be written', () => {
	const refused = draw('kdbx3', false);
	pressed();
	expect(note()?.querySelector('button')).toBeNull();
	unmount(refused);

	const onCopy = vi.fn().mockResolvedValue(undefined);
	const offered = draw('place', true, onCopy);
	pressed();
	const copy = note()?.querySelector('button');
	expect(copy?.textContent?.trim()).toBe('Save a copy somewhere else…');
	copy?.click();
	flushSync();
	expect(onCopy).toHaveBeenCalledTimes(1);
	expect(note()).toBeNull();
	expect(document.activeElement).toBe(word());
	unmount(offered);
});

/** Escape closes the note and goes no further: the window reads Escape as
 * closing the entry, and putting a note away is not putting the entry away.
 * The focus goes back to the word. With the note shut, Escape is the window's. */
it('closes on Escape, gives the focus back and keeps the key from the window', () => {
	const heard = vi.fn();
	window.addEventListener('keydown', heard);
	try {
		const component = draw('snapshot');
		pressed();
		const copy = note()?.querySelector('button');
		copy?.focus();

		copy?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
		flushSync();
		expect(note()).toBeNull();
		expect(document.activeElement).toBe(word());
		expect(heard, 'the window heard the Escape the note took').not.toHaveBeenCalled();

		word().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
		expect(heard, 'an Escape with nothing open never reached the window').toHaveBeenCalledTimes(1);
		unmount(component);
	} finally {
		window.removeEventListener('keydown', heard);
	}
});

/** The note is a disclosure: when the focus leaves it - Tab past the copy, a
 * press anywhere else - it goes. Moving between its own two buttons does not
 * close it. */
it('closes when the focus leaves it, and not while it stays inside', () => {
	const outside = document.createElement('button');
	document.body.appendChild(outside);
	try {
		const component = draw('place');
		pressed();
		const copy = note()?.querySelector('button');

		word().dispatchEvent(new FocusEvent('focusout', { bubbles: true, relatedTarget: copy }));
		flushSync();
		expect(note(), 'moving into the note closed it').not.toBeNull();

		copy?.dispatchEvent(new FocusEvent('focusout', { bubbles: true, relatedTarget: outside }));
		flushSync();
		expect(note()).toBeNull();
		unmount(component);
	} finally {
		outside.remove();
	}
});

/** WebKit gives a button no focus when it is clicked. Opened by a pointer, the
 * note takes the focus onto its button all the same, so Escape - which goes
 * where the focus is - closes the note and goes no further, and a press
 * anywhere else, which takes the focus away, closes it too. */
it('holds the focus when a pointer opens it, so Escape and a press elsewhere close it', () => {
	const heard = vi.fn();
	window.addEventListener('keydown', heard);
	try {
		const component = draw('snapshot');
		(document.activeElement as HTMLElement | null)?.blur();
		const click = () => {
			word().dispatchEvent(new MouseEvent('click', { bubbles: true }));
			flushSync();
		};

		click();
		expect(note()).not.toBeNull();
		expect(document.activeElement, 'the note holds nothing that hears a key').toBe(word());
		document.activeElement?.dispatchEvent(
			new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
		);
		flushSync();
		expect(note()).toBeNull();
		expect(heard, 'the window heard the Escape the note took').not.toHaveBeenCalled();

		click();
		expect(note()).not.toBeNull();
		word().blur();
		flushSync();
		expect(note(), 'a press elsewhere left the note up').toBeNull();
		unmount(component);
	} finally {
		window.removeEventListener('keydown', heard);
	}
});

/** Pressing the copy is not the focus leaving the note: its button keeps the
 * focus where it is, so the note is still there for the click to land on. */
it('keeps the focus where it is when the copy is pressed with a pointer', () => {
	const component = draw('place');
	pressed();
	const copy = note()?.querySelector('button');
	const down = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
	copy?.dispatchEvent(down);
	expect(down.defaultPrevented).toBe(true);
	unmount(component);
});
