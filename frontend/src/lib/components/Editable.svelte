<script lang="ts">
	import Field from './Field.svelte';

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
		bare = false,
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
		/** Drawn without {@link Field}'s box, for a value that reads as a heading
		 * rather than as a field. The entry's title is the only one: a hairline
		 * around a nineteen-pixel name is a box around the name of the screen. */
		bare?: boolean;
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

{#snippet control()}
	{#if readonly}
		<!-- Nothing to write into, so the value is drawn as one. An empty one says
		     what is missing, in the colour of something that is not there. -->
		<span
			class="min-w-0 flex-1 break-words whitespace-pre-wrap {value === ''
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
			class="min-w-0 flex-1 resize-none bg-transparent text-small leading-relaxed text-txt2 outline-none placeholder:text-txt4 focus:text-txt"
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
			class="min-w-0 flex-1 truncate bg-transparent outline-none placeholder:text-txt4 {classes} {mono
				? 'font-mono'
				: ''}"
		/>
	{/if}
{/snippet}

{#if bare}
	<span
		class="flex min-w-0 flex-1 items-center rounded-sm border border-transparent px-2 py-1 transition focus-within:border-accent focus-within:bg-surface2 focus-within:ring-4 focus-within:ring-accent/15 hover:border-hairline"
	>
		{@render control()}
	</span>
{:else}
	<Field>
		{@render control()}
	</Field>
{/if}
