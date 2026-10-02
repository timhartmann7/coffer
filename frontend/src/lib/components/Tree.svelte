<script lang="ts">
	import { marked } from '$lib/context.svelte';
	import { LANDING } from '$lib/dragging.svelte';
	import type { Group } from '$lib/model';
	import { visible } from '$lib/tree';
	import Icon from './Icon.svelte';

	let {
		root,
		selected,
		expanded,
		drop = null,
		lifted = null,
		onSelect,
		onToggle,
		onPress,
		onMenu
	}: {
		root: Group;
		/** The group being shown, or `null` for every entry in the vault. */
		selected: string | null;
		expanded: Set<string>;
		/** The `data-drop` key of the place a drag would land in now. */
		drop?: string | null;
		/** The folder being dragged. */
		lifted?: string | null;
		onSelect: (id: string) => void;
		onToggle: (id: string) => void;
		/** A press on a folder, which a drag may start from. Left out where
		 * nothing may be moved. */
		onPress?: (event: PointerEvent, group: Group) => void;
		/** A right-click anywhere on a folder's line, its chevron included,
		 * which draws Coffer's menu for the folder. */
		onMenu: (event: MouseEvent, group: Group) => void;
	} = $props();

	const lines = $derived(visible(root, expanded));

	/**
	 * Indentation is drawn as spacers rather than as padding, so that the
	 * selected line still runs the full width of the pane. They give way before
	 * the name does, so a group a hundred deep is still readable.
	 */
	const steps = (depth: number) => Array.from({ length: depth }, (_, step) => step);
</script>

{#each lines as { group, depth, entries } (group.id)}
	{@const here = selected === group.id}
	{@const landing = drop === group.id}
	<div
		role="presentation"
		data-drop={group.id}
		data-into={group.id}
		data-menu={marked('folder', group.id) || undefined}
		oncontextmenu={(event) => onMenu(event, group)}
		data-opens={group.sections.length > 0 && !expanded.has(group.id) ? group.id : undefined}
		class="relative flex items-center gap-1 rounded-sm px-2 py-[7px] transition-colors {landing
			? LANDING
			: here || lifted === group.id
				? 'bg-raised text-txt'
				: 'text-txt2 hover:bg-raised/60'}"
	>
		{#if here}
			<span
				class="absolute top-1 left-0 h-[calc(100%-8px)] w-[2px] rounded-full bg-accent"
				aria-hidden="true"
			></span>
		{/if}

		{#each steps(depth) as step (step)}
			<span class="w-4" aria-hidden="true"></span>
		{/each}

		{#if group.sections.length > 0}
			<button
				type="button"
				onclick={() => onToggle(group.id)}
				class="shrink-0 text-txt4 transition-colors hover:text-txt2"
				aria-label={expanded.has(group.id) ? `Collapse ${group.name}` : `Expand ${group.name}`}
			>
				<Icon name={expanded.has(group.id) ? 'chev-d' : 'chev-r'} class="h-4 w-4" />
			</button>
		{/if}

		<button
			type="button"
			onclick={() => onSelect(group.id)}
			onpointerdown={(event) => onPress?.(event, group)}
			class="flex min-w-0 flex-1 items-center gap-2 text-left"
		>
			<Icon
				name="folder"
				class="h-4 w-4 shrink-0 {here || landing ? 'text-accent' : 'text-txt4'}"
			/>
			<span class="flex-1 truncate">{group.name}</span>
			<span class="shrink-0 font-mono text-meta {here ? 'text-txt3' : 'text-txt4'}">{entries}</span>
		</button>
	</div>
{/each}
