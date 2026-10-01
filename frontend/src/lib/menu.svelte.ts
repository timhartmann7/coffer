/**
 * The page's half of the menu bar.
 *
 * Every item of Coffer's own in the menu bar is a press routed to the page, and
 * the screen that draws the button the item stands for answers it with that
 * button's own function, so nothing on the screen is done two ways. Screens
 * say what they can do with `answer`, the window tells Rust which items apply
 * (`report` in `greying.ts`, with `applying`), and a choice that arrives is run
 * by `run` - asked again first, because a choice can cross with a change on
 * its way and Rust only greys out what it was last told.
 */

import { flushSync, untrack } from 'svelte';
import { typing } from './keys';
import { COMMANDS, type Command } from './model';

/** What a screen does for an item, and whether it can now. `run` is the
 * function the screen's own button runs, and `when` the condition the button
 * is drawn on, read again at every choice. */
type Way = { run: () => void; when?: () => boolean };
type Ways = Partial<Record<Command, Way>>;

/** Every screen's ways, oldest first. */
let screens = $state.raw<Ways[]>([]);
/** What goes before anything chosen: a sheet over the window. Nothing is
 * drawn from it, so it is not state. */
let covers: (() => void)[] = [];

/**
 * A screen says what it can do; what this answers with takes it back. Called
 * from a screen's `$effect`, and untracked, so that one screen answering does
 * not run every other screen's effect again.
 */
export function answer(ways: Ways): () => void {
	untrack(() => (screens = [...screens, ways]));
	return () => (screens = screens.filter((screen) => screen !== ways));
}

/** A sheet over the window. It goes before anything chosen in the menu bar
 * happens under it - except Keyboard Shortcuts, which is a sheet itself. */
export function cover(away: () => void): () => void {
	covers = [...covers, away];
	return () => (covers = covers.filter((each) => each !== away));
}

/** The way an item is answered now: the newest screen that can, because a
 * screen drawn over another was drawn after it. */
function way(command: Command): Way | undefined {
	for (let at = screens.length - 1; at >= 0; at -= 1) {
		const found = screens[at][command];
		if (found && (found.when?.() ?? true)) return found;
	}
	return undefined;
}

/**
 * Runs what the reader chose, if anything on the window can still do it, the
 * way a press on its button would.
 *
 * A sheet goes first, and is gone from the page before the choice runs: what
 * the choice does - the focus put in the search field, a name to be typed - is
 * done to a page the sheet had made inert.
 *
 * Then the field being written in is left. A press on a button takes the focus
 * out of the field before the button runs, and leaving is when a field writes
 * what was typed into it; a choice from the menu bar takes no focus, and New
 * Entry would otherwise take the field off the screen with the value still in
 * it - WebKit tells nothing it removes that it lost the focus. Keyboard
 * Shortcuts leaves the field alone: its sheet takes the focus and gives it
 * back. So does a search field, whose leaving writes nothing: the reader who
 * copies a login from the menu while searching goes on typing in it.
 */
export function run(command: Command): void {
	const found = way(command);
	if (!found) return;
	if (command !== 'shortcuts') {
		if (covers.length > 0) {
			for (const away of covers) away();
			flushSync();
		}
		const field = document.activeElement;
		if (field instanceof HTMLElement && writes(field)) field.blur();
	}
	found.run();
}

/** Whether the focus is in a field that writes what was typed into it when it
 * is left: every field the reader writes in but a search field. */
function writes(field: HTMLElement): boolean {
	return typing(field) && field.getAttribute('role') !== 'searchbox';
}

/** The items the window can do now, in the bar's order. */
export function applying(): Command[] {
	return COMMANDS.filter((command) => way(command) !== undefined);
}
