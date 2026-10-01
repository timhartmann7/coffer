/**
 * Writing the vault back to its file after every change, and the question the
 * screen asks when the file will not take it: somebody else wrote it, or it is
 * not there any more.
 */

import { release } from './drafts';
import { asFailure, reload, rival, save, saveCopy, saveOver } from './ipc';
import type { Group, Rival } from './model';
import type { Notices } from './notices.svelte';
import { quoted } from './format';

/** The vault screen, as writing the file needs it. */
export interface Screen {
	/** Reads the versions in the pane back after a write. */
	reread(): Promise<void>;
	/** The file was read again: the tree it holds is the one to draw, and
	 * nothing in the pane is about it any more. */
	reloaded(tree: Group): void;
	/** Says why something was refused. */
	failed(thrown: unknown): void;
	notices: Notices;
}

export class Saving {
	/** Whether a write is on its way. */
	saving = $state(false);
	/**
	 * Whether the vault is holding a change the file has not got.
	 *
	 * A save refused with anything but a conflict raises a notice that fades
	 * after six seconds, and a reader who missed it goes on editing into a
	 * window whose every write is failing. This is the standing version of that
	 * notice, and the status bar is where it sits.
	 */
	unsaved = $state(false);
	/** The file that stopped the last save, while the reader is asked about it. */
	conflict = $state<Rival | null>(null);
	/** Whether the dialog is about a file somebody rewrote or one that is not
	 * there any more. The two ask different questions and offer different ways
	 * out: nothing can be reloaded from a file that is gone. */
	missing = $state(false);
	/** When the reader last changed the vault, which the dialog sets against
	 * when the file was written. */
	changedAt = $state<Date | null>(null);

	readonly #screen: Screen;

	constructor(screen: Screen) {
		this.#screen = screen;
	}

	/**
	 * Writes the vault back after a change.
	 *
	 * There is no save button, so this is what one means: the change is already
	 * in the window, and this is the moment it reaches the file. A file somebody
	 * else wrote in the meantime stops here and asks, and so does one that is not
	 * there any more.
	 *
	 * Both have to ask rather than report. A save that only raised a notice left
	 * the reader editing into a window whose every write failed - a renamed file,
	 * an unmounted disk - with a whole session's work in memory and nothing in
	 * the application able to put it anywhere.
	 */
	async persist(): Promise<void> {
		this.saving = true;
		try {
			await save();
			this.unsaved = false;
		} catch (thrown) {
			this.unsaved = true;
			const refused = asFailure(thrown);
			if (refused.code === 'externalChange' || refused.code === 'gone') {
				this.missing = refused.code === 'gone';
				this.conflict = await rival().catch(() => ({ modified: null, entries: null }));
			} else {
				this.#screen.failed(thrown);
			}
		} finally {
			this.saving = false;
		}
		await this.#screen.reread();
	}

	/** Throws the window's change away and reads the file again. */
	async takeTheirs(): Promise<void> {
		try {
			this.saving = true;
			const tree = await reload(release(null));
			this.#screen.reloaded(tree);
			this.changedAt = null;
			this.conflict = null;
			this.missing = false;
			// Reading the file again is throwing the change away, which is a
			// thing the reader chose. There is nothing left unsaved either way.
			this.unsaved = false;
		} catch (thrown) {
			this.#screen.failed(thrown);
		} finally {
			this.saving = false;
		}
	}

	/** Writes the window's version beside the file, and then takes the file's. */
	async keepBoth(): Promise<void> {
		try {
			this.saving = true;
			const beside = await saveCopy();
			if (!beside) return;
			this.conflict = null;
			this.#screen.notices.tell({ message: `Kept as ${quoted(beside.name)}`, kind: 'copied' });
			// Only where there is a file to take instead. When the vault itself
			// is gone there is nothing to read back, and the window goes on
			// holding the version the copy was made from - which is still the
			// only one, and can still be written back where it belongs.
			if (!this.missing) await this.takeTheirs();
			else await this.#screen.reread();
			this.missing = false;
		} catch (thrown) {
			this.#screen.failed(thrown);
		} finally {
			this.saving = false;
		}
	}

	/** Writes the window's version over the file. */
	async keepOurs(): Promise<void> {
		try {
			this.saving = true;
			await saveOver();
			this.conflict = null;
			this.missing = false;
			this.unsaved = false;
			await this.#screen.reread();
		} catch (thrown) {
			this.#screen.failed(thrown);
		} finally {
			this.saving = false;
		}
	}
}
