<script lang="ts">
	import Icon from './Icon.svelte';

	/**
	 * What just happened, bottom right, for as long as it is true.
	 *
	 * The bar underneath drains for exactly the time the clipboard holds the
	 * value, so the message and the fact agree with each other. It belongs to a
	 * copy that worked; the accent means "this is finishing by itself" and
	 * nothing that went wrong is finishing by itself.
	 */
	let { message, kind }: { message: string; kind: 'copied' | 'failed' } = $props();
</script>

<div
	data-notice
	class="pointer-events-none absolute right-5 bottom-5 w-[292px] overflow-hidden rounded-sm border border-hairline bg-raised"
>
	<div class="flex items-center gap-3 px-4 py-3">
		{#if kind === 'copied'}
			<Icon name="copy" class="h-4 w-4 shrink-0 text-accent" />
		{:else}
			<Icon name="warn" class="h-4 w-4 shrink-0 text-warn" />
		{/if}
		<span class="flex-1 text-small text-txt">{message}</span>
	</div>
	{#if kind === 'copied'}
		<span class="block h-[2px] w-full origin-left animate-drain-clipboard bg-accent"></span>
	{/if}
</div>
