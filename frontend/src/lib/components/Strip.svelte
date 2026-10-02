<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';

	/**
	 * The warning plane across the top of the window, for as long as what is
	 * open is not the vault itself: the copy a lock left, or a backup. What it
	 * is goes on the left, under the warn icon, and the ways out on the right.
	 *
	 * The first thing under the title bar rather than a word in the status
	 * line: a reader who forgot which file this is would be acting on the one
	 * they did not mean to. Not part of the panes, so the settings do not
	 * cover it.
	 */
	let {
		children,
		actions
	}: {
		/** What is open, and what the ways out do to the vault's file. */
		children: Snippet;
		/** The ways out, as pills. */
		actions: Snippet;
	} = $props();
</script>

<div class="flex shrink-0 items-start gap-3 border-b border-warn/35 bg-warnwash px-4 py-3">
	<Icon name="warn" class="mt-0.5 h-4 w-4 shrink-0 text-warn" />
	<div class="min-w-0 flex-1">
		{@render children()}
	</div>
	<div class="flex shrink-0 gap-2">
		{@render actions()}
	</div>
</div>
