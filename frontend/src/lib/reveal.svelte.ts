/**
 * The lifetime of a value that has been shown on the screen.
 *
 * A revealed value is not state. It is written straight into the text node that
 * shows it and it is wiped from there again, so it never reaches a variable, a
 * store, `localStorage`, a URL or an attribute where something else could read
 * it or a crash could write it down. What is state here is how long is left.
 */

import { reveal } from './ipc';

/** How long a revealed value stays on the screen. The bar that drains beside
 * it is set to the same half minute in `app.css`. */
const SECONDS = 30;

/**
 * Writes a value into the node that shows it, and wipes it again with `''`.
 *
 * A field being edited is an input and a field being read is a span. Either
 * way the value is a property of one DOM node and of nothing else. Every screen
 * that puts a secret on the screen goes through here, so there is one place
 * that knows what "on the screen and nowhere else" means.
 */
export function place(node: HTMLElement, value: string): void {
	if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement) {
		node.value = value;
	} else {
		node.textContent = value;
	}
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
		this.showing = false;
		this.left = 0;
	}
}
