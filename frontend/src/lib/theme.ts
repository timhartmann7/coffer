/**
 * Which look the window wears, and putting it on.
 *
 * Rust keeps the choice and hands it over with the rest of the settings. This
 * is the half that turns it into the attribute the stylesheet keys on, and it
 * resolves the Mac's own answer here rather than in CSS: a stylesheet that
 * answered both the attribute and the media query for `system` would hold a
 * third copy of the light palette, and the third one is the one that goes
 * stale.
 */

import type { Theme } from './model';

/** What each look is called on the settings screen. Rust decides which of them
 * may be chosen and sends the list with the value; this decides what they are
 * called, the way `duration.ts` decides what a number of seconds is called. */
export const LOOKS: Record<Theme, string> = {
	system: 'System',
	dark: 'Dark',
	light: 'Light'
};

const DARK = '(prefers-color-scheme: dark)';

/**
 * Puts the chosen look on the html element and keeps it there.
 *
 * The attribute is always `dark` or `light`. `system` is an answer about where
 * to look rather than a look, and a stylesheet asked to draw it would have
 * nothing to draw.
 *
 * Answers with the teardown, which is what an effect wants back: a lock
 * destroys this window, and the listener has to go with it.
 */
export function wear(chosen: Theme): () => void {
	const dark = window.matchMedia(DARK);
	const paint = () => {
		document.documentElement.dataset.theme =
			chosen === 'system' ? (dark.matches ? 'dark' : 'light') : chosen;
	};

	paint();
	dark.addEventListener('change', paint);
	return () => dark.removeEventListener('change', paint);
}
