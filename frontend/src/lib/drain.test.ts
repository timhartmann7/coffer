/**
 * The bar that drains while the clipboard holds a password.
 *
 * Its duration is a CSS animation, and CSS animations are written down at build
 * time: Tailwind reads class names out of the source, and the window's
 * `style-src 'self'` forbids setting a duration from script. So the list of
 * timeouts Rust offers has to have a matching animation in the stylesheet, and
 * nothing but this connects the two.
 */

import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const settings = readFileSync('../crates/vault-gui/src/settings.rs', 'utf8');
const css = readFileSync('src/app.css', 'utf8');
const toast = readFileSync('src/lib/components/Toast.svelte', 'utf8');

/** The seconds Rust will let the reader choose. */
function offered(): number[] {
	const found = settings.match(/CLIPBOARD_CHOICES: \[u64; \d+\] = \[([^\]]*)\]/);
	expect(found, 'settings.rs no longer states the clipboard timeouts it offers').not.toBeNull();
	return (found?.[1] ?? '')
		.split(',')
		.map((one) => Number(one.trim()))
		.filter((one) => Number.isFinite(one));
}

describe('every clipboard timeout has a bar that empties in exactly that long', () => {
	it('offers a handful of timeouts and no more', () => {
		const seconds = offered();
		expect(seconds.length).toBeGreaterThan(1);
		expect(seconds.every((one) => one > 0)).toBe(true);
	});

	it('defines one animation per offered timeout, of that many seconds', () => {
		for (const seconds of offered()) {
			expect(
				css.includes(`--animate-drain-clipboard-${seconds}: drain ${seconds}s linear forwards;`),
				`app.css has no ${seconds} second drain, so that choice would show a bar of the wrong length`
			).toBe(true);
		}
	});

	it('names every one of them in the toast, as a literal class', () => {
		for (const seconds of offered()) {
			expect(
				toast.includes(`'animate-drain-clipboard-${seconds}'`),
				`Toast.svelte cannot draw the ${seconds} second drain`
			).toBe(true);
		}
	});

	/** The other direction: an animation nothing can choose is a rule that
	 * compiles into the bundle for nobody. */
	it('defines no clipboard drain nothing offers', () => {
		const seconds = offered();
		for (const [, defined] of css.matchAll(/--animate-drain-clipboard-(\d+):/g)) {
			expect(seconds, `app.css defines a ${defined} second drain nothing offers`).toContain(
				Number(defined)
			);
		}
	});
});
