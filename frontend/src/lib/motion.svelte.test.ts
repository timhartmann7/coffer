/**
 * The one length that is written down twice.
 *
 * The name says `.svelte.` because that is how `vite.config.ts` picks the
 * project with a window in it, not because this module compiles runes. It does
 * not; it only asks one what the reader answered about motion.
 *
 * A notice arrives on a keyframe the stylesheet names and leaves on a transition
 * the stylesheet times, and the window has to take it out of the document once
 * it has gone - which is a number in script. Two files, one movement, and
 * nothing but this connects them: a stylesheet that slowed the exit down would
 * leave the notice half faded when the window removed it, and one that sped it
 * up would leave an invisible box in the corner for the difference.
 */

import { readFileSync } from 'node:fs';
import { describe, expect, it, vi } from 'vitest';
import { RISE } from './motion';

const css = readFileSync('src/app.css', 'utf8');
const toast = readFileSync('src/lib/components/Toast.svelte', 'utf8');
const vault = readFileSync('src/lib/components/Vault.svelte', 'utf8');

describe('a notice goes in exactly as long as it is drawn going', () => {
	it('leaves on the length the stylesheet gives it to arrive', () => {
		expect(css, 'app.css no longer says how long a rise takes').toContain(
			`--animate-rise: rise ${RISE}ms ease-out;`
		);
		expect(toast, 'the notice leaves on a length of its own').toContain(`duration-[${RISE}ms]`);
	});

	/** A notice that is removed on its own clock rather than on the movement's is
	 * the half-faded box, or the invisible one. */
	it('is taken out of the document by the same number', () => {
		expect(vault, 'the window waits on something other than the movement').toContain('span(RISE)');
	});

	/**
	 * The exit is a transition and transitions are inside the reduced-motion
	 * blanket, so the stylesheet already answers the preference. What script does
	 * has to answer it too, or a reader who asked for no motion waits a fifth of
	 * a second for a notice that went at once.
	 */
	it('waits for nothing when the reader asked for no motion', async () => {
		const { span } = await import('./motion');

		const asked = (reduce: boolean) => {
			vi.stubGlobal('matchMedia', () => ({ matches: reduce }));
			return span(RISE);
		};

		expect(asked(false), 'the movement lost its length').toBe(RISE);
		expect(asked(true), 'a reader who asked for no motion still waits for one').toBe(0);
	});
});
