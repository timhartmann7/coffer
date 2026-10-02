import { chosen } from './context.svelte';
import { closeByHand } from './locking';
import { run } from './menu.svelte';
import type { Action } from './model';

/**
 * What Rust says the reader chose outside the page: an item of the menu bar,
 * an item of a menu under the pointer, or the window's own close button.
 *
 * A choice in a menu is the reader being there, as a key or a press is, so it
 * `stir`s the window's presence, and it is run before this answers. The close
 * button does not stir: the window is going. What this answers with settles
 * once the close has been asked for, and never rejects - nobody is left to
 * hear it.
 */
export async function outside(action: Action, stir: () => void): Promise<void> {
	// Rust closes the window itself a few seconds on if this never asks.
	if (action.action === 'closing') return closeByHand().catch(() => {});
	stir();
	if (action.action === 'context') chosen(action.serial, action.chosen);
	else run(action.command);
}
