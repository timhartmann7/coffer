<script lang="ts">
	import { deleted } from '$lib/bin';
	import { marked } from '$lib/context.svelte';
	import type { Group } from '$lib/model';
	import { entriesOf } from '$lib/tree';
	import Icon from './Icon.svelte';

	/**
	 * The folders inside the part of the recycle bin being shown, above its
	 * entries, each a row that opens it.
	 *
	 * A deleted folder used to be poured out into the bin: its entries mixed in
	 * with everything else deleted, and nothing to say they had been together.
	 * It is a folder here, and it says when it went and where from.
	 *
	 * Drawn in the language of the list it sits at the top of: a band with a
	 * hairline under it in the wide list, a card beside an open entry.
	 */
	let {
		folders,
		root,
		now,
		compact,
		onOpen,
		onMenu
	}: {
		folders: Group[];
		root: Group;
		now: Date;
		compact: boolean;
		onOpen: (id: string) => void;
		/** A right-click on a folder, which draws Coffer's menu for it. */
		onMenu: (event: MouseEvent, folder: Group) => void;
	} = $props();
</script>

{#each folders as folder (folder.id)}
	{@const said = folder.binned ? deleted(folder.binned, root, now) : ''}
	<button
		type="button"
		data-folder
		data-menu={marked('folder', folder.id) || undefined}
		onclick={() => onOpen(folder.id)}
		oncontextmenu={(event) => onMenu(event, folder)}
		class={compact
			? 'mb-1 block w-full rounded-sm border border-transparent px-3 py-2.5 text-left transition hover:border-hairline hover:bg-raised/50'
			: 'flex w-full items-center gap-4 border-b border-line px-5 py-[11px] text-left transition-colors hover:bg-raised/50'}
	>
		<span class="block min-w-0 flex-1">
			<span class="flex min-w-0 items-center gap-2">
				<Icon name="folder" class="h-3.5 w-3.5 shrink-0 text-txt4" />
				<span class="min-w-0 flex-1 truncate text-body text-txt2">{folder.name}</span>
			</span>
			<span class="mt-1 block truncate pl-[22px] font-mono text-sub text-txt4">{said}</span>
		</span>
		{#if !compact}
			<span class="shrink-0 font-mono text-meta text-txt4">{entriesOf(folder).length}</span>
		{/if}
	</button>
{/each}
