<script lang="ts">
	import type { Group } from '$lib/model';
	import { entriesOf } from '$lib/tree';
	import Icon from './Icon.svelte';
	import Tree from './Tree.svelte';

	let {
		group,
		depth = 0,
		selected,
		expanded,
		onSelect,
		onToggle
	}: {
		group: Group;
		depth?: number;
		/** The group being shown, or `null` for every entry in the vault. */
		selected: string | null;
		expanded: Set<string>;
		onSelect: (id: string) => void;
		onToggle: (id: string) => void;
	} = $props();

	const count = $derived(entriesOf(group).length);
	const open = $derived(expanded.has(group.id));
	const here = $derived(selected === group.id);
	// Indentation is drawn as spacers rather than as padding, so that a group a
	// hundred deep still lines up and the selected row still runs the full width
	// of the pane.
	const steps = $derived(Array.from({ length: depth }, (_, step) => step));
</script>

<div
	class="relative flex items-center gap-1 rounded-sm px-2 py-[7px] transition-colors {here
		? 'bg-raised text-txt'
		: 'text-txt2 hover:bg-raised/60'}"
>
	{#if here}
		<span
			class="absolute top-1 left-0 h-[calc(100%-8px)] w-[2px] rounded-full bg-accent"
			aria-hidden="true"
		></span>
	{/if}

	{#each steps as step (step)}
		<span class="w-4 shrink-0" aria-hidden="true"></span>
	{/each}

	{#if group.sections.length > 0}
		<button
			type="button"
			onclick={() => onToggle(group.id)}
			class="shrink-0 text-txt4 transition-colors hover:text-txt2"
			aria-label={open ? `Collapse ${group.name}` : `Expand ${group.name}`}
		>
			<Icon name={open ? 'chev-d' : 'chev-r'} class="h-4 w-4" />
		</button>
	{/if}

	<button
		type="button"
		onclick={() => onSelect(group.id)}
		class="flex min-w-0 flex-1 items-center gap-2 text-left"
	>
		<Icon name="folder" class="h-4 w-4 shrink-0 {here ? 'text-accent' : 'text-txt4'}" />
		<span class="flex-1 truncate">{group.name}</span>
		<span class="shrink-0 font-mono text-meta {here ? 'text-txt3' : 'text-txt4'}">{count}</span>
	</button>
</div>

{#if open}
	{#each group.sections as section (section.id)}
		{#if !section.isRecycleBin}
			<Tree group={section} depth={depth + 1} {selected} {expanded} {onSelect} {onToggle} />
		{/if}
	{/each}
{/if}
