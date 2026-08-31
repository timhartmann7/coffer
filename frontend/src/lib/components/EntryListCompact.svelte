<script lang="ts">
	import type { EntryRow } from '$lib/model';
	import Icon from './Icon.svelte';
	import Mask from './Mask.svelte';

	/**
	 * The list as it is drawn beside an open entry: two lines and no columns.
	 *
	 * Drawn as cards rather than as full-width bands. The mockup gives this list
	 * the same rule as the wide one - a hairline under every row and a plane
	 * behind the selected one - and at two hundred and thirty pixels that reads
	 * as one block of stripes rather than as a list of entries. So it takes the
	 * folder list's language instead, which is the same window's answer to the
	 * same question at the same width: a card that lifts under the pointer, and
	 * an accent bar down the side of the one being read.
	 */
	let {
		rows,
		open,
		onOpen,
		onDismiss
	}: {
		rows: EntryRow[];
		open: string | null;
		onOpen: (id: string) => void;
		/** What the empty part of the list under the rows does: it puts the open
		 * entry away, the way pressing Escape does. */
		onDismiss: () => void;
	} = $props();

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

<!--
	Only a press that landed on the empty part below the rows, which is what
	`currentTarget` says: every row is a button inside this, and a press on one of
	those is a press on a row and arrives here on its way up.
-->
<div
	role="presentation"
	onclick={(event) => event.target === event.currentTarget && onDismiss()}
	class="flex-1 overflow-y-auto p-2"
>
	{#each rows as row (row.id)}
		{@const here = row.id === open}
		<button
			type="button"
			onclick={() => onOpen(row.id)}
			class="relative mb-1 block w-full overflow-hidden rounded-sm border px-3 py-2.5 text-left transition {here
				? 'border-hairline bg-raised'
				: 'border-transparent hover:border-hairline hover:bg-raised/50'}"
		>
			{#if here}
				<span
					class="absolute top-1 bottom-1 left-0 w-[2px] rounded-full bg-accent"
					aria-hidden="true"
				></span>
			{/if}
			<span class="flex min-w-0 items-center gap-2">
				<Icon
					name={row.hasPassword || row.attachments === 0 ? 'key' : 'clip'}
					class="h-3.5 w-3.5 shrink-0 {here ? 'text-txt3' : 'text-txt4'}"
				/>
				<span class="min-w-0 flex-1 truncate text-body {here ? 'text-txt' : 'text-txt2'}">
					{#if row.title === null}
						<Mask />
					{:else}
						{row.title}
					{/if}
				</span>
			</span>
			<span
				class="mt-1 block truncate pl-[22px] font-mono text-sub {here ? 'text-txt3' : 'text-txt4'}"
			>
				{subtitle(row)}
			</span>
		</button>
	{/each}
</div>
