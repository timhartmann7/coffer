/**
 * The sentence in the corner of the vault screen: what just happened, for a
 * few seconds, and the one way to take it back while that is still on offer.
 */

import { RISE, span } from './motion';

export type Notice = {
	message: string;
	/** What the notice is about, which is the icon it carries: a copy, a
	 * failure, something gone to the bin or out of the file, or something
	 * moved between folders. */
	kind: 'copied' | 'failed' | 'removed' | 'moved';
	/** Takes back what the notice is about, while that is still on offer. */
	undo?: () => void;
};

/**
 * How long a notice stays up: one that reports, one that says what was
 * copied, and one that offers to take a change back.
 *
 * The last is the longest because it asks for a decision, and the pointer
 * has the width of the window to cross to reach it.
 */
export const REPORTED = 6000;
export const COPIED = 5000;
export const UNDOABLE = 8000;

export class Notices {
	/** The notice on the screen. */
	notice = $state<Notice | null>(null);
	/** Whether the notice on the screen is on its way out. Nothing animates an
	 * element that has already gone, so it says so first and goes after. */
	leaving = $state(false);
	/** Which notice the toast is. Each is drawn afresh, so that a new sentence
	 * rises into the corner rather than changing its words in place. */
	told = $state(0);

	/**
	 * The way to take back what the notice on the screen reports, while it
	 * still can be.
	 *
	 * Nothing is drawn from it, so it is not state: it is the answer to whether
	 * the offer still stands, asked by the notice's button, by Cmd+Z and by
	 * everything that withdraws it. An undo clears it before its first wait, so
	 * the button and the key pressed together run it once.
	 */
	#offered: (() => Promise<void>) | null = null;
	#fading: ReturnType<typeof setTimeout> | null = null;
	/** Where an undo that failed is reported. */
	readonly #failed: (thrown: unknown) => void;

	constructor(failed: (thrown: unknown) => void) {
		this.#failed = failed;
	}

	/** Whether something is on offer to be taken back. */
	get offering(): boolean {
		return this.#offered !== null;
	}

	/**
	 * Puts a sentence on the screen and takes it away again.
	 *
	 * The clock of whatever notice was there is stopped first: without that, a
	 * timer left over from the last one takes this one away early, and the class
	 * that was fading it out arrives already on it. Whatever that notice offered
	 * to take back goes with it, because an undo belongs to the sentence that
	 * says what it undoes.
	 */
	tell(next: Notice, after = REPORTED): void {
		this.clear();
		this.told += 1;
		this.notice = next;
		this.#fading = setTimeout(() => this.#go(), after);
	}

	/** Says why something did not happen. */
	warn(message: string): void {
		this.tell({ message, kind: 'failed' });
	}

	/**
	 * Says what was just done and offers to take it back, for eight seconds.
	 *
	 * The offer is withdrawn by the notice going and by a newer notice, because
	 * an undo belongs to the sentence that says what it undoes. A lock takes the
	 * whole window down, and the offer with it. Nothing else withdraws it: every
	 * undo acts on what it is about by its id, whichever entry the pane shows by
	 * then, and never opens anything over the reader's later choice, and one the
	 * vault has moved on from is refused by Rust at the press and said to be.
	 *
	 * `kind` is what was done: something taken away, by default, or moved.
	 */
	offer(message: string, undo: () => Promise<void>, kind: Notice['kind'] = 'removed'): void {
		this.tell({ message, kind, undo: () => void this.#takeBack(undo) }, UNDOABLE);
		this.#offered = undo;
	}

	/** Takes back whatever is on offer, from a key rather than the button. */
	takeBack(): void {
		if (this.#offered !== null) void this.#takeBack(this.#offered);
	}

	/**
	 * Runs an undo once, and only while it is still the one on offer.
	 *
	 * The notice starts going as the undo starts: its one question has been
	 * answered, and whatever the undo has to say next is a notice of its own.
	 */
	async #takeBack(undo: () => Promise<void>): Promise<void> {
		if (this.#offered !== undo) return;
		this.clear();
		this.#go();
		try {
			await undo();
		} catch (thrown) {
			this.#failed(thrown);
		}
	}

	/**
	 * Takes the notice away, once it has finished going.
	 *
	 * The second of two waits: the first is how long the sentence is worth
	 * reading, this one is the length of the movement that takes it off the
	 * screen, and `motion.svelte.test.ts` is what keeps it and the stylesheet
	 * saying the same number. A notice that has started going offers nothing
	 * any more, whatever it said.
	 */
	#go(): void {
		this.#offered = null;
		this.leaving = true;
		this.#fading = setTimeout(() => {
			this.notice = null;
			this.leaving = false;
			this.#fading = null;
		}, span(RISE));
	}

	/** Stops the clock and withdraws the offer: a newer notice, or the screen
	 * going. */
	clear(): void {
		this.#offered = null;
		if (this.#fading !== null) {
			clearTimeout(this.#fading);
			this.#fading = null;
		}
		// A notice that is replaced while it is going arrives fully faded out
		// otherwise, because the class that is taking it away is still on it.
		this.leaving = false;
	}
}
