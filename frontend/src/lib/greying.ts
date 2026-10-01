/**
 * Telling the menu bar which of Coffer's items apply now, so that it greys out
 * the rest.
 *
 * One message at a time, and only when the list has changed. Rust answers two
 * side by side in either order, and the bar would be left as whichever
 * finished last said. What changes while one is on its way is sent once it has
 * landed, and only the latest of it. A message that fails leaves the bar as it
 * was. A choice the bar still offers is asked again when it arrives (`run` in
 * `menu.svelte.ts`), so an item offered too long does nothing; one greyed too
 * long cannot be chosen at all, which is why Rust answers these without
 * waiting for anything.
 */

import { menuState } from './ipc';
import type { Command } from './model';

/** What the bar was last told, so that a change leaving it the same costs no
 * message. Nothing yet: the bar a new window finds is the one Rust left. */
let told: string | null = null;
/** What the bar is to be told once the message on its way has landed. */
let next: Command[] | null = null;
let sending = false;

/** Tells Rust that the items in `now` are the ones the window can do. */
export function report(now: Command[]): void {
	next = now;
	if (!sending) void send();
}

async function send(): Promise<void> {
	sending = true;
	while (next !== null) {
		const now = next;
		next = null;
		const said = now.join(' ');
		if (said === told) continue;
		try {
			await menuState(now);
			told = said;
		} catch {
			// The bar still shows what it was last told, which is `told`.
		}
	}
	sending = false;
}
