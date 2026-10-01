/**
 * The lifetime of a value that has been shown on the screen.
 *
 * A revealed value is not state. It is written straight into the text node that
 * shows it and it is wiped from there again, so it never reaches a variable, a
 * store, `localStorage`, a URL or an attribute where something else could read
 * it or a crash could write it down. What is state here is how long is left.
 */

import { SvelteMap } from 'svelte/reactivity';
import { reveal } from './ipc';

/** How long a revealed value stays on the screen. The bar that drains beside
 * it is set to the same half minute in `app.css`. */
const SECONDS = 30;

/**
 * Writes a value into the node that shows it, and wipes it again with `''`.
 *
 * The node is text to read and never a field: a value written into a field was
 * a value one stray key could change. It is one text node inside one element
 * and nothing else, which is also what lets a selection in it be counted
 * without reading it (see `guard.ts`). Every screen that puts a secret on the
 * screen goes through here, so there is one place that knows what "on the
 * screen and nowhere else" means.
 */
export function place(node: HTMLElement, value: string): void {
	node.textContent = value;
}

/** Every value on the screen, by the entry it is a value of. */
const shown = new SvelteMap<Revealed, string>();

/**
 * Takes every value of an entry off the screen: the entry has come back
 * changed.
 *
 * A value shown before a change is not what the entry holds after it. Left up,
 * it was the old password read out to a guest under the label of the new one,
 * for the rest of its half minute - and a part of it selected and copied was
 * cut out of the new value in Rust at positions counted on the old text.
 */
export function conceal(entry: string): void {
	for (const [revealed, of] of shown) if (of === entry) revealed.hide();
}

export class Revealed {
	/** Whether a value is on the screen right now. */
	showing = $state(false);
	/** Seconds before it is wiped. */
	left = $state(0);

	#node: HTMLElement | null = null;
	#countdown: ReturnType<typeof setInterval> | null = null;
	/** Which reveal is the current one. A value that arrives after the screen
	 * has moved on has nowhere to go. */
	#asked = 0;

	/**
	 * Puts the value into `node` and starts counting down.
	 *
	 * `read` is what fetches it: a reveal of the entry as it is, or of one of
	 * its previous versions. Whatever it is, the value goes straight into the
	 * node and is never held anywhere this class can be asked for it.
	 */
	async show(
		node: HTMLElement,
		entry: string,
		field: string,
		read: (entry: string, field: string) => Promise<string> = reveal
	): Promise<void> {
		const asked = ++this.#asked;
		const value = await read(entry, field);

		// Hidden, or asked again, while this one was in flight: the node it was
		// going into may already be off the screen, and writing there would
		// leave the value in a element nothing can wipe again.
		if (asked !== this.#asked) return;

		this.hide();
		this.#asked = asked;
		place(node, value);
		this.#node = node;
		shown.set(this, entry);
		this.showing = true;
		this.left = SECONDS;

		this.#countdown = setInterval(() => {
			this.left -= 1;
			if (this.left <= 0) this.hide();
		}, 1000);
	}

	/**
	 * Takes it off the screen. Called by the timer, by the reader, and by every
	 * component that holds one when it is destroyed - leaving the window is
	 * leaving the value behind.
	 */
	hide(): void {
		this.#asked += 1;
		if (this.#countdown !== null) {
			clearInterval(this.#countdown);
			this.#countdown = null;
		}
		if (this.#node) {
			place(this.#node, '');
			this.#node = null;
		}
		shown.delete(this);
		this.showing = false;
		this.left = 0;
	}
}
