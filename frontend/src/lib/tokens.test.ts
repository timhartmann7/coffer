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
	 * The mark is drawn twice: once in `assets/brand/coffer-logo-accent.svg`,
	 * which is the drawing, and once as the table in `Mark.svelte`, which is what
	 * the window draws. `vite.config.ts` refuses two copies of a brand file for
	 * exactly this reason, and the copy that survives here is the one that would
	 * quietly go stale.
	 */
	it('draws the same mark the brand file does', () => {
		const drawing = readFileSync('../assets/brand/coffer-logo-accent.svg', 'utf8');
		const squares = [
			...drawing.matchAll(
				/<rect x="([\d.]+)" y="([\d.]+)" width="([\d.]+)" height="([\d.]+)" rx="([\d.]+)"\/>/g
			)
		].map(([, x, y, width, height, corner]) => {
			expect(width, 'a square in the mark is not square').toBe(height);
			return [x, y, width, corner].map(Number);
		});

		const component = readFileSync('src/lib/components/Mark.svelte', 'utf8');
		const table = [...component.matchAll(/\[([\d.]+), ([\d.]+), ([\d.]+), ([\d.]+)\]/g)].map(
			([, ...row]) => row.map(Number)
		);

		expect(squares.length).toBeGreaterThan(100);
		expect(table).toEqual(squares);
	});

	/** A mark with a colour baked into it is invisible on one of the two themes,
	 * and the screen still renders, so nothing else would fail. */
	it('lets the mark take the colour of whatever draws it', () => {
		const component = readFileSync('src/lib/components/Mark.svelte', 'utf8');
		expect(component).toContain('fill="currentColor"');
		expect(/#[0-9a-fA-F]{3,8}\b/.test(component)).toBe(false);

		for (const { path, text } of sources(['.svelte'])) {
			expect(/["'(]\/coffer-[\w-]*\.svg/.test(text), `${path} loads a brand file`).toBe(false);
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

	/**
	 * WebKit has never shipped `user-select` unprefixed, so the plain spelling
	 * is a no-op in the engine this window runs in and the prefixed one is the
	 * rule. The bundler adds it on the way to a release build and not on the way
	 * to a development one, which made the window a selectable page in exactly
	 * the build somebody works in all day.
	 */
	it('says who may be selected in the spelling WebKit reads', () => {
		const css = readFileSync('src/app.css', 'utf8');

		for (const [, value] of css.matchAll(/(?<!-)\buser-select:\s*(\w+);/g)) {
			expect(
				css.includes(`-webkit-user-select: ${value};`),
				`app.css asks for user-select: ${value} in a spelling WebKit ignores`
			).toBe(true);
		}

		// Naming no colour is not naming none: a run with no rule of its own is
		// painted in the system's highlight.
		expect(css, 'the chrome paints a selection again').toContain('background: transparent;');
	});
});
