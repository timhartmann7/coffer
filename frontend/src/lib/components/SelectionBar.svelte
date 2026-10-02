<script lang="ts">
	import { tick } from 'svelte';
	import { eraseQuestion } from '$lib/bin';
	import { counted } from '$lib/format';
	import { cancels, composing } from '$lib/lines';
	import type { EntryRow, Group } from '$lib/model';
	import { KEYS, OWN } from '$lib/shortcuts';
	import Confirm from './Confirm.svelte';
	import FolderPicker from './FolderPicker.svelte';
	import Icon from './Icon.svelte';

	/**
	 * What can be done to the entries chosen in the list, standing where the
	 * list's column names stand, in their type, so nothing under it moves.
	 *
	 * Outside the bin: move them to a folder, put a tag on them, delete them.
	 * In it, where nothing is edited: put them back, or delete them for good.
	 * Deleting asks first when they would go out of the file, in the window's
	 * one question box under the bar; going to the bin it does not ask, and the
	 * notice offers it back. The rows of one list all go the same way, so the
	 * question is about every one of them.
	 *
	 * Whatever is open - the folder list, the tag being typed, the question -
	 * is about these rows, and the window draws the bar afresh when the choice
	 * changes, which closes it.
	 */
	let {
		rows,
		root,
		binned,
		onMove,
		onTag,
		onDelete,
		onPutBack,
		onClear
	}: {
		/** The rows chosen, in the order the list draws them. */
		rows: EntryRow[];
		/** The tree, which the folder list is drawn from. */
		root: Group;
		/** Whether the list is the recycle bin or a folder inside it. */
		binned: boolean;
		onMove: (into: string) => void;
		/** Puts a tag on every one of them, given as it was typed. */
		onTag: (tag: string) => void;
		/** Deletes them: asked first here when any would go for good. */
		onDelete: () => void;
		onPutBack: () => void;
		/** Lets go of the choice, the way Escape does. */
		onClear: () => void;
	} = $props();

	let step = $state<'moving' | 'tagging' | 'asking' | null>(null);
	let mover = $state<HTMLButtonElement>();
	let tagger = $state<HTMLButtonElement>();
	let field = $state<HTMLInputElement>();

	/** The rows a deletion would take out of the file rather than to the bin. */
	const forever = $derived(rows.filter((row) => row.deletion === 'forever'));
	/** The one folder every row is in, which the folder list marks and leaves
	 * nothing to do in; none when they are in several. */
	const current = $derived(
		rows.length > 0 && rows.every((row) => row.group === rows[0].group) ? rows[0].group : null
	);

	$effect(() => {
		if (step === 'tagging') field?.focus();
	});

	/** Gives the focus back to the button whose step just closed, once it is
	 * drawn again. */
	async function back(to: () => HTMLButtonElement | undefined) {
		await tick();
		to()?.focus();
	}

	function remove() {
		if (forever.length > 0) step = 'asking';
		else onDelete();
	}

	/** Puts what was typed on every row, once: Return is the one way a tag
	 * goes on here. */
	function finish() {
		if (step !== 'tagging' || !field) return;
		const named = field.value.trim();
		field.value = '';
		step = null;
		if (named !== '') onTag(named);
	}

	/**
	 * The focus left the tag field, which lets the tag go the way Escape does.
	 *
	 * Unlike an entry's own tag chip, leaving does not put it on. Here a tag
	 * is one version on every row chosen - three hundred of them after Cmd+A -
	 * and whatever took the focus was not a Return: a press of the bar's own
	 * Delete, which would send the tag and the deletion side by side, or a
	 * click somewhere else. The window going behind another one, or a lock
	 * taking it down, is not leaving at all: nothing is sent, and a window
	 * that comes back gives the focus back to the field with what was typed
	 * still in it.
	 */
	function left() {
		if (!document.hasFocus()) return;
		step = null;
	}

	/**
	 * The tag field's keys. Escape lets the tag go and goes no further: the
	 * window's own Escape would let go of the choice the tag was meant for.
	 */
	function keys(event: KeyboardEvent) {
		if (cancels(event)) {
			event.preventDefault();
			event.stopPropagation();
			step = null;
			void back(() => tagger);
			return;
		}
		if (event.key === 'Enter' && !composing(event)) {
			event.preventDefault();
			finish();
			void back(() => tagger);
		}
	}
</script>

{#snippet dot()}
	<span aria-hidden="true" class="text-txt4">·</span>
{/snippet}

<div
	role="group"
	aria-label="The selected entries"
	class="relative flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-line px-5 py-2 font-mono text-label tracking-label uppercase"
>
	<span class="text-txt2">{rows.length} selected</span>
	{#if binned}
		{@render dot()}
		<button
			type="button"
			onclick={onPutBack}
			class="text-accent uppercase transition-colors hover:text-accenthi"
		>
			Put back
		</button>
		{@render dot()}
		<button
			type="button"
			onclick={remove}
			class="text-txt3 uppercase transition-colors hover:text-danger"
		>
			Delete forever…
		</button>
	{:else}
		{@render dot()}
		<!-- A press while the list is open keeps the focus in its filter, so
		     the list does not close on the press only for the click to open it
		     again. -->
		<button
			bind:this={mover}
			type="button"
			aria-haspopup="listbox"
			aria-expanded={step === 'moving'}
			title="Move to another folder"
			onmousedown={(event) => {
				if (step === 'moving') event.preventDefault();
			}}
			onclick={() => (step = step === 'moving' ? null : 'moving')}
			class="text-txt3 uppercase transition-colors hover:text-txt"
		>
			Move to…
		</button>
		{@render dot()}
		{#if step === 'tagging'}
			<input
				bind:this={field}
				type="text"
				autocomplete="off"
				spellcheck="false"
				aria-label="A tag for the selected entries"
				placeholder="Tag"
				onblur={left}
				onkeydown={keys}
				class="-my-1 w-24 animate-rise rounded-full border border-accent bg-surface2 px-2.5 py-1 font-mono text-label tracking-label text-txt uppercase ring-4 ring-accent/15 outline-none placeholder:text-txt4"
			/>
		{:else}
			<button
				bind:this={tagger}
				type="button"
				onclick={() => (step = 'tagging')}
				class="text-txt3 uppercase transition-colors hover:text-txt"
			>
				Add tag
			</button>
		{/if}
		{@render dot()}
		<button
			type="button"
			onclick={remove}
			title={forever.length > 0 ? undefined : `Move to the Recycle Bin · ${KEYS.moveToBin}`}
			class="text-txt3 uppercase transition-colors hover:text-danger"
		>
			{forever.length > 0 ? 'Delete forever…' : 'Delete'}
		</button>
	{/if}
	<button
		type="button"
		onclick={onClear}
		aria-label="Deselect all"
		title="Deselect all · {OWN.away}"
		class="ml-auto text-txt4 transition-colors hover:text-txt2"
	>
		<Icon name="x" class="h-3.5 w-3.5" />
	</button>

	{#if step === 'moving'}
		<FolderPicker
			{root}
			heading="Move to"
			label="Find the folder to move the selected entries to"
			{current}
			chosen={current ?? root.id}
			class="top-full left-5 w-[292px]"
			onPick={(into) => {
				step = null;
				onMove(into);
				void back(() => mover);
			}}
			onClose={(escaped) => {
				step = null;
				if (escaped) void back(() => mover);
			}}
		/>
	{/if}
</div>

{#if step === 'asking'}
	<Confirm
		class="mx-5 my-2 shrink-0 bg-surface2"
		question={eraseQuestion(counted(forever))}
		keep={rows.length === 1 ? 'Keep it' : 'Keep them'}
		act="Delete forever"
		onKeep={() => (step = null)}
		onAct={() => {
			step = null;
			onDelete();
		}}
	/>
{/if}
