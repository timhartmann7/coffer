/**
 * Telling Rust the reader is there.
 *
 * The idle timer is Rust's, and it has to be told when somebody is at the
 * machine or a vault would lock itself under their hands. This is the throttle
 * on that message and nothing else: it runs on every keystroke and every click,
 * and one message each would be thousands of messages to say one thing.
 *
 * There is deliberately no countdown here any more. The number used to tick in
 * the corner of the status bar once a second, which is a thing that moves for no
 * reason a reader can act on.
 */

import { stirred } from './ipc';

/** How often the window may tell Rust the reader is there. Long enough that a
 * paragraph of typing is one message, and far shorter than the shortest timeout
 * on offer. */
const APART = 15_000;

export class Presence {
	/** When the reader was last reported. Negative infinity so that the first
	 * thing they do is always worth saying. */
	#last = Number.NEGATIVE_INFINITY;

	/** The reader did something. */
	stir(now: number = Date.now()): void {
		if (now - this.#last < APART) return;
		this.#last = now;
		void stirred().catch(() => {});
	}
}
