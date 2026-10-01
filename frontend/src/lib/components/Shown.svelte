<script lang="ts">
	import type { Snippet } from 'svelte';
	import { sealed } from '$lib/guard';
	import type { Span } from '$lib/model';
	import type { Revealed } from '$lib/reveal.svelte';
	import Field from './Field.svelte';
	import Mask from './Mask.svelte';

	/**
	 * Where a value the database protects is read: a mask until somebody asks,
	 * the value as text for half a minute, and the half minute itself.
	 *
	 * Text, and never a field. The value used to be revealed into the input it
	 * was edited in, with the focus put there, so one stray key while a Wi-Fi
	 * password was on the screen for a guest changed the stored password and the
	 * next click anywhere saved it. Nothing here takes a key: changing a value is
	 * a press of its own (`Change.svelte`).
	 *
	 * Nothing here changes size when a value on one line is shown. The mask and
	 * the value share one box, and the countdown is the box's own bottom edge
	 * rather than a row that arrives under it. A value on one line stays on one
	 * line however long it is, and scrolls sideways the way the field it used to
	 * be shown in did. A value with line breaks in it takes the lines it has,
	 * because showing ten recovery codes on one line is showing something that
	 * is not in the vault.
	 */
	let {
		revealed,
		node = $bindable(),
		bare = false,
		lines = false,
		classes = 'text-small',
		hidden,
		onCopy
	}: {
		/** The clock the value is on, and whether it is on the screen at all. */
		revealed: Revealed;
		/** The node the value is written into and wiped from. */
		node?: HTMLElement;
		/** Drawn without {@link Field}'s box, for a reading rather than a field:
		 * a previous version. Long lines wrap there, and there is no countdown. */
		bare?: boolean;
		/** A value that is prose in lines, a note: its long lines wrap rather
		 * than scroll. */
		lines?: boolean;
		/** The size the value is drawn at. */
		classes?: string;
		/** What the box holds while nothing is shown. The mask, unless there is
		 * nothing to show at all. */
		hidden?: Snippet;
		/** Where a copy of the value goes instead of the system's pasteboard:
		 * Rust, with the part the reader selected or `null` for all of it. */
		onCopy: (range: Span | null) => void;
	} = $props();

	const flow = $derived(
		bare
			? 'break-all whitespace-pre-wrap'
			: lines
				? 'break-words whitespace-pre-wrap'
				: 'overflow-x-auto whitespace-pre [&::-webkit-scrollbar]:hidden'
	);
</script>

{#snippet value()}
	{#if !revealed.showing}
		{#if hidden}
			{@render hidden()}
		{:else}
			<span class="min-w-0 flex-1 overflow-hidden"><Mask /></span>
		{/if}
	{/if}
	<!-- The value lives here and nowhere else. Hidden rather than taken out, so
	     that the node a reveal writes into is there before it is asked for. -->
	<span
		bind:this={node}
		data-value
		hidden={!revealed.showing}
		{@attach sealed(onCopy)}
		class="min-w-0 flex-1 font-mono text-txt {classes} {flow}"
	></span>

	{#if revealed.showing && !bare}
		<!-- Four characters, because the value beside them is what the pane is
		     for and it is three hundred and eighty-four pixels wide. The
		     sentence the mockup writes out is still here for anything that
		     reads the screen aloud, where there is no such shortage. -->
		<span class="shrink-0 animate-fade font-mono text-label text-accent">
			<span class="sr-only">Hides in </span>0:{String(revealed.left).padStart(2, '0')}
		</span>
		<!-- The bar the mockup gives a countdown, drawn as the box's own bottom
		     edge. Anywhere else it is a row that arrives under the value and
		     pushes the rest of the entry down the screen. -->
		<span class="absolute inset-x-0 bottom-0 h-[2px] bg-line" aria-hidden="true">
			<span class="block h-full w-full origin-left animate-drain-reveal bg-accent"></span>
		</span>
	{/if}
{/snippet}

{#if bare}
	<span class="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">{@render value()}</span>
{:else}
	<Field>{@render value()}</Field>
{/if}
