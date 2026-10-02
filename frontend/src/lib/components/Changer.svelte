<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { Place } from '$lib/drafts';
	import { quoted } from '$lib/format';
	import Change from './Change.svelte';

	/**
	 * The way to write a new value into a field the database protects, on the
	 * line under the field: a Change in words, and in its place, once pressed, a
	 * field of its own with Save and Cancel (`Change.svelte`).
	 *
	 * One of these per field on the screen, each with its own field, because a
	 * field left with something typed in it asks what to do with it and waits
	 * for the answer - and the next one may be opened meanwhile.
	 */
	let {
		entry,
		name,
		multiline,
		draft,
		disabled = false,
		open = $bindable(false),
		onSave,
		onFailure,
		children
	}: {
		/** The entry the field is on. Another entry is another field, and one
		 * opened for the last entry is put away. */
		entry: string;
		/** The field's name in the file. */
		name: string;
		/** Written in lines: the value it replaces is in lines, or the reader
		 * asked for lines when they named the field, or it is the notes. */
		multiline: boolean;
		draft: Place;
		/** Change may not be pressed: something else about the field is on its
		 * way, and the field it would open could be gone when that lands. */
		disabled?: boolean;
		/** Whether the field for the new value is open. */
		open?: boolean;
		/** Writes the new value. Rejects with what the vault said when it would
		 * not take it. */
		onSave: (value: string) => Promise<void>;
		onFailure: (thrown: unknown) => void;
		/** Whatever else the line offers beside Change. It goes while the field
		 * is open: the reader is writing a value of their own there. */
		children?: Snippet;
	} = $props();

	/**
	 * Whether the field just closed with the focus in it.
	 *
	 * Change comes back in place of the field and takes the focus back as it is
	 * drawn, rather than the focus falling out of the pane - where the next
	 * Escape closes the entry. Nothing is drawn from it, so it is not state.
	 */
	let returning = false;

	/** Which entry the line is under: its own value, because the pane hands the
	 * id on from an entry that is a new object after every change. */
	const showing = $derived(entry);

	// Cleared on the way out rather than on the way in: a write inside the body
	// of an effect is a read of what was there, and an effect that reads what
	// it writes runs again the moment anything sets it.
	$effect(() => {
		void showing;
		return () => {
			open = false;
			returning = false;
		};
	});

	function closed(back: boolean) {
		returning = back;
		open = false;
	}

	function returned(button: HTMLButtonElement) {
		if (!returning) return;
		returning = false;
		button.focus();
	}
</script>

{#if open}
	<Change
		class="mt-2 animate-rise"
		label="New value of {name}"
		placeholder="New value"
		what="value of {quoted(name)}"
		{multiline}
		{draft}
		{onSave}
		onClose={closed}
		{onFailure}
	/>
{:else}
	<div class="mt-1 flex flex-wrap items-center gap-x-3">
		<button
			{@attach returned}
			type="button"
			onclick={() => (open = true)}
			{disabled}
			class="text-fine text-txt3 transition-colors hover:text-txt disabled:cursor-not-allowed disabled:hover:text-txt3"
			aria-label="Change {name}"
		>
			Change
		</button>
		{@render children?.()}
	</div>
{/if}
