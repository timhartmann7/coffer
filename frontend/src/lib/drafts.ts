/**
 * What the reader is typing into an entry and has not finished, told to Rust
 * as it is typed.
 *
 * A value is written when its field is left. Until then it is in this window
 * and nowhere else, and a lock destroys this window - on triggers that cannot
 * wait for it. The Mac going to sleep, the screen locking and Coffer quitting
 * all arrive on the thread the window is drawn on, and neither a message into
 * this page nor its answer can get through while that thread is busy locking.
 * So nothing asks the page to finish first. The page says what is in a field a
 * moment after the last key, every second or so while the keys keep coming, and
 * at once when the window loses focus, and the lock writes the last of it into
 * the vault before wiping it.
 *
 * Every word about a field - what is in it, that it was taken back, the value
 * written - carries a number from one count that only goes up. Tauri runs
 * commands side by side, so a draft sent before its field was written can
 * arrive after it, and Rust drops a word older than the last one it heard about
 * the field, its entry or the whole vault. The count starts at the moment this
 * window was built, in microseconds, so that a word from a window a lock
 * destroyed is older than anything the next one says.
 *
 * What is typed is read out of the field when Rust is told, not when the key is
 * pressed, so nothing but the field holds it in the meantime.
 *
 * Text typed into a field where it stands is what the reader wants there, and
 * a lock writes it into the field. A new value typed in a Change field is not:
 * the reader has not saved it, and it may be half a password or the wrong one
 * pasted. Rust is told which it is, and a lock keeps a new value beside the
 * one it was for rather than over it.
 */

import { SvelteMap } from 'svelte/reactivity';
import { draft as tell } from './ipc';

/** How long after the last key the field is told to Rust. */
const PAUSE = 250;

/** How long the reader can go on typing before the field is told to Rust
 * anyway. A pause alone never comes for somebody typing steadily, a key every
 * fifth of a second, and a lid closed then would cost everything since they
 * last stopped. With this, it costs at most this much of the typing. */
const LONGEST = 1000;

let count = Date.now() * 1000;

function next(): number {
	count += 1;
	return count;
}

/** A field of an entry, and whether the database protects it. */
export interface Place {
	entry: string;
	field: string;
	protect: boolean;
}

interface Typing {
	place: Place;
	/** Whether it is a new value typed in a Change field of its own. */
	beside: boolean;
	/** What the field holds now. */
	read: () => string;
	/** The pause before Rust is told, while one is running. */
	waiting: ReturnType<typeof setTimeout> | null;
	/** The latest Rust is told, however the keys go on: set by the first key
	 * it has not heard and not moved by the ones after it. */
	due: ReturnType<typeof setTimeout> | null;
	/** Whether Rust has been told anything since the field was last written or
	 * taken back, and so has something to be told to let go of. */
	told: boolean;
}

/**
 * Every field holding text that has not been written, by entry and name.
 *
 * Drawn: a field in here carries the mark that says so. A key is added by the
 * first key typed and not by each one after it, so the mark is drawn once.
 */
const typing = new SvelteMap<string, Typing>();

/** Every word on its way to Rust: drafts, and the values that finish them. */
const sending = new Set<Promise<unknown>>();

function key(place: Place): string {
	return JSON.stringify([place.entry, place.field]);
}

/**
 * Keeps a word in `sending` until it has arrived, whichever way it arrives. A
 * draft refused - the vault locked meanwhile - has nobody to tell.
 *
 * Also what the entry pane sends when something typed that is not a field's
 * value is left - a tag, a field's new name - so that `flush` waits for those
 * too: a copy of the entry asked for at the same moment would otherwise be
 * answered beside them, and could hold the entry as it was before them.
 */
export function track<T>(sent: Promise<T>): Promise<T> {
	sending.add(sent);
	const arrived = () => sending.delete(sent);
	sent.then(arrived, arrived);
	return sent;
}

/** Rust is hearing the field now, or never will: neither clock may run on. */
function hush(held: Typing) {
	if (held.waiting) clearTimeout(held.waiting);
	if (held.due) clearTimeout(held.due);
	held.waiting = null;
	held.due = null;
}

function say(held: Typing) {
	hush(held);
	held.told = true;
	const { entry, field, protect } = held.place;
	track(tell(entry, field, held.read(), protect, held.beside, next()));
}

/** Starts the pause again, and the longest wait if it is not running. */
function wait(held: Typing) {
	if (held.waiting) clearTimeout(held.waiting);
	held.waiting = setTimeout(() => say(held), PAUSE);
	held.due ??= setTimeout(() => say(held), LONGEST);
}

function forget(name: string) {
	const held = typing.get(name);
	if (held) hush(held);
	typing.delete(name);
}

function hear(place: Place, read: () => string, beside: boolean) {
	const name = key(place);
	let held = typing.get(name);
	if (!held) {
		held = { place, beside, read, waiting: null, due: null, told: false };
		typing.set(name, held);
	}
	held.place = place;
	held.read = read;
	wait(held);
}

/** The reader typed into a field where it stands. `read` answers with what is
 * in it. */
export function typed(place: Place, read: () => string): void {
	hear(place, read, false);
}

/** The reader typed a new value for a field in a Change field of its own.
 * `read` answers with what is in that. */
export function replacing(place: Place, read: () => string): void {
	hear(place, read, true);
}

/** What was typed into a field was taken back: Escape, Cancel, Discard, or the
 * field left as it was, emptied, or gone with its text still in it. */
export function drop(place: Place): void {
	const name = key(place);
	const held = typing.get(name);
	if (!held) return;
	forget(name);
	if (held.told) {
		track(tell(place.entry, place.field, null, place.protect, held.beside, next()));
	}
}

/**
 * Writes a value into a field, and finishes what was typed there.
 *
 * `write` is given the number the write carries, which is newer than any draft
 * of the field already sent, so a draft still on its way is dropped when it
 * arrives rather than written over the value at the next lock.
 */
export function settle<T>(place: Place, write: (sequence: number) => Promise<T>): Promise<T> {
	forget(key(place));
	return track(write(next()));
}

/**
 * Entries are being deleted, or - for `null` - the file read again, and what
 * was typed into them goes with them. Answers with the number the command
 * carries, which is how Rust knows which drafts came before it.
 */
export function release(entries: readonly string[] | null): number {
	const named = entries === null ? null : new Set(entries);
	const going = [...typing].filter(([, held]) => named === null || named.has(held.place.entry));
	for (const [name] of going) forget(name);
	return next();
}

/**
 * Tells Rust everything typed that it has not heard yet, now, and waits until
 * every word on its way has arrived - the values being written included, and
 * every other change `track` was handed.
 *
 * The window losing focus is when this happens on its own: the reader may be
 * about to close the lid. A lock the reader asked for waits for it, so that the
 * lock finds everything that was in the window.
 */
export async function flush(): Promise<void> {
	for (const held of typing.values()) {
		if (held.waiting !== null) say(held);
	}
	await Promise.allSettled([...sending]);
}

/** Whether a field holds text that has not been written. */
export function unfinished(place: Place): boolean {
	return typing.has(key(place));
}
