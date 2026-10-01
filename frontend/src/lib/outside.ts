import { closeByHand } from './locking';
import { run } from './menu.svelte';
import type { Action } from './model';

/**
 * What Rust says the reader chose outside the page: an item of the menu bar,
 * or the window's own close button.
 *
 * A choice in the menu bar is the reader being there, as a key or a press is,
 * so it `stir`s the window's presence, and it is run before this answers. The
 * close button does not stir: the window is going. What this answers with
 * settles once the close has been asked for, and never rejects - nobody is
 * left to hear it.
 */
export async function outside(action: Action, stir: () => void): Promise<void> {
	// Rust closes the window itself a few seconds on if this never asks.
	if (action.action === 'closing') return closeByHand().catch(() => {});
	stir();
	run(action.command);
}
