/**
 * A props object a test can change.
 *
 * Handing a component a plain object gives it props that never move: the window
 * hands the entry pane one entry and then another, and a test that cannot do
 * the same cannot check what the pane does about it. Runes only compile in a
 * `.svelte.ts` module, which is what this file is for.
 */
export function reactive<T extends object>(initial: T): T {
	const held = $state(initial);
	return held;
}
