<script lang="ts">
	import type { EntryRow } from '$lib/model';
	import Mask from './Mask.svelte';

	/** The list as it is drawn beside an open entry: two lines, no columns, and
	 * the open one marked with the same bar the tree uses. */
	let {
		rows,
		open,
		onOpen
	}: { rows: EntryRow[]; open: string | null; onOpen: (id: string) => void } = $props();

	/**
	 * The second line of a row. A value the database protects is not here to
	 * write, so it is left out rather than replaced by something that would read
	 * as an entry without one.
	 */
	function subtitle(row: EntryRow): string {
		const parts = [row.username, row.url].filter(
			(value): value is string => value !== null && value !== ''
		);
		if (parts.length > 0) return parts.join(' · ');
		return row.username === '' ? 'no login' : '';
	}
</script>

<div class="flex-1 overflow-y-auto">
	{#each rows as row (row.id)}
		{@const here = row.id === open}
		<button
			type="button"
			onclick={() => onOpen(row.id)}
			class="relative block w-full border-b border-line px-4 py-3 text-left transition-colors {here
				? 'bg-raised'
				: 'hover:bg-raised/50'}"
		>
			{#if here}
				<span class="absolute top-0 left-0 h-full w-[2px] bg-accent" aria-hidden="true"></span>
			{/if}
			<div class="truncate text-body {here ? 'text-txt' : 'text-txt2'}">
				{#if row.title === null}
					<Mask />
				{:else}
					{row.title}
				{/if}
			</div>
			<div class="mt-1 truncate font-mono text-sub {here ? 'text-txt3' : 'text-txt4'}">
				{subtitle(row)}
			</div>
		</button>
	{/each}
</div>
