<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { Group } from '$lib/model';
	import Icon from './Icon.svelte';

	/**
	 * The top of the entry pane: the folders down to the entry, its name, and
	 * the way out.
	 *
	 * Drawn once, for an entry Rust has read and for one it is still reading, so
	 * that the name and the close stand in the same place in both and nothing
	 * moves when the entry arrives.
	 */
	let {
		path,
		onClose,
		children
	}: {
		/** The folders from the vault down to the entry, for the line above
		 * the name. */
		path: Group[];
		/** Puts the pane away. */
		onClose: () => void;
		/** The name. */
		children: Snippet;
	} = $props();
</script>

<header class="shrink-0 border-b border-hairline px-6 py-5">
	<!--
		The folders down to this entry, and only when there are any. An entry at
		the top of a vault has none, and the empty line it used to leave was
		what pushed the title below the two buttons beside it.
	-->
	{#if path.length > 0}
		<div class="mb-1.5 truncate font-mono text-label tracking-label text-txt4 uppercase">
			{path.map((group) => group.name).join(' · ')}
		</div>
	{/if}

	<!-- The name and the way out, on one line and centred against each other.
	     The trash used to stand sixteen pixels from the close at the same size,
	     and a press meant for one took the other; deleting is now a labelled
	     action at the foot of the pane. -->
	<div class="flex items-center gap-4">
		{@render children()}
		<button
			type="button"
			onclick={onClose}
			class="shrink-0 text-txt4 transition-colors hover:text-txt2"
			aria-label="Close this entry"
		>
			<Icon name="x" class="h-4 w-4" />
		</button>
	</div>
</header>
