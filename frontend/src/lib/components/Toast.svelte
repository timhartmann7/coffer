<script lang="ts">
	import Icon from './Icon.svelte';

	/**
	 * What just happened, bottom right, for as long as it is true.
	 *
	 * The bar underneath drains for exactly the time the clipboard holds the
	 * value, so the message and the fact agree with each other.
	 */
	let { message, draining = false }: { message: string; draining?: boolean } = $props();
</script>

<div
	class="pointer-events-none absolute right-5 bottom-5 w-[292px] overflow-hidden rounded-sm border border-hairline bg-raised"
>
	<div class="flex items-center gap-3 px-4 py-3">
		<Icon name="copy" class="h-4 w-4 shrink-0 text-accent" />
		<span class="flex-1 text-small text-txt">{message}</span>
	</div>
	{#if draining}
		<span class="block h-[2px] w-full origin-left animate-drain-clipboard bg-accent"></span>
	{/if}
</div>
