<script lang="ts">
	import { Revealed } from '$lib/reveal.svelte';
	import Icon from './Icon.svelte';
	import Mask from './Mask.svelte';

	let {
		entry,
		field,
		empty,
		onCopy,
		onFailure
	}: {
		entry: string;
		field: string;
		empty: boolean;
		onCopy: (field: string) => void;
		onFailure: (thrown: unknown) => void;
	} = $props();

	const revealed = new Revealed();
	let node = $state<HTMLElement>();

	// A different entry is a different secret. Whatever is on the screen goes
	// before the next one arrives.
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

<div class="mt-5">
	<span class="font-mono text-label tracking-label text-txt3 uppercase">Password</span>

	{#if empty}
		<div class="mt-1.5 text-body text-txt4">No password on this entry</div>
	{:else}
		<div class="mt-1.5 flex items-start gap-3">
			<!-- A value can be anything a file can hold: a megabyte of text, or
			     eight combining accents with no letter under them. It is clipped
			     to its own box so that whatever it is stays out of the label
			     above and out of the buttons beside it. -->
			<span class="min-w-0 flex-1 overflow-hidden">
				{#if !revealed.showing}
					<Mask />
				{/if}
				<!-- The value lives here and nowhere else. -->
				<span
					bind:this={node}
					class="block font-mono text-body leading-snug break-all text-txt select-text"
				></span>
			</span>
			<button
				type="button"
				onclick={toggle}
				class="flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-hairline px-3 text-fine text-txt2 transition-colors hover:border-txt3 hover:text-txt"
			>
				<Icon name="eye" class="h-3.5 w-3.5" />
				<span>{revealed.showing ? 'Hide' : 'Show'}</span>
			</button>
			<button
				type="button"
				onclick={() => onCopy(field)}
				class="flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-hairline px-3 text-fine text-txt2 transition-colors hover:border-txt3 hover:text-txt"
			>
				<Icon name="copy" class="h-3.5 w-3.5" /> Copy
			</button>
		</div>

		{#if revealed.showing}
			<div
				class="mt-2 flex items-center gap-2 font-mono text-label tracking-label text-accent uppercase"
			>
				<span>Hides in 0:{String(revealed.left).padStart(2, '0')}</span>
				<span class="h-[2px] w-16 overflow-hidden rounded-full bg-line">
					<span class="block h-full w-full origin-left animate-drain-reveal rounded-full bg-accent"
					></span>
				</span>
			</div>
		{/if}
	{/if}
</div>
