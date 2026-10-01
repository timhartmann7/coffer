<script lang="ts">
	import type { Notice } from '$lib/notices.svelte';
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
	 *
	 * A notice about something that can be taken back carries the one button
	 * that does it. The notice lets the pointer through everywhere else, because
	 * it sits over the corner of the list and a sentence is not something to
	 * click on; the button is the one part that takes a press.
	 */
	let {
		message,
		kind,
		leaving,
		onUndo
	}: {
		message: string;
		kind: Notice['kind'];
		leaving: boolean;
		/** Takes back what the notice is about. The window decides for how long
		 * that is on offer, and takes the notice away when it no longer is. */
		onUndo?: () => void;
	} = $props();
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
		{:else if kind === 'removed'}
			<Icon name="trash" class="h-4 w-4 shrink-0 text-txt3" />
		{:else}
			<Icon name="warn" class="h-4 w-4 shrink-0 text-warn" />
		{/if}
		<!-- A field's name is a value out of somebody's database and can be a
		     megabyte long, so the sentence that names one is bounded. -->
		<span class="line-clamp-3 min-w-0 flex-1 text-small break-words text-txt">{message}</span>
		{#if onUndo}
			<!-- The press does not take the focus. A field holding typing would
			     otherwise be left, and write what it holds, on the way to the
			     button - a change of its own, landing in the middle of the undo. -->
			<button
				type="button"
				onmousedown={(event) => event.preventDefault()}
				onclick={onUndo}
				aria-keyshortcuts="Meta+Z"
				class="pointer-events-auto -my-1 flex h-7 shrink-0 items-center gap-2 rounded-full px-2.5 text-small text-accent transition-colors hover:text-accenthi"
			>
				Undo
				<kbd class="rounded-xs border border-hairline px-1.5 py-0.5 font-mono text-label text-txt4">
					⌘Z
				</kbd>
			</button>
		{/if}
	</div>
</div>
