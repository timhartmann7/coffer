/**
 * The menu bar and the window, checked against each other.
 *
 * The bar is Rust's: `menu.rs` names Coffer's items and the key each is
 * chosen by, in the words muda reads. The window names the same items by the
 * same words to answer them, and draws the same keys in the sheet of keyboard
 * shortcuts, the Lock button's tooltip and the search field's key cap. Nothing
 * but this keeps the two saying the same thing.
 */

import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { COMMANDS } from './model';
import { KEYS, OWN } from './shortcuts';

const menu = readFileSync('../crates/vault-gui/src/menu.rs', 'utf8');

/** `NewEntry` as the page spells it: `newEntry`. */
const camel = (variant: string) => variant[0].toLowerCase() + variant.slice(1);

/** The variants of `Command`, in the order the enum declares them. */
function variants(): string[] {
	const body = menu.match(/pub enum Command \{([^}]*)\}/)?.[1] ?? '';
	return [...body.matchAll(/^\s*(\w+),/gm)].map(([, name]) => camel(name));
}

/** The key each item is built with, as the bar draws it on a Mac: ⌥ before ⇧
 * before ⌘, and the key itself last. */
function drawn(): Map<string, string | null> {
	const found = new Map<string, string | null>();
	const body = menu.match(/fn accelerator\(self\)[^{]*\{([\s\S]*?)\n {4}\}/)?.[1] ?? '';
	for (const [, variant, accelerator] of body.matchAll(
		/Command::(\w+) => (?:Some\("([^"]*)"\)|None)/g
	)) {
		if (accelerator === undefined) {
			found.set(camel(variant), null);
			continue;
		}
		const parts = accelerator.split('+');
		const key = parts.at(-1) ?? '';
		const glyph = key === 'Backspace' ? '⌫' : key;
		const modifiers =
			(parts.includes('Alt') ? '⌥' : '') +
			(parts.includes('Shift') ? '⇧' : '') +
			(parts.includes('CmdOrCtrl') ? '⌘' : '');
		found.set(camel(variant), modifiers + glyph);
	}
	return found;
}

describe('the menu bar and the window agree on its items', () => {
	it('names every item of Coffer’s own the way menu.rs does, in the bar’s order', () => {
		expect(variants().length).toBeGreaterThan(5);
		expect(variants()).toEqual([...COMMANDS]);
	});

	/** A key drawn in the window that the bar does not answer is a promise the
	 * reader finds broken; one the bar answers and the window does not draw is
	 * a key nobody learns. */
	it('draws every key the way the bar draws the key menu.rs gives it', () => {
		const bar = drawn();
		expect(bar.size, 'menu.rs gives no keys this test can read').toBe(COMMANDS.length);

		const keyed: Partial<Record<string, string>> = KEYS;
		for (const command of COMMANDS) {
			expect(keyed[command] ?? null, command).toBe(bar.get(command));
		}
		for (const command of Object.keys(KEYS)) {
			expect(COMMANDS, `${command} is drawn with a key and is no item`).toContain(command);
		}
	});

	/** The window answers its own keys before AppKit looks in the menu, so a
	 * key in both lists would happen twice. */
	it('gives no item a key the window keeps for itself', () => {
		const own: string[] = Object.values(OWN);
		for (const [command, key] of Object.entries(KEYS)) {
			expect(own, `${command} is on ${key}, which the window answers`).not.toContain(key);
		}
		expect(new Set(Object.values(KEYS)).size, 'two items share a key').toBe(
			Object.values(KEYS).length
		);
	});
});
