<script lang="ts">
	import { Revealed } from '$lib/reveal.svelte';
	import Icon from './Icon.svelte';
	import Mask from './Mask.svelte';

	/** A custom field the database protects: a mask, and an eye to see it for
	 * half a minute. */
	let {
		entry,
		field,
		onFailure
	}: {
		entry: string;
		field: string;
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
			await revealed.show(node, entry, field);
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
		class="block font-mono text-small leading-snug break-all text-txt select-text"
	></span>
</span>
<button
	type="button"
	onclick={toggle}
	class="shrink-0 text-txt4 transition-colors hover:text-txt2"
	aria-label={revealed.showing ? `Hide ${field}` : `Show ${field}`}
>
	<Icon name="eye" class="h-4 w-4" />
</button>
