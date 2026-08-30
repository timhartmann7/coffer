/**
 * Wearing a look.
 *
 * The name says `.svelte.` because that is how `vite.config.ts` picks the
 * project with a document in it, not because this module compiles runes. It
 * does not.
 */

import { afterEach, expect, it, vi } from 'vitest';
import { wear } from './theme';

/** A stand-in for the Mac's own answer, which a test cannot change for real. */
function macSays(dark: boolean) {
	const listeners = new Set<() => void>();
	const media = {
		matches: dark,
		addEventListener: (_: string, run: () => void) => listeners.add(run),
		removeEventListener: (_: string, run: () => void) => listeners.delete(run)
	};

	vi.stubGlobal('matchMedia', vi.fn().mockReturnValue(media));
	return {
		listeners,
		flip(now: boolean) {
			media.matches = now;
			for (const run of listeners) run();
		}
	};
}

afterEach(() => delete document.documentElement.dataset.theme);

/** An implementation that wrote `system` straight onto the document would match
 * no rule in `app.css`, and leave the reader in whichever palette `:root` holds. */
it('writes what the Mac answered, never the question', () => {
	const mac = macSays(true);
	const stop = wear('system');

	expect(document.documentElement.dataset.theme).toBe('dark');

	mac.flip(false);
	expect(document.documentElement.dataset.theme).toBe('light');

	stop();
});

/** The bug this catches is picking Light and keeping it until the Mac's own
 * schedule turns the machine dark at sunset and takes the window with it. */
it('ignores the Mac once the reader has answered for themselves', () => {
	const mac = macSays(false);
	const stop = wear('light');

	expect(document.documentElement.dataset.theme).toBe('light');

	mac.flip(true);
	expect(document.documentElement.dataset.theme).toBe('light');

	stop();
});

it('draws dark without asking the Mac at all', () => {
	const mac = macSays(false);
	const stop = wear('dark');

	expect(document.documentElement.dataset.theme).toBe('dark');
	mac.flip(true);
	expect(document.documentElement.dataset.theme).toBe('dark');

	stop();
});

/** A lock destroys this window a hundred times over a working day. A `wear`
 * that added a listener per unlock and removed none is the leak that the
 * standing attack list exists to find. */
it('lets go of the Mac when the window it was worn by goes', () => {
	const mac = macSays(true);

	for (let again = 0; again < 100; again += 1) wear('system')();
	expect(mac.listeners.size).toBe(0);

	const stop = wear('system');
	expect(mac.listeners.size).toBe(1);
	stop();
	expect(mac.listeners.size).toBe(0);
});
