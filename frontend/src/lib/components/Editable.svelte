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
		readonly = false,
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
		/** A database Coffer will not write back is one nothing here may offer to
		 * change. */
		readonly?: boolean;
		/** Answers whether the value was taken. A value that was refused is put
		 * back, so that the screen never shows something the vault does not have.
		 */
		onCommit: (value: string) => Promise<boolean>;
	} = $props();

	let node = $state<HTMLInputElement | HTMLTextAreaElement>();

	async function commit() {
		if (!node || node.value === value) return;
		const taken = await onCommit(node.value);
		// A refusal leaves the vault as it was, so the field goes back to what
		// the vault has rather than standing there showing something else.
		if (!taken && node) node.value = value;
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

{#if readonly}
	<!-- Nothing to write into, so the value is drawn as one. An empty one says
	     what is missing, in the colour of something that is not there. -->
	<span
		class="min-w-0 flex-1 px-2 py-1 break-words whitespace-pre-wrap {value === ''
			? 'text-body text-txt4'
			: classes} {mono ? 'font-mono' : ''}"
	>
		{value === '' ? placeholder : value}
	</span>
{:else if multiline}
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
