/**
 * The rules about the window that are true of the source rather than of a
 * running screen, and that a reader would otherwise have to remember.
 */

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

function sources(extensions: string[]): { path: string; text: string }[] {
	const found: { path: string; text: string }[] = [];

	function walk(directory: string) {
		for (const entry of readdirSync(directory)) {
			const path = join(directory, entry);
			if (statSync(path).isDirectory()) {
				walk(path);
			} else if (extensions.some((extension) => entry.endsWith(extension))) {
				found.push({ path, text: readFileSync(path, 'utf8') });
			}
		}
	}

	walk('src');
	return found;
}

/** Tailwind's own palette. `app.css` clears the colour namespace so none of
 * these compile to anything, and this is the check that nobody writes one and
 * wonders why the screen is unstyled. */
const PALETTE =
	/\b(?:bg|text|border|ring|fill|stroke|from|via|to|decoration|outline|shadow|accent|caret|divide|placeholder)-(?:slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose|white|black)(?:-\d{2,3})?\b/;

describe('the window is drawn in the tokens of design.html', () => {
	it('uses no colour from the framework palette', () => {
		for (const { path, text } of sources(['.svelte', '.ts', '.css', '.html'])) {
			expect(PALETTE.test(text), `${path} names a colour that is not in design.html`).toBe(false);
		}
	});

	/** The one injection path a vault has is a value out of somebody's database
	 * written as markup. */
	it('never renders a value as HTML', () => {
		for (const { path, text } of sources(['.svelte'])) {
			expect(text.includes('{@html'), `${path} renders HTML from a value`).toBe(false);
		}
	});

	/**
	 * The Content-Security-Policy is `style-src 'self'`, which covers style
	 * attributes as well as style elements, and Tauri's nonce only reaches what
	 * is in the HTML at build time. An inline style is a rule the window would
	 * silently lose in a real build.
	 */
	it('sets no style from markup or from script', () => {
		for (const { path, text } of sources(['.svelte', '.ts', '.html'])) {
			expect(/\sstyle="/.test(text), `${path} carries a style attribute`).toBe(false);
			expect(/\sstyle:[a-z-]+/.test(text), `${path} sets a style from markup`).toBe(false);
			expect(/\.style\.[a-zA-Z]/.test(text), `${path} sets a style from script`).toBe(false);
		}
	});
});
