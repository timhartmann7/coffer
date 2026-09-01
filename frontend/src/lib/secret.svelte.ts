/**
 * Whose value is in a field: the vault's, or the reader's.
 *
 * A value Coffer put on the screen and a value somebody is typing look the same
 * and must be treated as opposites. The first is already in the file, so writing
 * it back would spend a second of key derivation, a rewrite of the whole
 * database and one of the ten snapshots beside it to store what is already
 * there. The second is the only copy there is, so a timer that wiped it
 * mid-word would take away what the reader had just written.
 *
 * Every row that both shows and takes a protected value goes through here, so
 * the rule is in one place: the entry's password, and every field of the
 * reader's own that the database protects.
 */

import { Revealed } from './reveal.svelte';

export class Secret extends Revealed {
	/** Whether what is in the field belongs to the reader. Set by the first
	 * keystroke and by opening an empty field, and never by a reveal. */
	#writing = $state(false);

	/** Whether the field is live at all, however its value got there. */
	get live(): boolean {
		return this.showing || this.#writing;
	}

	/** Opens an empty field for a value to be written into, for a field that has
	 * nothing to reveal. */
	open(node: HTMLInputElement): void {
		this.hide();
		node.value = '';
		this.#writing = true;
	}

	/** The first keystroke turns a value being read into one being written. */
	written(): void {
		if (!this.showing) return;
		this.release();
		this.#writing = true;
	}

	/** Takes whatever is in the field off the screen, writing nothing back. */
	close(node: HTMLInputElement | undefined): void {
		this.hide();
		if (this.#writing && node) node.value = '';
		this.#writing = false;
	}

	/**
	 * Closes the field and answers with what the reader wrote, or `null` when
	 * nothing in it was theirs.
	 *
	 * `null` rather than `''`: an empty string is a value somebody deliberately
	 * cleared, and a caller that could not tell the two apart would either lose
	 * that or write an empty value over a real one every time a reveal timed
	 * out and the focus left afterwards.
	 */
	settle(node: HTMLInputElement | undefined): string | null {
		const written = this.#writing && node ? node.value : null;
		this.close(node);
		return written;
	}
}
