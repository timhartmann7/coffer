<script lang="ts">
	import { tick } from 'svelte';
	import { Secret } from '$lib/secret.svelte';
	import Field from './Field.svelte';
	import Icon from './Icon.svelte';
	import Mask from './Mask.svelte';

	/**
	 * A value the database protects that is not the entry's password: a field of
	 * the reader's own, or an address a foreign database keeps protected.
	 *
	 * `ProtectedValue` draws the same value where nothing may be done to it - a
	 * previous version, which the format gives no way to write into. This is the
	 * row for one that can be changed, and it exists because the reading-only row
	 * was being used for both. A protected field of the reader's own could be
	 * looked at and nothing else: to change an API token they had to delete the
	 * field and make it again, and to use one they revealed it and pressed
	 * Cmd+C - an ordinary pasteboard write with none of the markers Coffer's own
	 * copy carries, which is how a password lands in Maccy.
	 *
	 * The value is never in this component. Showing it writes it into the input
	 * that displays it and nowhere else, and what is in that input when the focus
	 * leaves is written back only if the reader put it there. `Secret` is what
	 * knows the difference; the same rule draws the password row.
	 */
	let {
		entry,
		field,
		readOnly,
		onCopy,
		onCommit,
		onFailure
	}: {
		entry: string;
		field: string;
		/** A database Coffer will not write back: the value can still be shown
		 * and copied, and nothing here offers to change it. */
		readOnly: boolean;
		onCopy: () => void;
		onCommit: (value: string) => void;
		onFailure: (thrown: unknown) => void;
	} = $props();

	const secret = new Secret();
	let node = $state<HTMLInputElement>();

	// A different entry is a different secret. `field` is not read: the list this
	// row is drawn in is keyed by the field's name, so a rename destroys the row
	// rather than changing it under itself.
	$effect(() => {
		void entry;
		return () => secret.close(node);
	});

	async function toggle() {
		if (secret.live) {
			settle();
			return;
		}
		if (!node) return;

		try {
			await secret.show(node, entry, field);
			// The field is hidden until the value is on the screen, so it can
			// only be focused after the screen has caught up with that.
			await tick();
			node?.focus();
		} catch (thrown) {
			onFailure(thrown);
		}
	}

	/** A value the reader wrote is written back; one Coffer put on the screen is
	 * put away. `null` is the second, and an empty string is a value somebody
	 * deliberately cleared. */
	function settle() {
		const written = secret.settle(node);
		if (written === null || readOnly) return;
		onCommit(written);
	}

	function keys(event: KeyboardEvent) {
		if (event.key === 'Escape') secret.close(node);
		if (event.key === 'Enter') {
			event.preventDefault();
			settle();
		}
	}
</script>

<Field>
	{#if !secret.live}
		<span class="min-w-0 flex-1 overflow-hidden"><Mask /></span>
	{/if}
	<!-- The value lives here and nowhere else. -->
	<input
		bind:this={node}
		data-value
		type="text"
		autocomplete="off"
		spellcheck="false"
		aria-label={field}
		hidden={!secret.live}
		readonly={readOnly}
		oninput={() => secret.written()}
		onblur={settle}
		onkeydown={keys}
		class="min-w-0 flex-1 bg-transparent font-mono text-small text-txt outline-none"
	/>

	{#if secret.showing}
		<span class="shrink-0 animate-fade font-mono text-label text-accent">
			<span class="sr-only">Hides in </span>0:{String(secret.left).padStart(2, '0')}
		</span>
		<span class="absolute inset-x-0 bottom-0 h-[2px] bg-line" aria-hidden="true">
			<span class="block h-full w-full origin-left animate-drain-reveal bg-accent"></span>
		</span>
	{/if}
</Field>
<!-- While something is on the screen this press does not move the focus, so it
     reaches `toggle` before the field's own blur does. The other way round, Hide
     wrote the value back and then opened it again. -->
<button
	type="button"
	onmousedown={(event) => secret.live && event.preventDefault()}
	onclick={toggle}
	class="shrink-0 text-txt4 transition-colors hover:text-txt2"
	aria-label={secret.live ? `Hide ${field}` : `Show ${field}`}
>
	<Icon name={secret.live ? 'eye-off' : 'eye'} class="h-4 w-4" />
</button>
<button
	type="button"
	onclick={onCopy}
	class="shrink-0 text-txt4 transition-colors hover:text-txt2"
	aria-label="Copy {field}"
>
	<Icon name="copy" class="h-4 w-4" />
</button>
