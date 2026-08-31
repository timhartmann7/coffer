/**
 * The bar that drains while a revealed value counts down to being hidden.
 *
 * Its duration is a CSS animation, and CSS animations are written down at build
 * time: Tailwind reads class names out of the source, and the window's
 * `style-src 'self'` forbids setting a duration from script. So the seconds the
 * bar takes to empty and the seconds the value stays on the screen are two
 * numbers in two files, and nothing but this connects them.
 */

import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const css = readFileSync('src/app.css', 'utf8');
const reveal = readFileSync('src/lib/reveal.svelte.ts', 'utf8');
const field = readFileSync('src/lib/components/PasswordField.svelte', 'utf8');

describe('the drain empties in exactly as long as the value is shown', () => {
	it('gives the reveal a bar of its own length', () => {
		const shown = reveal.match(/const SECONDS = (\d+)/);
		expect(shown, 'reveal.svelte.ts no longer states how long a value stays').not.toBeNull();

		expect(css).toContain(`--animate-drain-reveal: drain ${shown?.[1]}s linear forwards;`);
		expect(field).toContain('animate-drain-reveal');
	});

	/** A drain nothing draws with is a rule that compiles into the bundle for
	 * nobody, and the four clipboard drains became exactly that when the copy
	 * notice stopped counting a minute down in the corner. */
	it('defines no drain nothing draws', () => {
		for (const [, name] of css.matchAll(/--animate-(drain-[\w-]+):/g)) {
			const drawn = [field, reveal].some((source) => source.includes(`animate-${name}`));
			expect(drawn, `app.css defines animate-${name}, and nothing draws it`).toBe(true);
		}
	});

	/**
	 * The reduced-motion block empties every animation in the window at once,
	 * and the drain is the exception: its length is the reading. A bar that
	 * emptied at once beside "Hides in 0:30" would be saying something untrue.
	 */
	it('keeps the drain out of the reduced-motion blanket', () => {
		const exemption = css.match(/\*:not\(\[class\*='([^']+)'\]\)/);
		expect(exemption, 'the reduced-motion block spares nothing any more').not.toBeNull();
		expect('animate-drain-reveal').toContain(exemption?.[1] ?? 'nothing at all');
	});

	/**
	 * Everything else is the other way round. A panel arriving, a sheet opening
	 * and a notice landing all move for their own sake, so reduced motion is
	 * right to silence every one of them - and the way this window silences an
	 * animation is by not sparing it, which means the exemption has to stay
	 * narrow as animations are added.
	 */
	it('leaves every animation that is not a drain inside the blanket', () => {
		const spared = css.match(/\*:not\(\[class\*='([^']+)'\]\)/)?.[1] ?? '';
		const named = [...css.matchAll(/--animate-([\w-]+):/g)].map(([, name]) => name);

		expect(named.length, 'app.css names no animations at all').toBeGreaterThan(1);

		for (const name of named) {
			expect(
				`animate-${name}`.includes(spared) === name.startsWith('drain-'),
				`animate-${name} is on the wrong side of the reduced-motion blanket`
			).toBe(true);
		}
	});
});
