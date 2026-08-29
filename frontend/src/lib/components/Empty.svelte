<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';

	/** What a pane says when there is nothing in it. Silence would leave the
	 * reader wondering whether something failed. */
	let {
		icon,
		title,
		detail,
		action
	}: { icon: string; title: string; detail: string; action?: Snippet } = $props();
</script>

<div class="flex flex-1 flex-col items-center justify-center px-8 text-center">
	<Icon name={icon} class="h-7 w-7 text-txt4" />
	<p class="mt-5 text-base text-txt">{title}</p>
	<p class="mt-2 max-w-[34ch] text-fine leading-relaxed text-txt2">{detail}</p>
	{#if action}
		<div class="mt-6">{@render action()}</div>
	{/if}
</div>
