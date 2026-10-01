<script lang="ts">
	import type { Snippet } from 'svelte';

	/**
	 * The box of a value that reads as a heading rather than as a field: the
	 * entry's name.
	 *
	 * No line around it until it is pointed at or written in, and the room that
	 * line takes all the same. One component, because the name of an entry still
	 * being read is drawn in it too, and a copy of its padding that drifted from
	 * the field's moved the name sideways the moment the entry arrived.
	 */
	let {
		live = false,
		children
	}: {
		/** Something can be written into it, and the box says so under the
		 * pointer and around the focus. */
		live?: boolean;
		children: Snippet;
	} = $props();
</script>

<span
	class="flex min-w-0 flex-1 items-center gap-2 rounded-sm border border-transparent px-2 py-1 {live
		? 'transition focus-within:border-accent focus-within:bg-surface2 focus-within:ring-4 focus-within:ring-accent/15 hover:border-hairline'
		: ''}"
>
	{@render children()}
</span>
