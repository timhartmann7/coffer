<script lang="ts">
	import { copying, typing } from '$lib/keys';
	import type { Span } from '$lib/model';
	import { Revealed } from '$lib/reveal.svelte';
	import Icon from './Icon.svelte';
	import Shown from './Shown.svelte';

	/**
	 * A value the database protects that is not the entry's password: a field of
	 * the reader's own, a login or an address a foreign database protects, a
	 * protected note, or any of them as a previous version held it. A mask, an
	 * eye to read it for half a minute, and a copy that goes through Rust.
	 *
	 * Reading only. Changing one is a press of its own beside the row, which the
	 * entry draws (`Change.svelte`); a version cannot be changed at all.
	 *
	 * One of these per value on the screen. Each holds the node its own value
	 * goes into, which is what keeps the value of the field that was asked for
	 * out of the row of the field that was not.
	 */
	let {
		entry,
		field,
		label = field,
		read,
		bare = false,
		lines = false,
		onCopy,
		onFailure
	}: {
		entry: string;
		field: string;
		/** What the eye and the copy are called, when the value is not the
		 * entry's own. */
		label?: string;
		/** Where the value comes from: the entry as it is, or one of its
		 * previous versions. */
		read?: (entry: string, field: string) => Promise<string>;
		/** Drawn without the field's box. A previous version is a reading and
		 * not a field: nothing in that list can be written into, and a row of
		 * boxes there would offer an edit the format has no way to take. */
		bare?: boolean;
		/** A value that is prose in lines, a note. */
		lines?: boolean;
		/** Copies the value in Rust, whole or the part of it that was selected. */
		onCopy: (range: Span | null) => void;
		onFailure: (thrown: unknown) => void;
	} = $props();

	const revealed = new Revealed();
	let node = $state<HTMLElement>();
	let eye = $state<HTMLButtonElement>();

	// A different entry is a different secret. `field` is not read: the lists
	// this row is drawn in are keyed by the field's name, so a rename destroys
	// the row rather than changing it under itself.
	$effect(() => {
		void entry;
		return () => revealed.hide();
	});

	/**
	 * Shows the value, and puts it away again.
	 *
	 * Once it is on the screen the focus goes to the eye that hides it, so that
	 * Cmd+C copies this value through Rust and a stray key lands on a button
	 * rather than in the page. Never out of a field the reader is writing in: a
	 * new value half typed beside the old one is theirs to finish.
	 */
	async function toggle() {
		if (revealed.showing) {
			revealed.hide();
			return;
		}
		if (!node) return;
		try {
			await revealed.show(node, entry, field, read);
			if (!typing(document.activeElement)) eye?.focus();
		} catch (thrown) {
			onFailure(thrown);
		}
	}

	/** Cmd+C with the focus on this row copies this row's value, whole. */
	function keys(event: KeyboardEvent) {
		if (!copying(event)) return;
		event.preventDefault();
		onCopy(null);
	}
</script>

<Shown {revealed} bind:node {bare} {lines} {onCopy} />
<!-- Neither press moves the focus, so a new value being written beside this
     row keeps it, and the old one can be looked at and copied from while it is
     being replaced. -->
<button
	bind:this={eye}
	type="button"
	onmousedown={(event) => event.preventDefault()}
	onclick={toggle}
	onkeydown={keys}
	class="shrink-0 text-txt4 transition-colors hover:text-txt2"
	aria-label={revealed.showing ? `Hide ${label}` : `Show ${label}`}
>
	<Icon name={revealed.showing ? 'eye-off' : 'eye'} class="h-4 w-4" />
</button>
<button
	type="button"
	onmousedown={(event) => event.preventDefault()}
	onclick={() => onCopy(null)}
	onkeydown={keys}
	class="shrink-0 text-txt4 transition-colors hover:text-txt2"
	aria-label="Copy {label}"
>
	<Icon name="copy" class="h-4 w-4" />
</button>
