/**
 * How long the open vault has left, and telling Rust the reader is there.
 *
 * The number itself is Rust's. This ticks it down between messages so the
 * status line moves once a second without asking anything once a second, and
 * puts it back to whatever Rust says whenever it does ask.
 *
 * The two jobs are one class on purpose. A countdown that reset the deadline
 * every time it drew itself would be an idle timer that never fires, so what
 * may reset it - real input, and rarely - is decided in the same place that
 * knows what the number means.
 */

import { stirred } from './ipc';

/** How often the window may tell Rust the reader is there. Long enough that a
 * paragraph of typing is one message, and far shorter than the shortest
 * timeout on offer. */
const APART = 15_000;

/** Twelve steps and an empty one, written out because Tailwind cannot read a
 * class that was computed. */
const WIDTHS = [
	'w-0',
	'w-1/12',
	'w-2/12',
	'w-3/12',
	'w-4/12',
	'w-5/12',
	'w-6/12',
	'w-7/12',
	'w-8/12',
	'w-9/12',
	'w-10/12',
	'w-11/12',
	'w-full'
];

export class Countdown {
	/** Seconds until the vault locks itself, or `null` when none is open. */
	left = $state<number | null>(null);
	/** Bumped whenever a fresh number arrives from Rust, so that the bar beside
	 * it can be drawn again: a CSS animation does not replay on its own. */
	round = $state(0);

	#ticking: ReturnType<typeof setInterval> | null = null;
	/** When the reader was last reported. Negative infinity so that the first
	 * thing they do is always worth saying. */
	#last = Number.NEGATIVE_INFINITY;
	/** What the whole timeout was when this round started, so the bar knows what
	 * it is a fraction of. */
	#full = 0;

	/**
	 * Takes the number Rust just gave, and starts counting it down.
	 *
	 * `null` stops the clock: the vault is locked, and a countdown that went on
	 * running would be counting down to something that already happened.
	 */
	sync(seconds: number | null): void {
		this.stop();
		this.left = seconds;
		if (seconds === null) return;

		this.#full = Math.max(1, seconds);
		this.round += 1;
		this.#ticking = setInterval(() => {
			if (this.left === null) return;
			// Never past zero. The moment it reaches zero is the moment Rust
			// destroys this window, so there is nothing to draw after it.
			this.left = Math.max(0, this.left - 1);
		}, 1000);
	}

	/**
	 * The reader did something.
	 *
	 * Throttled, and the throttle is the point: this runs on every keystroke and
	 * every click, and a message to Rust for each of them would be thousands of
	 * messages to say one thing.
	 */
	stir(now: number = Date.now()): void {
		if (now - this.#last < APART) return;
		this.#last = now;
		void stirred()
			.then((seconds) => this.sync(seconds))
			.catch(() => {});
	}

	/**
	 * How much of the bar is left, as one of a fixed set of classes.
	 *
	 * A width and not an animation: Tailwind reads class names out of the
	 * source, so a computed one compiles to nothing, and the window's
	 * `style-src 'self'` means a width cannot be set from script either. Twelve
	 * steps is as fine as a sixty-four pixel bar can show anyway.
	 */
	width(): string {
		const left = this.left;
		if (left === null) return WIDTHS[0];
		const step = Math.round((left / this.#full) * 12);
		return WIDTHS[Math.min(12, Math.max(0, step))];
	}

	stop(): void {
		if (this.#ticking !== null) {
			clearInterval(this.#ticking);
			this.#ticking = null;
		}
	}
}
