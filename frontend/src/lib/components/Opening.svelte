<script lang="ts">
	import type { EntryRow, Group } from '$lib/model';
	import Field from './Field.svelte';
	import Heading from './Heading.svelte';
	import Mask from './Mask.svelte';

	/**
	 * An entry chosen from the list that Rust has not read yet.
	 *
	 * Rust answers one command at a time, and a save holds it for a whole key
	 * derivation and the encryption of the vault, about a second. An entry
	 * chosen in that second used to wait for it with the one before still on the
	 * screen. The row that was pressed already says what the entry is called and
	 * who it logs in as, so the pane says that at once, and says that the rest is
	 * on its way.
	 *
	 * Nothing here can be changed, copied or revealed, because none of it has
	 * been read. The heading and the login's box are the ones the entry is drawn
	 * with, so the name and the login stay where they are when it arrives.
	 */
	let {
		row,
		path,
		onClose
	}: {
		/** The row that was pressed. */
		row: EntryRow;
		/** The folders from the vault down to the entry. */
		path: Group[];
		/** Puts the pane away, and the entry's answer with it. */
		onClose: () => void;
	} = $props();
</script>

<section class="flex h-full flex-col overflow-hidden bg-surface" aria-busy="true">
	<Heading {path} {onClose}>
		{#if row.title === null}
			<h1 class="min-w-0 flex-1 truncate text-title font-medium tracking-tight text-txt">
				<Mask />
			</h1>
		{:else}
			<!-- The padding and the invisible border of the title's own field, so
			     the name is where the field puts it. -->
			<h1 class="-ml-2 flex min-w-0 flex-1">
				<span
					class="min-w-0 flex-1 truncate rounded-sm border border-transparent px-2 py-1 text-title font-medium tracking-tight {row.title ===
					''
						? 'text-txt4'
						: 'text-txt'}"
				>
					{row.title === '' ? 'Untitled' : row.title}
				</span>
			</h1>
		{/if}
	</Heading>

	<div class="flex-1 overflow-y-auto px-6 py-5">
		<div class="block">
			<span class="font-mono text-label tracking-label text-txt3 uppercase">Login</span>
			<span class="mt-1.5 flex items-center gap-2">
				<Field>
					{#if row.username === null}
						<Mask />
					{:else}
						<span
							class="min-w-0 flex-1 truncate font-mono text-body {row.username === ''
								? 'text-txt4'
								: 'text-txt'}"
						>
							{row.username === '' ? 'No login' : row.username}
						</span>
					{/if}
				</Field>
			</span>
		</div>

		<!-- Faded in rather than drawn at once: an entry Rust answers for in the
		     same frame replaces this before it has been seen, and only one that
		     waits on a save lets it arrive. -->
		<p class="mt-5 animate-fade text-fine text-txt4">Opening…</p>
	</div>
</section>
