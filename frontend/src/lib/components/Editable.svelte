<script lang="ts">
	import { drop, typed as said, unfinished, type Place } from '$lib/drafts';
	import { composing, finishes, lines } from '$lib/lines';
	import Field from './Field.svelte';
	import Unsaved from './Unsaved.svelte';

	/**
	 * A value the reader changes where it stands.
	 *
	 * There is no edit mode and no save button, which is what the mockup asks
	 * for: the value is a field, and what the reader wrote in it is written when
	 * the focus leaves. Escape puts back what was there, and Return finishes a
	 * value on one line.
	 *
	 * Nothing is written unless the reader wrote something. Clicking through an
	 * entry does not stamp a modification time on it, and a field only clicked
	 * into and out of again writes nothing whatever the field made of the value
	 * on its way in. That is not a nicety: a one-line input drops the line
	 * breaks of what it is given, and a text area reads a CRLF or a lone CR back
	 * as a line feed, and either of them compared against the vault's value
	 * looked like an edit. Ten recovery codes clicked on to select one were
	 * written back to the file as one line.
	 *
	 * A value with a line break in it is written in a text area that grows with
	 * it, where Return starts a line and Cmd+Return or leaving finishes. A field
	 * that may be given lines - one of the reader's own - is a text area from the
	 * start, one line tall until it has more, so that a paste keeps its breaks
	 * and Option+Return adds one. After a real edit the text area's reading is
	 * what is written, line feeds included, which is also what the reader was
	 * looking at.
	 *
	 * Until the field is left, what is typed is told to Rust as it goes
	 * (`drafts.ts`), so that a lock arriving first - a lid closed mid-sentence -
	 * writes it rather than wiping it, and a mark beside the field says it is
	 * not saved yet.
	 */
	let {
		value,
		label,
		placeholder = '',
		multiline = false,
		breaks = false,
		mono = false,
		classes = 'text-body text-txt',
		readonly = false,
		bare = false,
		draft,
		onCommit
	}: {
		value: string;
		/** What a screen reader calls this field. */
		label: string;
		placeholder?: string;
		/** Written in lines whatever it holds: the entry's notes, and a field the
		 * reader asked to write in lines. Four lines tall at least. */
		multiline?: boolean;
		/** May be given lines: a field of the reader's own. */
		breaks?: boolean;
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
		/** Where what is typed would be written: the entry, the field, and whether
		 * the database protects it. What is typed is told to Rust before the field
		 * is left; `onCommit` is what writes it. */
		draft: Place;
		/** Answers whether the value was taken. A value that was refused is put
		 * back, so that the screen never shows something the vault does not have.
		 */
		onCommit: (value: string) => Promise<boolean>;
	} = $props();

	let node = $state<HTMLInputElement | HTMLTextAreaElement>();
	/** Whether the reader has written in the field since it last showed what the
	 * vault holds. Nothing is drawn from it. */
	let edited = false;
	/** How many lines the field holds: the vault's value, and the reader's as
	 * they write it. */
	let written = $derived(lines(value));

	/** Decided by the vault's value rather than by what is being typed, so the
	 * field is never swapped for another under the reader's fingers. */
	const area = $derived(multiline || breaks || lines(value) > 1);
	/** Whether Return starts a line rather than finishing the value. */
	const lined = $derived(multiline || written > 1);

	function typed() {
		if (!node) return;
		edited = true;
		written = lines(node.value);
		// The element rather than `node`, which a field that has gone no longer
		// has: a draft read from nothing would be an empty one, and a lock would
		// write it over what the field held.
		const element = node;
		said(draft, () => element.value);
	}

	async function commit() {
		if (!node || !edited) return;
		edited = false;
		if (node.value === value) {
			// Not going to be written, so Rust is told to let go of it too.
			drop(draft);
			return;
		}
		const taken = await onCommit(node.value);
		// A refusal leaves the vault as it was, so the field goes back to what
		// the vault has rather than standing there showing something else.
		if (!taken && node) {
			node.value = value;
			written = lines(value);
		}
	}

	function keys(event: KeyboardEvent) {
		if (!node) return;
		if (event.key === 'Escape') {
			node.value = value;
			edited = false;
			written = lines(value);
			drop(draft);
			node.blur();
			return;
		}
		if (area ? finishes(event, lined) : event.key === 'Enter' && !composing(event)) {
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
	{:else if area}
		<!-- One line and no wrapping until there is a second, so that a value of
		     the reader's own on one line looks and scrolls the way the fields
		     beside it do. -->
		<textarea
			bind:this={node}
			{value}
			{placeholder}
			aria-label={label}
			rows={multiline ? Math.max(4, written) : written}
			wrap={lined ? 'soft' : 'off'}
			autocomplete="off"
			spellcheck="false"
			oninput={typed}
			onblur={commit}
			onkeydown={keys}
			class="min-w-0 flex-1 resize-none bg-transparent outline-none placeholder:text-txt4 {classes} {mono
				? 'font-mono'
				: ''} {lined ? 'overflow-x-hidden overflow-y-auto' : 'overflow-hidden'}"></textarea>
		{@render mark('mt-2 self-start')}
	{:else}
		<input
			bind:this={node}
			{value}
			{placeholder}
			aria-label={label}
			type="text"
			autocomplete="off"
			spellcheck="false"
			oninput={typed}
			onblur={commit}
			onkeydown={keys}
			class="min-w-0 flex-1 truncate bg-transparent outline-none placeholder:text-txt4 {classes} {mono
				? 'font-mono'
				: ''}"
		/>
		{@render mark('')}
	{/if}
{/snippet}

<!-- Beside the first line of a value in lines, and in the middle of one that is
     a line. -->
{#snippet mark(at: string)}
	{#if unfinished(draft)}
		<Unsaved class={at} />
	{/if}
{/snippet}

{#if bare}
	<span
		class="flex min-w-0 flex-1 items-center gap-2 rounded-sm border border-transparent px-2 py-1 transition focus-within:border-accent focus-within:bg-surface2 focus-within:ring-4 focus-within:ring-accent/15 hover:border-hairline"
	>
		{@render control()}
	</span>
{:else}
	<Field>
		{@render control()}
	</Field>
{/if}
