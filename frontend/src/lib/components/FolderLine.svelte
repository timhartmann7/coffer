<script lang="ts">
	import type { Group } from '$lib/model';
	import { TOP_LABEL } from '$lib/places';
	import FolderPicker from './FolderPicker.svelte';
	import Icon from './Icon.svelte';
	import Path from './Path.svelte';

	/**
	 * The line above an entry's title: where the entry is, and the way to put it
	 * somewhere else.
	 *
	 * The mockup writes the folders down to the entry there. In an entry Coffer
	 * can write, the same line is a button that opens the folder list under it,
	 * and it reads "Top of the vault" for an entry in no folder rather than
	 * being left out. Without `onMove` - the entry is still being read - the
	 * line is drawn the same way with nothing to press, so nothing moves when
	 * the entry arrives.
	 */
	let {
		path,
		root,
		current,
		onMove
	}: {
		/** The folders from the vault down to the entry. */
		path: Group[];
		root: Group;
		/** The folder the entry is in: the root's id at the top of the vault. */
		current: string;
		/** Moves the entry to another folder, or to the top of the vault. */
		onMove?: (into: string) => void;
	} = $props();

	let open = $state(false);
	let button = $state<HTMLButtonElement>();
</script>

{#snippet where()}
	<span class="sr-only">Folder: </span>
	<span class="min-w-0 truncate">
		{#if path.length === 0}
			{TOP_LABEL}
		{:else}
			<Path names={path.map((group) => group.name)} />
		{/if}
	</span>
	<Icon name="chev-d" class="h-3.5 w-3.5 shrink-0" />
{/snippet}

{#if onMove}
	<div class="relative mb-1.5">
		<!-- The press keeps the focus where it was: the list takes it as it opens,
		     and a press while the list is open closes it instead of the focus
		     leaving the list first and the press opening it again. -->
		<button
			bind:this={button}
			type="button"
			aria-haspopup="listbox"
			aria-expanded={open}
			onmousedown={(event) => event.preventDefault()}
			onclick={() => (open = !open)}
			class="flex max-w-full items-center gap-1.5 font-mono text-label tracking-label text-txt4 uppercase transition-colors hover:text-txt2"
		>
			{@render where()}
		</button>
		{#if open}
			<FolderPicker
				{root}
				heading="Move to"
				label="Find the folder to move it to"
				{current}
				chosen={current}
				class="top-full right-0 left-0"
				onPick={(into) => {
					open = false;
					onMove(into);
					// The entry stays in the pane wherever it goes, and so does
					// this line: the focus comes back to it rather than falling
					// to the top of the window with the list it was in.
					button?.focus();
				}}
				onClose={(escaped) => {
					open = false;
					if (escaped) button?.focus();
				}}
			/>
		{/if}
	</div>
{:else}
	<div
		aria-disabled="true"
		class="mb-1.5 flex max-w-full items-center gap-1.5 font-mono text-label tracking-label text-txt4 uppercase"
	>
		{@render where()}
	</div>
{/if}
