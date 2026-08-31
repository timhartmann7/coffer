<script lang="ts">
	import type { Snippet } from 'svelte';

	/**
	 * The box a value sits in, drawn once for every value in the window.
	 *
	 * It is the mockup's own field: a hairline, the panel plane behind it, and
	 * the accent ring the moment something inside it takes focus. Every entry
	 * screen used to draw a value as bare text and grow a border under the
	 * pointer, which told a reader that a value could be changed only once they
	 * had already found out by accident.
	 *
	 * Its height is the reason it is a component rather than a class somebody
	 * remembers. A masked password and the field that reveals it are two
	 * different elements in the same place, and a box that measured itself from
	 * whichever one is showing moved the whole screen down every time an eye was
	 * opened. So the line inside is a fixed one, the box is at least as tall as
	 * that line, and the two states are the same size by construction.
	 */
	let { children }: { children: Snippet } = $props();
</script>

<!--
	Positioned and clipping, so that a value out of somebody's database stays
	inside the box drawn for it and the countdown can be drawn as the box's own
	bottom edge. The focus ring is a shadow on this element rather than something
	inside it, so clipping never reaches it.

	The mockup draws a third state for a field, the one with a border the colour
	of danger. Nothing in an entry can be refused a value - the engine takes any
	string a field can hold - so the two screens that do refuse one, the unlock
	and the creation, draw it themselves, and a prop for a state nobody here
	reaches would be a branch nothing evaluates.
-->
<span
	class="relative flex min-h-[42px] min-w-0 flex-1 items-center gap-2 overflow-hidden rounded-sm border border-hairline bg-surface2 px-3 py-2.5 leading-snug transition focus-within:border-accent focus-within:ring-4 focus-within:ring-accent/15"
>
	{@render children()}
</span>
