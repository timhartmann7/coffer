<script lang="ts">
	import { Revealed } from '$lib/reveal.svelte';
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
		onFailure
	}: {
		entry: string;
		field: string;
		/** What the eye is called, when the value is not the entry's own. */
		label?: string;
		/** Where the value comes from: the entry as it is, or one of its
		 * previous versions. */
		read?: (entry: string, field: string) => Promise<string>;
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

<span class="min-w-0 flex-1 overflow-hidden">
	{#if !revealed.showing}
		<Mask />
	{/if}
	<span
		bind:this={node}
		data-value
		class="block font-mono text-small leading-snug break-all text-txt"
	></span>
</span>
<button
	type="button"
	onclick={toggle}
	class="shrink-0 text-txt4 transition-colors hover:text-txt2"
	aria-label={revealed.showing ? `Hide ${label}` : `Show ${label}`}
>
	<Icon name="eye" class="h-4 w-4" />
</button>
