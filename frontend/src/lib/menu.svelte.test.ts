import { flushSync } from 'svelte';
import { afterEach, expect, it } from 'vitest';
import { answer, applying, cover, run } from './menu.svelte';
import type { Command } from './model';

/** What each test put on the registry, taken back after it however it ended.
 * The registry is the module's, and a screen one test left answering would
 * answer in every test after it. */
const left: (() => void)[] = [];

afterEach(() => {
	for (const gone of left.splice(0)) gone();
});

/** A screen's ways, with every press written down. */
function screen(commands: Command[], when: () => boolean = () => true) {
	const pressed: Command[] = [];
	const gone = answer(
		Object.fromEntries(
			commands.map((command) => [command, { run: () => pressed.push(command), when }])
		)
	);
	left.push(gone);
	return { pressed, gone };
}

/** A sheet over the window, writing down every time it was put away into
 * `said`. */
function sheet(said: string[]) {
	const gone = cover(() => said.push('sheet away'));
	left.push(gone);
	return gone;
}

/** A screen drawn over another is the one the reader is looking at: the pane
 * over the list, the settings over the vault. */
it('runs the screen drawn last that can still do it', () => {
	const under = screen(['find']);
	let able = true;
	const over = screen(['find'], () => able);

	run('find');
	expect(over.pressed).toEqual(['find']);
	expect(under.pressed).toEqual([]);

	able = false;
	run('find');
	expect(under.pressed, 'a screen that could not do it took the press').toEqual(['find']);
	expect(over.pressed).toEqual(['find']);
});

/** Rust greys out what it was last told, and a choice can cross with a change
 * on its way: it is asked again when it arrives. */
it('runs nothing for a choice nothing on the window can do any more', () => {
	let able = true;
	const only = screen(['copyPassword'], () => able);
	expect(applying()).toContain('copyPassword');

	able = false;
	run('copyPassword');
	run('newEntry');

	expect(only.pressed).toEqual([]);
});

/** A choice that crossed with a change is grey by the time it lands, and does
 * nothing. A sheet it put away first would be one closed for nothing: the
 * reader reading the list of keys loses it to a press that did not happen. */
it('keeps a sheet up for a choice nothing on the window can do any more', () => {
	const said: string[] = [];
	sheet(said);
	screen(['copyPassword'], () => false);

	run('copyPassword');
	run('newFolder');

	expect(said).toEqual([]);
});

it('forgets a screen the moment it goes', () => {
	const gone = screen(['lock', 'newFolder']);
	gone.gone();

	expect(applying()).toEqual([]);
	run('lock');
	run('newFolder');
	expect(gone.pressed).toEqual([]);
});

/** The bar is told each item once, in its own order, whichever screens answer
 * it and in whatever order they were drawn. */
it('lists what applies in the bar’s order, each item once', () => {
	screen(['moveToBin', 'copyLogin', 'settings']);
	screen(['shortcuts', 'settings', 'lock']);
	screen(['find'], () => false);

	expect(applying()).toEqual(['settings', 'lock', 'copyLogin', 'moveToBin', 'shortcuts']);
});

/** A sheet over the window makes the page under it inert, so it has to be
 * gone before what was chosen is done there. Keyboard Shortcuts is the one
 * choice that is not about the page under it. */
it('puts a sheet away before doing what was chosen, and keeps it for Keyboard Shortcuts', () => {
	const said: string[] = [];
	const away = sheet(said);
	left.push(
		answer({
			find: { run: () => said.push('find') },
			shortcuts: { run: () => said.push('shortcuts') }
		})
	);

	run('shortcuts');
	expect(said).toEqual(['shortcuts']);

	run('find');
	expect(said).toEqual(['shortcuts', 'sheet away', 'find']);

	away();
	run('find');
	expect(said, 'a sheet that went was put away again').toEqual([
		'shortcuts',
		'sheet away',
		'find',
		'find'
	]);
});

/**
 * A press on a button takes the focus out of the field being written in, and
 * leaving is when a field writes what was typed into it. A choice from the
 * menu bar takes no focus, so the field is left for it - before the choice
 * runs, because New Entry takes the field off the screen and WebKit tells a
 * field it removes nothing. Keyboard Shortcuts is the exception: its sheet
 * takes the focus itself and gives it back.
 */
it('takes the focus out of a field being written in before the choice runs', () => {
	const notes = document.createElement('textarea');
	const button = document.createElement('button');
	document.body.append(notes, button);
	const said: string[] = [];
	notes.addEventListener('blur', () => said.push('left the field'));
	left.push(
		answer({
			newEntry: {
				run: () => said.push(document.activeElement === notes ? 'ran in the field' : 'ran')
			},
			shortcuts: { run: () => said.push('sheet') }
		})
	);
	try {
		notes.focus();
		run('shortcuts');
		expect(document.activeElement, 'Keyboard Shortcuts took the field away').toBe(notes);

		run('newEntry');
		expect(said).toEqual(['sheet', 'left the field', 'ran']);

		button.focus();
		run('newEntry');
		expect(document.activeElement, 'a button was taken for a field').toBe(button);
	} finally {
		notes.remove();
		button.remove();
	}
});

/** A search field writes nothing when it is left, so a choice from the menu
 * bar leaves the focus in it: the reader who copies a login from the menu
 * while searching goes on typing where they were, rather than into nothing. */
it('leaves the focus in a search field, which writes nothing when it is left', () => {
	const search = document.createElement('input');
	search.setAttribute('role', 'searchbox');
	document.body.append(search);
	const said: string[] = [];
	left.push(answer({ copyLogin: { run: () => said.push('copied') } }));
	try {
		search.focus();
		run('copyLogin');
		expect(said).toEqual(['copied']);
		expect(document.activeElement, 'the search field lost the focus to a copy').toBe(search);
	} finally {
		search.remove();
	}
});

/** Every screen answers from an effect. One that made the others run again
 * would loop until Svelte gave up. */
it('survives screens answering from inside their effects', () => {
	const stop = $effect.root(() => {
		$effect(() => answer({ find: { run: () => {} } }));
		$effect(() => answer({ lock: { run: () => {} } }));
		$effect(() => {
			// A reader of the list, the way the window's report is one.
			applying();
		});
	});
	for (let round = 0; round < 5; round += 1) flushSync();

	expect(applying()).toEqual(['lock', 'find']);
	stop();
	expect(applying()).toEqual([]);
});
