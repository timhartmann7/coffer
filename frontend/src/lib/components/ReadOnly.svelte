<script lang="ts">
	import type { ReadOnlyBecause } from '$lib/model';
	import { leaves } from '$lib/popup';

	/**
	 * The status bar's "Read only", which says why when it is pressed.
	 *
	 * The two words used to stand for four different things, and three of them
	 * have a way on that nothing said. The note says which it is and, where Rust
	 * says what is open can be written somewhere else, offers that: a backup
	 * and a place are about where the file is, and a copy is written somewhere
	 * else, while a format Coffer will not write is about the bytes, which a
	 * copy would carry with it. There it only explains.
	 *
	 * A disclosure, like the chips in the settings: it closes when the focus
	 * leaves it, on Escape - which it keeps from the window, where Escape closes
	 * the entry - and when its button is pressed again.
	 *
	 * WebKit gives a button no focus when it is clicked, so a note opened with
	 * a pointer would hold nothing that hears either: Escape would go to the
	 * window and close the entry under it, and a press elsewhere would leave it
	 * up. Opening it puts the focus on its button, which is where a key opening
	 * it leaves it anyway, and the copy's button keeps it there when it is
	 * pressed, so the press is not a focus leaving that closes the note first.
	 */
	let {
		because,
		copyable,
		onCopy
	}: {
		because: ReadOnlyBecause;
		/** Whether what is open can be written to a file somewhere else. */
		copyable: boolean;
		/** Writes it there, through the system's save panel, and says where. */
		onCopy: () => Promise<void>;
	} = $props();

	let open = $state(false);
	let wrapper = $state<HTMLElement>();
	let trigger = $state<HTMLButtonElement>();

	/** Why, for each reason, in the words a reader who never heard of a file
	 * format can act on. */
	const said: Record<ReadOnlyBecause, string> = {
		snapshot:
			'This is a backup, opened to look at. Coffer never writes to a backup: the next save of your vault would push the change out of the ten it keeps.',
		place:
			'This vault is kept somewhere that will not take a file: a locked folder, a disk image or a stick mounted read only, a backup disk, or a share you can only read. Coffer can read it there but cannot save to it.',
		kdb: 'This vault is in KDB, an older format Coffer reads and does not write, so nothing in it can be changed here.',
		kdbx3:
			'This vault is in KDBX 3 and holds files, which Coffer cannot write back without losing some of them, so nothing in it can be changed here.'
	};

	function left(event: FocusEvent) {
		if (leaves(event, wrapper)) open = false;
	}

	function keys(event: KeyboardEvent) {
		if (event.key !== 'Escape' || !open) return;
		// The window reads Escape as "close the entry", and a note put away is
		// not an entry put away.
		event.stopPropagation();
		open = false;
		trigger?.focus();
	}

	function toggle() {
		open = !open;
		if (open) trigger?.focus();
	}

	function copy() {
		open = false;
		trigger?.focus();
		void onCopy();
	}
</script>

<span bind:this={wrapper} class="relative" onfocusout={left}>
	<button
		bind:this={trigger}
		type="button"
		aria-expanded={open}
		aria-controls="read-only-note"
		onclick={toggle}
		onkeydown={keys}
		class="tracking-label text-txt3 uppercase transition-colors hover:text-txt2 active:text-txt4"
	>
		Read only
	</button>

	{#if open}
		<div
			id="read-only-note"
			class="absolute right-0 bottom-full z-30 mb-3 w-[292px] origin-bottom-right animate-pop rounded-sm border border-hairline bg-raised p-3 text-left font-sans tracking-normal normal-case"
		>
			<p class="text-fine leading-relaxed text-txt2">{said[because]}</p>
			{#if copyable}
				<button
					type="button"
					onclick={copy}
					onkeydown={keys}
					onmousedown={(event) => event.preventDefault()}
					class="mt-3 flex h-7 items-center rounded-full border border-hairline px-3 text-fine text-txt2 transition hover:border-txt3 hover:text-txt active:bg-raised"
				>
					Save a copy somewhere else…
				</button>
			{/if}
		</div>
	{/if}
</span>
