/**
 * The two palettes, and the one list of looks they are chosen from.
 *
 * A colour is the one kind of mistake a test that mounts components cannot
 * find: a token left behind in the light block still renders, it is just
 * unreadable. So this reads the stylesheet itself, the way `drain.test.ts`
 * reads the clipboard timeouts out of Rust.
 */

import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { LOOKS } from './theme';

const css = readFileSync('src/app.css', 'utf8');
const settings = readFileSync('../crates/vault-gui/src/settings.rs', 'utf8');
const page = readFileSync('src/routes/+page.svelte', 'utf8');
const document = readFileSync('src/app.html', 'utf8');
const configuration = JSON.parse(readFileSync('../crates/vault-gui/tauri.conf.json', 'utf8'));
const opening = readFileSync('../crates/vault-gui/src/window.rs', 'utf8');

/** The body of the rule a selector opens, found by counting braces so that a
 * nested block cannot end it early. */
function ruleBody(selector: string): string {
	const at = css.indexOf(selector);
	expect(at, `app.css no longer has a rule for ${selector}`).toBeGreaterThan(-1);

	let depth = 0;
	for (let scan = css.indexOf('{', at); scan < css.length; scan += 1) {
		if (css[scan] === '{') depth += 1;
		if (css[scan] === '}') {
			depth -= 1;
			if (depth === 0) return css.slice(css.indexOf('{', at) + 1, scan);
		}
	}
	throw new Error(`the rule for ${selector} is never closed`);
}

function colours(body: string): Map<string, string> {
	return new Map([...body.matchAll(/--color-([a-z0-9]+):\s*([^;]+);/g)].map((m) => [m[1], m[2]]));
}

/** Everything a rule declares, not only its colours: `color-scheme` decides the
 * scrollbars, the caret and the range control, and it drifts as easily as a hex. */
function declarations(body: string): Map<string, string> {
	return new Map(
		[...body.matchAll(/([a-z-]+(?:-[a-z0-9]+)*):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()])
	);
}

/** The dark palette is the `@theme` block, whose declarations carry exactly one
 * tab. Everything inside `@layer base` is nested at least twice. */
const dark = new Map(
	[...css.matchAll(/^\t--color-([a-z0-9]+):\s*([^;]+);/gm)].map((m) => [m[1], m[2]])
);
/** The same palette twice: once for the Mac's own answer, once for a reader who
 * said. A media query and an attribute cannot share a selector list. */
const followedBody = ruleBody(":root:not([data-theme='dark'])");
const forcedBody = ruleBody(":root[data-theme='light']");
const forced = colours(forcedBody);

/** Not a colour: the absence of one, in either palette. */
const NOT_A_COLOUR = 'transparent';

function luminance(hex: string): number {
	const channels = [1, 3, 5].map((at) => parseInt(hex.slice(at, at + 2), 16) / 255);
	const [r, g, b] = channels.map((c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(one: string, other: string): number {
	const [a, b] = [luminance(one), luminance(other)].sort((x, y) => y - x);
	return (a + 0.05) / (b + 0.05);
}

function readable(palette: Map<string, string>, ink: string, ground: string): number {
	const first = palette.get(ink);
	const second = palette.get(ground);
	expect(first && second, `${ink} or ${ground} is missing from a palette`).toBeTruthy();
	return contrast(first as string, second as string);
}

describe('the light theme answers the dark one', () => {
	/** Two copies of a palette part company silently: a forced light window
	 * quietly differs from one the Mac chose, and nothing else notices. */
	it('says the same thing to the media query and to the attribute', () => {
		const followed = declarations(followedBody);

		expect(followed.get('color-scheme'), 'the light rule sets no colour scheme').toBe('light');
		expect(Object.fromEntries(declarations(forcedBody))).toEqual(Object.fromEntries(followed));
	});

	it('has a light value for every colour the dark theme names', () => {
		const owed = [...dark.keys()].filter((name) => name !== NOT_A_COLOUR).sort();
		expect(owed.length).toBeGreaterThan(10);
		expect([...forced.keys()].sort()).toEqual(owed);
	});

	/** A token copied across rather than mirrored is invisible in a diff full of
	 * hexes, and leaves one dark value in a light window. */
	it('mirrors every colour rather than carrying one over', () => {
		for (const [name, value] of forced) {
			expect(value, `--color-${name} is the same in both themes`).not.toBe(dark.get(name));
		}
	});

	/** `system` is resolved before it reaches the document. A rule for it would
	 * be the moment this stylesheet grew a third copy of the palette. */
	it('never draws a look called system', () => {
		expect(css).not.toContain("data-theme='system'");
		expect(css).not.toContain('data-theme="system"');
	});
});

describe('both palettes are readable', () => {
	const planes = ['surface', 'surface2', 'raised'];
	const steps = ['txt', 'txt2', 'txt3', 'txt4'];

	it('clears 4.5 : 1 for every step of text on the window plane', () => {
		for (const palette of [dark, forced]) {
			for (const step of steps) {
				expect(readable(palette, step, 'surface')).toBeGreaterThanOrEqual(4.5);
			}
		}
	});

	/** The dark ramp is the mockup's and falls under the bar on the two planes
	 * above the window; the light one is ours, so it clears all three. */
	it('clears 4.5 : 1 on every plane it paints on, in the light theme', () => {
		for (const plane of planes) {
			for (const step of steps) {
				expect(readable(forced, step, plane)).toBeGreaterThanOrEqual(4.5);
			}
		}
	});

	/** `text-canvas` on `bg-accent` is the label of every primary button in the
	 * application, and nothing in a component's class list says so. */
	it('keeps the label on a filled accent button readable', () => {
		for (const palette of [dark, forced]) {
			expect(readable(palette, 'canvas', 'accent')).toBeGreaterThanOrEqual(4.5);
			expect(readable(palette, 'canvas', 'accenthi')).toBeGreaterThanOrEqual(4.5);
		}
	});

	/** The two pairings that exist in no class list: the selection colour, and
	 * the destructive button's own hover. */
	it('keeps a selected run of text and a danger message on its wash readable', () => {
		for (const palette of [dark, forced]) {
			expect(readable(palette, 'txt', 'selection')).toBeGreaterThanOrEqual(4.5);
			expect(readable(palette, 'danger', 'dangerwash')).toBeGreaterThanOrEqual(4.5);
		}
	});
});

describe('the looks the screen offers are the looks Rust knows', () => {
	/** Both directions, following `drain.test.ts`: a look Rust added and the
	 * screen did not draws an empty segment; a word the screen has and Rust does
	 * not is a segment nobody can reach. */
	it('has a word for each look Rust offers, and for nothing else', () => {
		const offered = [...settings.matchAll(/Theme::\w+ => "(\w+)"/g)].map((m) => m[1]);
		expect(offered.length).toBeGreaterThan(1);
		expect(offered.sort()).toEqual(Object.keys(LOOKS).sort());
	});

	/** Restoring a theme to the configuration file would kill the
	 * follow-the-system state without breaking a single rule in the stylesheet:
	 * the media query would simply never match. */
	it('leaves the appearance to the setting rather than to the config', () => {
		const main = configuration.app.windows.find((one: { label: string }) => one.label === 'main');
		expect(main).toBeTruthy();
		expect(main.theme).toBeUndefined();
	});

	/** A `wear` that exists and is never called passes every other test here and
	 * ships a settings row that stores a look and never draws it. */
	it('is worn by the screen that holds the choice', () => {
		expect(page).toContain('wear(');
	});

	/**
	 * The one line that decides what the first frame is.
	 *
	 * A document that says nothing about its colour scheme is painted white,
	 * whatever the window around it is wearing, and `app.css` is a second request
	 * that lands after that paint. Measured at 97% of the window for a frame, on
	 * every launch and on every lock. It reads as a tidy-away and it is the whole
	 * fix, so it is written down here.
	 */
	it('tells the document its colour scheme before a stylesheet can', () => {
		expect(document, 'the first frame is white again').toMatch(
			/<meta name="color-scheme" content="light dark"\s*\/>/
		);
	});

	/**
	 * The other half of the same frame.
	 *
	 * A document that knows its colour scheme is still nothing at all until the
	 * webview has drawn it once, and a webview is white until then whatever the
	 * window around it is wearing. So the window is built with nothing on the
	 * screen and shown when the page has drawn - which is what a lock, the one
	 * moment a whole window is built while the reader watches, would otherwise
	 * show as a white flash.
	 */
	it('shows the window when the page has drawn rather than when it is built', () => {
		expect(opening, 'the window is built onto the screen').toContain('config.visible = false');
		expect(opening, 'nothing shows the window again').toMatch(
			/PageLoadEvent::Finished[\s\S]{0,200}\.show\(\)/
		);
	});
});
