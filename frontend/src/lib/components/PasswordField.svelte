<script lang="ts">
	import { tick } from 'svelte';
	import { Revealed } from '$lib/reveal.svelte';
	import Field from './Field.svelte';
	import Generator from './Generator.svelte';
	import Icon from './Icon.svelte';
	import Mask from './Mask.svelte';

	/**
	 * The password row: a mask, and the three things that can be done to it.
	 *
	 * The value is never in this component. Showing it writes it into the input
	 * that displays it and nowhere else, the timer wipes that input again, and
	 * what is in the input when the focus leaves is what gets written back.
	 * Writing the same value again costs nothing: the engine drops a version
	 * that changed nothing and puts the modification time back.
	 *
	 * Nothing here changes size when an eye opens. The mask and the field that
	 * replaces it share one box, the three buttons have a line of their own
	 * whether or not anything is revealed, and the countdown is the field's own
	 * bottom edge rather than a row that arrives under it. What the reader used
	 * to get instead was the whole entry moving down the screen on every press,
	 * and back up again half a minute later without being asked.
	 */
	let {
		entry,
		field,
		empty,
		protect,
		readOnly,
		onCopy,
		onCommit,
		onFailure
	}: {
		entry: string;
		field: string;
		empty: boolean;
		/** Whether the database keeps this value protected. It goes back with the
		 * edit so the file does not quietly lose the protection. */
		protect: boolean;
		/** A database Coffer will not write back: the value can still be shown
		 * and copied, and nothing here offers to change it. */
		readOnly: boolean;
		onCopy: (field: string) => void;
		onCommit: (field: string, value: string, protect: boolean) => void;
		onFailure: (thrown: unknown) => void;
	} = $props();

	const revealed = new Revealed();
	let node = $state<HTMLInputElement>();
	let generating = $state(false);
	/** Set on an entry that has no password: there is nothing to reveal, and the
	 * field is opened empty for one to be written into. */
	let writing = $state(false);

	/** Whether the field is live, whether that is because a value was revealed
	 * into it or because one is being written. */
	const live = $derived(revealed.showing || writing);

	// A different entry is a different secret. Whatever is on the screen goes
	// before the next one arrives.
	$effect(() => {
		void entry;
		return () => {
			revealed.hide();
			writing = false;
			generating = false;
		};
	});

	async function toggle() {
		if (live) {
			close();
			return;
		}
		if (!node) return;

		// An entry with no password has nothing to reveal, and asking for one
		// would be asking for a field the file may not even carry.
		if (empty) {
			if (readOnly) return;
			writing = true;
			node.value = '';
			await tick();
			node?.focus();
			return;
		}

		try {
			await revealed.show(node, entry, field);
			// The field is hidden until the value is on the screen, so it can
			// only be focused after the screen has caught up with that.
			await tick();
			node?.focus();
		} catch (thrown) {
			onFailure(thrown);
		}
	}

	/** Takes whatever is in the field off the screen. */
	function close() {
		revealed.hide();
		if (writing && node) node.value = '';
		writing = false;
	}

	/**
	 * The first keystroke turns a value being read into a value being written.
	 *
	 * The half minute is how long a password Coffer put on the screen stays
	 * there. What the reader is typing is not that, and a timer that wiped the
	 * field mid-word would take away their new password and leave them looking
	 * at nothing.
	 */
	function written() {
		if (!revealed.showing) return;
		revealed.release();
		writing = true;
	}

	/**
	 * Writes what is in the field.
	 *
	 * Only while the value is still on the screen: the timer empties the input
	 * on its own, and a blur after that would write an empty password over a
	 * real one.
	 */
	function commit() {
		if (!node || !live || readOnly) return;
		const written = node.value;
		close();
		onCommit(field, written, protect);
	}

	function keys(event: KeyboardEvent) {
		if (event.key === 'Escape') close();
		if (event.key === 'Enter') {
			event.preventDefault();
			commit();
		}
	}

	function insert(made: string) {
		close();
		onCommit(field, made, protect);
	}
</script>

<div class="mt-5">
	<span class="font-mono text-label tracking-label text-txt3 uppercase">Password</span>

	<div class="mt-1.5 flex items-center">
		<Field>
			<!-- A value can be anything a file can hold: a megabyte of text, or
			     eight combining accents with no letter under them. It is clipped to
			     its own box so that whatever it is stays inside the field. -->
			{#if !live}
				{#if empty}
					<span class="min-w-0 flex-1 truncate text-body text-txt4">
						No password on this entry
					</span>
				{:else}
					<span class="min-w-0 flex-1 overflow-hidden"><Mask /></span>
				{/if}
			{/if}
			<!-- The value lives here and nowhere else. -->
			<input
				bind:this={node}
				data-value
				type="text"
				autocomplete="off"
				spellcheck="false"
				aria-label="Password"
				hidden={!live}
				readonly={readOnly}
				oninput={written}
				onblur={commit}
				onkeydown={keys}
				class="min-w-0 flex-1 bg-transparent font-mono text-body text-txt outline-none"
			/>

			{#if revealed.showing}
				<!-- Four characters, because the value beside them is what the pane is
				     for and this is three hundred and eighty-four pixels wide. The
				     sentence the mockup writes out is still here for anything that
				     reads the screen aloud, where there is no such shortage. -->
				<span class="shrink-0 animate-fade font-mono text-label text-accent">
					<span class="sr-only">Hides in </span>0:{String(revealed.left).padStart(2, '0')}
				</span>
				<!-- The bar the mockup gives a countdown, drawn as the field's own
				     bottom edge. Anywhere else it is a row that arrives under the
				     field and pushes the rest of the entry down the screen. -->
				<span class="absolute inset-x-0 bottom-0 h-[2px] bg-line" aria-hidden="true">
					<span class="block h-full w-full origin-left animate-drain-reveal bg-accent"></span>
				</span>
			{/if}
		</Field>
	</div>

	<!-- Their own line, whether or not anything is revealed. Sharing one with
	     the value put them on a second line in a pane this narrow anyway, and
	     which line they landed on then depended on how long the value was. -->
	<div class="mt-2 flex flex-wrap items-center gap-2">
		<button
			type="button"
			onclick={toggle}
			class="flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-hairline px-3 text-fine text-txt2 transition hover:border-txt3 hover:text-txt active:bg-raised"
		>
			<Icon name={live ? 'eye-off' : 'eye'} class="h-3.5 w-3.5" />
			<span>{live ? 'Hide' : empty && !readOnly ? 'Set one' : 'Show'}</span>
		</button>
		<button
			type="button"
			onclick={() => onCopy(field)}
			disabled={empty}
			class="flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-hairline px-3 text-fine text-txt2 transition hover:border-txt3 hover:text-txt active:bg-raised disabled:cursor-not-allowed disabled:text-txt4 disabled:hover:border-hairline"
		>
			<Icon name="copy" class="h-3.5 w-3.5" /> Copy
		</button>
		{#if !readOnly}
			<button
				type="button"
				onclick={() => (generating = !generating)}
				aria-expanded={generating}
				class="flex h-7 shrink-0 items-center gap-1.5 rounded-full border px-3 text-fine transition active:bg-raised {generating
					? 'border-txt4 text-txt'
					: 'border-hairline text-txt2 hover:border-txt3 hover:text-txt'}"
			>
				<Icon name="refresh" class="h-3.5 w-3.5" /> Make one
			</button>
		{/if}
	</div>

	{#if generating}
		<div class="mt-3 animate-rise">
			<Generator onInsert={insert} onClose={() => (generating = false)} {onFailure} />
		</div>
	{/if}
</div>
