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
	let {
		message,
		kind,
		seconds = 60
	}: { message: string; kind: 'copied' | 'failed'; seconds?: number } = $props();

	/**
	 * How long the bar takes to empty, as a class rather than a duration.
	 *
	 * Tailwind reads class names out of the source, so a computed one compiles
	 * to nothing, and a strict `style-src` means the duration cannot be set from
	 * script either. The table is short because the list of timeouts is short,
	 * and `drain.test.ts` fails if Rust ever offers one that is not here.
	 */
	const draining: Record<number, string> = {
		15: 'animate-drain-clipboard-15',
		30: 'animate-drain-clipboard-30',
		60: 'animate-drain-clipboard-60',
		300: 'animate-drain-clipboard-300'
	};

	const drain = $derived(draining[seconds] ?? draining[60]);
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
		<span class="block h-[2px] w-full origin-left bg-accent {drain}"></span>
	{/if}
</div>
