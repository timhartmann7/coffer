/**
 * Coffer's menus under the pointer, from the page's side: asking Rust for one
 * in place of WebKit's, marking what it is about while it is open, and running
 * what was chosen from it.
 *
 * WebKit's own menu is a web page's - Look Up, Translate, Search, Share - and
 * each of those hands what is under the pointer to another application. So a
 * right-click anywhere in the window is prevented, and where the window knows
 * what was clicked it asks Rust for Coffer's menu instead, naming the thing by
 * id. Only a field the reader types into keeps WebKit's: its Cut, Copy, Paste
 * and spelling are about their typing.
 *
 * One menu at a time. Each is asked for under a number, and the item chosen
 * comes back through the window's way in (`outside.ts`) with that number: only
 * an item of the menu asked for last is run, and once, whether it arrives
 * before Rust answers that the menu has closed or after.
 */

import { contextMenu } from './ipc';
import { writable } from './keys';
import type { Chosen, Subject } from './model';

/** What a thing on the screen is, for `marked`. */
type Marked = 'entry' | 'folder' | 'file' | 'bin';

const NONE: ReadonlySet<string> = new Set();

/** How many menus this page has asked for. */
let count = 0;
/** What runs an item of the menu asked for last, by that menu's number, and
 * the element that asked for it. */
let asked: { serial: number; answer: (item: Chosen) => void; thing: Element | null } | null = null;
/** What the menu asked for last is about, while it is open. */
let pointed = $state.raw<{ serial: number; keys: ReadonlySet<string> }>({ serial: 0, keys: NONE });

/** One thing's key, which the menu marks and `marked` reads. */
function key(kind: Marked, ids: readonly string[]): string {
	return `${kind}:${ids.join('\u0000')}`;
}

/** What a menu about `subject` marks on the screen: the rows, the folder or
 * the file. A field and a revealed value are under the pointer already. */
function keys(subject: Subject): ReadonlySet<string> {
	switch (subject.kind) {
		case 'entry':
			return new Set([key('entry', [subject.entry])]);
		case 'entries':
			return new Set(subject.entries.map((entry) => key('entry', [entry])));
		case 'folder':
			return new Set([key('folder', [subject.group])]);
		case 'bin':
			return new Set([key('bin', [])]);
		case 'file':
			return new Set([key('file', [subject.entry, subject.name])]);
		default:
			return NONE;
	}
}

/**
 * A right-click on something the window knows: Coffer's menu about `subject`
 * where the pointer is, in place of WebKit's. `answer` runs the item chosen,
 * once, and `failed` says why there is no menu - the thing went from the vault
 * a moment before, or the menu could not be drawn.
 *
 * Only the innermost thing that knows what was clicked asks: a field inside a
 * row is the field's menu, not the row's. In a field the reader types into
 * nothing is asked, and WebKit's menu is left to it.
 *
 * What asked is remembered by its element: the one the handler is on, or for a
 * value - whose menu the document's guard draws - the one under the pointer.
 */
export function offer(
	event: MouseEvent,
	subject: Subject,
	answer: (item: Chosen) => void,
	failed: (thrown: unknown) => void
): void {
	if (writable(event.target)) return;
	event.preventDefault();
	event.stopPropagation();
	count += 1;
	const serial = count;
	const thing =
		event.currentTarget instanceof Element
			? event.currentTarget
			: event.target instanceof Element
				? event.target
				: null;
	asked = { serial, answer, thing };
	pointed = { serial, keys: keys(subject) };
	contextMenu(serial, subject, { x: event.clientX, y: event.clientY })
		.catch(failed)
		.finally(() => {
			// The menu has closed. What it was about stays marked only while it
			// is the one asked for last: one asked for since marks its own.
			if (pointed.serial === serial) pointed = { serial, keys: NONE };
		});
}

/**
 * An item chosen from the menu numbered `serial`. Run once, and only for the
 * menu asked for last: an item of one the page has moved on from, or of a
 * menu a page before this one asked for, does nothing. Nor does one whose
 * thing has left the screen since - a field of the entry the pane showed
 * before - because what would run it is about whatever stands there now.
 */
export function chosen(serial: number, item: Chosen): void {
	if (asked === null || asked.serial !== serial) return;
	const { answer, thing } = asked;
	asked = null;
	if (thing !== null && !thing.isConnected) return;
	answer(item);
}

/**
 * Whether an item is about the field `field` of `entry`, which is what a
 * field's row and a value ask before they run it: the row may be about
 * another entry or another field by the time the item arrives, while its
 * element is still on the screen.
 */
export function about(
	item: Chosen,
	entry: string,
	field: string
): item is Extract<Chosen, { field: string }> {
	return 'field' in item && item.entry === entry && item.field === field;
}

/** A right-click on anything else: no menu, unless it is in a field the reader
 * types into. The window's own listener, under every screen. */
export function plain(event: MouseEvent): void {
	if (!writable(event.target)) event.preventDefault();
}

/** The screen that asked is going: nothing chosen afterwards is run, and
 * nothing stays marked. */
export function forget(): void {
	asked = null;
	pointed = { serial: count, keys: NONE };
}

/** Whether the menu open now is about this thing, which is drawn marked
 * (`[data-menu]` in `app.css`). */
export function marked(kind: Marked, ...ids: string[]): boolean {
	return pointed.keys.has(key(kind, ids));
}
