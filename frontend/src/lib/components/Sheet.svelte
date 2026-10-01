<script lang="ts">
	import type { Snippet } from 'svelte';
	import { cancels } from '$lib/lines';
	import { cover } from '$lib/menu.svelte';

	/**
	 * A sheet over the window, under the title bar: the conflict dialog's veil
	 * and card, with whatever the sheet is for inside.
	 *
	 * Every key is its own while it is up, wherever the focus is. One Shift+Tab
	 * from its first button is the title bar, which stays above the veil, and a
	 * press on the title bar puts the focus nowhere; a key from either still
	 * reaches the window, where Cmd+C would copy a password out of the pane
	 * under the sheet and Escape would put that pane away as well. So a key
	 * aimed outside the card is stopped as the window first sees it, and one
	 * aimed inside on its way back out, after whatever in the card it was for.
	 * Escape, a press on the veil and anything chosen in the menu bar put the
	 * sheet away; the focus comes in on its first button.
	 */
	let {
		title,
		onClose,
		children
	}: {
		title: string;
		onClose: () => void;
		children: Snippet;
	} = $props();

	const id = $props.id();
	let card = $state<HTMLElement>();
	/** Whether the focus goes back where it was when the sheet goes. Not when
	 * a choice in the menu bar put it away: that choice moves the focus itself,
	 * or leaves it for the screen it opens to take. */
	let restoring = true;

	$effect(() =>
		cover(() => {
			restoring = false;
			onClose();
		})
	);

	$effect(() => {
		const before = document.activeElement;
		(card?.querySelector('button') ?? card)?.focus();
		return () => {
			if (!restoring) return;
			// After the flush that took the sheet away, not during it: the
			// screens under it are still inert while it is being taken down, and
			// WebKit puts the focus nowhere in an inert screen. Not over anything
			// that has taken the focus since, either.
			queueMicrotask(() => {
				const now = document.activeElement;
				if (now !== null && now !== document.body) return;
				if (before instanceof HTMLElement && before.isConnected) before.focus();
			});
		};
	});

	function keys(event: KeyboardEvent) {
		event.stopPropagation();
		if (cancels(event)) onClose();
	}

	/** A key aimed outside the card, as the window first sees it. */
	function aside(event: KeyboardEvent) {
		if (event.target instanceof Node && card?.contains(event.target)) return;
		keys(event);
	}
</script>

<svelte:window onkeydowncapture={aside} />

<div
	role="presentation"
	onclick={(event) => event.target === event.currentTarget && onClose()}
	onkeydown={keys}
	class="absolute inset-0 z-40 flex animate-fade items-center justify-center bg-canvas/75 px-6"
>
	<div
		bind:this={card}
		role="dialog"
		aria-modal="true"
		aria-labelledby={id}
		tabindex="-1"
		class="max-h-full w-full max-w-[720px] animate-rise overflow-y-auto rounded-md border border-hairline bg-raised p-8 outline-none"
	>
		<h2 {id} class="text-title font-medium tracking-tight text-txt">{title}</h2>
		{@render children()}
	</div>
</div>
