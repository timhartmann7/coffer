<script lang="ts">
	/**
	 * A value the reader changes where it stands.
	 *
	 * There is no edit mode and no save button, which is what the mockup asks
	 * for: the value is a field, and what is in it when the focus leaves is what
	 * gets written. Escape puts back what was there and Enter finishes a single
	 * line.
	 *
	 * Nothing is written when nothing changed, so clicking through an entry does
	 * not stamp a modification time on it.
	 */
	let {
		value,
		label,
		placeholder = '',
		multiline = false,
		mono = false,
		classes = 'text-body text-txt',
		onCommit
	}: {
		value: string;
		/** What a screen reader calls this field. */
		label: string;
		placeholder?: string;
		multiline?: boolean;
		mono?: boolean;
		/** The type this value is drawn in. A title is a title wherever it is
		 * being edited. */
		classes?: string;
		onCommit: (value: string) => void;
	} = $props();

	let node = $state<HTMLInputElement | HTMLTextAreaElement>();

	function commit() {
		if (node && node.value !== value) onCommit(node.value);
	}

	function keys(event: KeyboardEvent) {
		if (!node) return;
		if (event.key === 'Escape') {
			node.value = value;
			node.blur();
		}
		if (event.key === 'Enter' && !multiline) {
			event.preventDefault();
			node.blur();
		}
	}
</script>

{#if multiline}
	<textarea
		bind:this={node}
		{value}
		{placeholder}
		aria-label={label}
		rows="4"
		spellcheck="false"
		onblur={commit}
		onkeydown={keys}
		class="mt-3 block w-full resize-none rounded-sm border border-transparent bg-transparent px-2 py-1.5 text-small leading-relaxed text-txt2 transition-colors outline-none placeholder:text-txt4 hover:border-hairline focus:border-accent focus:bg-surface2 focus:text-txt focus:ring-4 focus:ring-accent/15"
	></textarea>
{:else}
	<input
		bind:this={node}
		{value}
		{placeholder}
		aria-label={label}
		type="text"
		autocomplete="off"
		spellcheck="false"
		onblur={commit}
		onkeydown={keys}
		class="min-w-0 flex-1 truncate rounded-sm border border-transparent bg-transparent px-2 py-1 transition-colors outline-none placeholder:text-txt4 hover:border-hairline focus:border-accent focus:bg-surface2 focus:ring-4 focus:ring-accent/15 {classes} {mono
			? 'font-mono'
			: ''}"
	/>
{/if}
