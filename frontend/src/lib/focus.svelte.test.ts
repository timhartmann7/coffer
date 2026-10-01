import { flushSync } from 'svelte';
import { expect, it } from 'vitest';
import { focused, writing } from './focus.svelte';

/** Cmd+Backspace in a field deletes to the start of the line, so the item
 * that takes the key is out of the way for exactly as long as the focus is in
 * one - and not a moment after it leaves for a button. Whatever reads the
 * answer, as the window's report to the menu bar does, reads it again each
 * time the focus moves. */
it('greys out Move to Recycle Bin from the moment the focus is in a field until it leaves', () => {
	const field = document.createElement('input');
	const notes = document.createElement('textarea');
	const button = document.createElement('button');
	document.body.append(field, notes, button);
	window.addEventListener('focusin', focused);
	window.addEventListener('focusout', focused);
	const seen: boolean[] = [];
	const stop = $effect.root(() => {
		$effect(() => {
			seen.push(writing());
		});
	});
	const now = () => {
		flushSync();
		return seen.at(-1);
	};

	try {
		expect(now()).toBe(false);
		field.focus();
		expect(now()).toBe(true);
		notes.focus();
		expect(now(), 'between two fields the key was handed back').toBe(true);
		button.focus();
		expect(now(), 'a button kept the item greyed').toBe(false);
		notes.focus();
		expect(now()).toBe(true);
		notes.blur();
		expect(now(), 'the focus left the field and the item stayed grey').toBe(false);
	} finally {
		stop();
		window.removeEventListener('focusin', focused);
		window.removeEventListener('focusout', focused);
		for (const each of [field, notes, button]) each.remove();
	}
});

/** WebKit tells nothing it takes off the screen that it lost the focus. The
 * unlock screen's password field goes with the focus in it when the vault
 * opens, and an answer kept from the last focus event greyed Move to Recycle
 * Bin until the reader happened to click into a field and out again. */
it('says nobody is writing once the field with the focus has gone', () => {
	const field = document.createElement('input');
	document.body.append(field);
	field.focus();
	focused();
	expect(writing()).toBe(true);

	field.remove();

	expect(writing(), 'a field that is gone is still being written in').toBe(false);
});
