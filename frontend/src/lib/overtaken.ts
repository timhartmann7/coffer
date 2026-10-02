/**
 * What an undo says when Rust will not take back what it names because the
 * vault has moved on since.
 *
 * An undo acts by id on what its notice named, eight seconds and any number of
 * other changes later. A field can have been changed again since it came off,
 * an entry or a folder moved again since a move, gone out of the file, come
 * back out of the bin some other way, or into a folder that went to the bin
 * since, and Rust refuses the undo whole rather than take back part of it or
 * the reader's later choice. Nothing was done: every undo in the window says
 * so here, in one sentence, and reads the tree again to show what is there
 * now.
 */

import { asFailure } from './ipc';
import type { Failure } from './model';
import type { Notices } from './notices.svelte';

/** The window, as an undo that came too late needs it. */
interface Screen {
	/** Reads the tree again and draws it, writing nothing. */
	redraw(): Promise<void>;
	notices: Notices;
}

/**
 * Says an undo can no longer be done and reads the tree again, when Rust
 * refused it with one of the codes in `late`, or hands any other refusal on
 * to be said as it is.
 *
 * Which refusals mean the undo came too late, rather than that it failed, is
 * the undo's own to say: `refused` from putting entries back is one no longer
 * in the bin, and from another command it is something else altogether.
 */
export async function overtaken(
	thrown: unknown,
	screen: Screen,
	late: readonly Failure['code'][]
): Promise<void> {
	if (!late.includes(asFailure(thrown).code)) throw thrown;
	screen.notices.warn('Something has changed since, so that can no longer be undone.');
	await screen.redraw();
}
