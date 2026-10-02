/**
 * Whose a key is: the menu bar's or the window's, never both.
 *
 * The page sees a key before AppKit looks in the menu, and a key it answers is
 * gone, so a key both answered would happen twice. The two tables below are
 * the whole of who has which, and everything that draws a key - the sheet of
 * keyboard shortcuts, the Lock button's tooltip, the search field's key cap, a
 * row's copy buttons, the notice's Undo - draws it from here.
 */

import type { Command } from './model';

/**
 * What the menu bar draws beside each of Coffer's items that has a key, in the
 * order the bar draws them. The bar is Rust's (`menu.rs`), and
 * `shortcuts.test.ts` reads it and fails when the two disagree. An item with no
 * key is not here.
 */
export const KEYS = {
	settings: '⌘,',
	lock: '⌘L',
	newEntry: '⌘N',
	newFolder: '⇧⌘N',
	duplicate: '⌘D',
	openVault: '⌘O',
	find: '⌘F',
	copyLogin: '⌘B',
	copyPassword: '⇧⌘C',
	moveToBin: '⌘⌫'
} as const satisfies Partial<Record<Command, string>>;

/**
 * The keys the window answers itself, before AppKit looks in the menu bar, the
 * presses that choose rows in the list, and the system's Close Window, which
 * locks the vault here. None of them is an item's: a key both answered would
 * happen twice.
 */
export const OWN = {
	copy: '⌘C',
	undo: '⌘Z',
	choose: '⌘-click',
	reach: '⇧-click',
	all: '⌘A',
	away: 'Esc',
	close: '⌘W'
} as const;
