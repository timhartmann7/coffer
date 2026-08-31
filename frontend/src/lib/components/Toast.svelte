<script lang="ts">
	import Icon from './Icon.svelte';

	/**
	 * What just happened, bottom right, for a few seconds.
	 *
	 * It says what was done and how long the clipboard will hold it, and then it
	 * goes. It used to stay for the whole minute and count every second of it
	 * down, which is a notice about something already decided sitting on the
	 * screen for a minute.
	 *
	 * It arrives on a keyframe and it leaves on a transition, which are two
	 * mechanisms for one movement and the reason is the movement itself: nothing
	 * can animate an element that has already been taken out of the document, so
	 * a notice that is going has to still be here while it goes. `leaving` is
	 * what says it is, and the window takes it away once it has.
	 */
	let { message, kind, leaving }: { message: string; kind: 'copied' | 'failed'; leaving: boolean } =
		$props();
</script>

<div
	data-notice
	class="pointer-events-none absolute right-5 bottom-5 z-40 w-[292px] animate-rise overflow-hidden rounded-sm border border-hairline bg-raised transition duration-[170ms] ease-out {leaving
		? 'translate-y-2 opacity-0'
		: ''}"
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
