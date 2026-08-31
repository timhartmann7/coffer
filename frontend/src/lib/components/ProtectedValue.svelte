<script lang="ts">
	import { Revealed } from '$lib/reveal.svelte';
	import Field from './Field.svelte';
	import Icon from './Icon.svelte';
	import Mask from './Mask.svelte';

	/**
	 * A value the database protects: a mask, and an eye to see it for half a
	 * minute.
	 *
	 * One of these per value on the screen. Each holds the node its own value
	 * goes into, which is what keeps the value of the field that was asked for
	 * out of the row of the field that was not.
	 */
	let {
		entry,
		field,
		label = `${field}`,
		read,
		bare = false,
		onFailure
	}: {
		entry: string;
		field: string;
		/** What the eye is called, when the value is not the entry's own. */
		label?: string;
		/** Where the value comes from: the entry as it is, or one of its
		 * previous versions. */
		read?: (entry: string, field: string) => Promise<string>;
		/** Drawn without {@link Field}'s box. A previous version is a reading and
		 * not a field: nothing in that list can be written into, and a row of
		 * boxes there would offer an edit the format has no way to take. */
		bare?: boolean;
		onFailure: (thrown: unknown) => void;
	} = $props();

	const revealed = new Revealed();
	let node = $state<HTMLElement>();

	$effect(() => {
		void entry;
		return () => revealed.hide();
	});

	async function toggle() {
		if (revealed.showing) {
			revealed.hide();
			return;
		}
		if (!node) return;
		try {
			await revealed.show(node, entry, field, read);
		} catch (thrown) {
			onFailure(thrown);
		}
	}
</script>

{#snippet value()}
	{#if !revealed.showing}
		<span class="min-w-0 flex-1 overflow-hidden"><Mask /></span>
	{/if}
	<!-- The value lives here and nowhere else. Hidden rather than taken out, so
	     that the node a reveal writes into is there before it is asked for. -->
	<span
		bind:this={node}
		data-value
		hidden={!revealed.showing}
		class="min-w-0 flex-1 font-mono text-small break-all text-txt"
	></span>
{/snippet}

{#if bare}
	<span class="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">{@render value()}</span>
{:else}
	<!-- The same box the value would be edited in, so that a field the database
	     protects sits on the line its neighbours sit on rather than a thinner one
	     of its own, and so that revealing it moves nothing. -->
	<Field>{@render value()}</Field>
{/if}
<button
	type="button"
	onclick={toggle}
	class="shrink-0 text-txt4 transition-colors hover:text-txt2"
	aria-label={revealed.showing ? `Hide ${label}` : `Show ${label}`}
>
	<Icon name="eye" class="h-4 w-4" />
</button>
