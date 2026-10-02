/**
 * Whether the reader is writing in a field, as the focus moves.
 *
 * `typing` in `keys.ts` answers it for one key press, from the key's target.
 * The menu bar needs it between presses: an item greyed out while a field has
 * the focus has to be grey before the key arrives, because AppKit drops the key
 * of a grey item and the field has it to itself.
 */

import { typing } from './keys';

/** How many times the focus has moved. Nothing reads the number: it is what
 * makes whatever asked `writing` ask again when the focus moves. */
let moved = $state(0);

/** Follows the focus. The window hands it every `focusin` and `focusout`. */
export function focused(): void {
	moved += 1;
}

/**
 * Whether the focus is in a field. Cmd+Backspace there deletes to the start of
 * the line, as it does in every field on a Mac, so Move to Recycle Bin is
 * greyed out rather than racing the field for the key - the way Finder greys
 * its own Move to Trash while a name is being edited.
 *
 * Read off the document each time it is asked, not kept from the events. A
 * field WebKit takes off the screen with the focus in it - the password field
 * once the vault opens - sends no event at all, and an answer kept from the
 * last one would say the reader was still writing in a field that is gone.
 */
export function writing(): boolean {
	void moved;
	return typing(document.activeElement);
}

/**
 * Puts the reader in a field as it is drawn, with what is in it selected, so
 * that the first key replaces it: a name being changed is typed over, not
 * added to. An attachment.
 */
export function ready(element: HTMLInputElement): void {
	element.focus();
	element.select();
}
