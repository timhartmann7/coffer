/**
 * The lengths `app.css` names, for the one thing a stylesheet cannot do.
 *
 * CSS animates a thing arriving. It cannot animate one leaving, because the
 * element is gone before any rule could run on it, and a notice that vanished
 * between two frames is the only motion in this window a reader ever asked for
 * and did not get. That half is Svelte's, and Svelte wants a number where the
 * stylesheet has a token - so the numbers are written here as well, and
 * `motion.svelte.test.ts` is what keeps the two copies from parting company.
 */

/** How long something that came from below takes to arrive, or to go. */
export const RISE = 170;

const LESS = '(prefers-reduced-motion: reduce)';

/**
 * How long something may take, once the reader has been asked.
 *
 * The stylesheet answers `prefers-reduced-motion` for everything it draws, in
 * one blanket rule. What Svelte drives does not go through a stylesheet at all:
 * it hands keyframes straight to the animation engine, which no CSS rule
 * reaches. So the preference is read here instead, and answered with the only
 * length that means "no motion".
 */
export function span(milliseconds: number): number {
	return window.matchMedia(LESS).matches ? 0 : milliseconds;
}
