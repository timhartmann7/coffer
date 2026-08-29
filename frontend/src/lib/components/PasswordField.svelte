<script lang="ts">
	import { tick } from 'svelte';
	import { Revealed } from '$lib/reveal.svelte';
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

	<!-- The buttons keep their size and the value gets what is left, down to a
	     floor: below that the three of them go to a line of their own rather
	     than squeezing the value into a column one word wide. -->
	<div class="mt-1.5 flex flex-wrap items-start gap-x-3 gap-y-2">
		<!-- A value can be anything a file can hold: a megabyte of text, or
		     eight combining accents with no letter under them. It is clipped to
		     its own box so that whatever it is stays out of the label above and
		     out of the buttons beside it. -->
		<span class="min-w-[9rem] flex-1 overflow-hidden">
			{#if !live}
				{#if empty}
					<span class="block text-body text-txt4">No password on this entry</span>
				{:else}
					<Mask />
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
				class="block w-full rounded-sm border border-transparent bg-transparent font-mono text-body leading-snug text-txt transition-colors outline-none select-text focus:border-accent focus:bg-surface2 focus:ring-4 focus:ring-accent/15"
			/>
		</span>

		<span class="flex shrink-0 items-center gap-2">
			<button
				type="button"
				onclick={toggle}
				class="flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-hairline px-3 text-fine text-txt2 transition-colors hover:border-txt3 hover:text-txt"
			>
				<Icon name={live ? 'eye-off' : 'eye'} class="h-3.5 w-3.5" />
				<span>{live ? 'Hide' : empty && !readOnly ? 'Set one' : 'Show'}</span>
			</button>
			<button
				type="button"
				onclick={() => onCopy(field)}
				disabled={empty}
				class="flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-hairline px-3 text-fine text-txt2 transition-colors hover:border-txt3 hover:text-txt disabled:cursor-not-allowed disabled:text-txt4 disabled:hover:border-hairline"
			>
				<Icon name="copy" class="h-3.5 w-3.5" /> Copy
			</button>
			{#if !readOnly}
				<button
					type="button"
					onclick={() => (generating = !generating)}
					class="flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-hairline px-3 text-fine text-txt2 transition-colors hover:border-txt3 hover:text-txt"
				>
					<Icon name="refresh" class="h-3.5 w-3.5" /> Make one
				</button>
			{/if}
		</span>
	</div>

	{#if revealed.showing}
		<div
			class="mt-2 flex items-center gap-2 font-mono text-label tracking-label text-accent uppercase"
		>
			<span>Hides in 0:{String(revealed.left).padStart(2, '0')}</span>
			<span class="h-[2px] w-16 overflow-hidden rounded-full bg-line">
				<span class="block h-full w-full origin-left animate-drain-reveal rounded-full bg-accent"
				></span>
			</span>
		</div>
	{/if}

	{#if generating}
		<div class="mt-3">
			<Generator onInsert={insert} onClose={() => (generating = false)} {onFailure} />
		</div>
	{/if}
</div>
