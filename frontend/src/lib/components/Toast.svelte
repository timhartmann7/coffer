<script lang="ts">
	import Icon from './Icon.svelte';

	/**
	 * What just happened, bottom right, for a few seconds.
	 *
	 * It says what was done and how long the clipboard will hold it, and then it
	 * goes. It used to stay for the whole minute and count every second of it
	 * down, which is a notice about something already decided sitting on the
	 * screen for a minute.
	 */
	let { message, kind }: { message: string; kind: 'copied' | 'failed' } = $props();
</script>

<div
	data-notice
	class="pointer-events-none absolute right-5 bottom-5 w-[292px] animate-notice overflow-hidden rounded-sm border border-hairline bg-raised"
>
	<div class="flex items-center gap-3 px-4 py-3">
		{#if kind === 'copied'}
			<Icon name="copy" class="h-4 w-4 shrink-0 text-accent" />
		{:else}
			<Icon name="warn" class="h-4 w-4 shrink-0 text-warn" />
		{/if}
		<span class="flex-1 text-small text-txt">{message}</span>
	</div>
</div>
