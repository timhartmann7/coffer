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
	let {
		children,
		invalid = false,
		class: extra = ''
	}: {
		children: Snippet;
		/** A value the vault would refuse. The mockup draws this state, and it is
		 * the one place a field is not the accent's. */
		invalid?: boolean;
		class?: string;
	} = $props();
</script>

<!--
	Positioned and clipping, so that a value out of somebody's database stays
	inside the box drawn for it and the countdown can be drawn as the box's own
	bottom edge. The focus ring is a shadow on this element rather than something
	inside it, so clipping never reaches it.
-->
<span
	class="relative flex min-h-[42px] min-w-0 flex-1 items-center gap-2 overflow-hidden rounded-sm border bg-surface2 px-3 py-2.5 leading-snug transition {invalid
		? 'border-danger/60'
		: 'border-hairline focus-within:border-accent focus-within:ring-4 focus-within:ring-accent/15'} {extra}"
>
	{@render children()}
</span>
