<script lang="ts">
	import { tick } from 'svelte';
	import { Secret } from '$lib/secret.svelte';
	import Field from './Field.svelte';
	import Generator from './Generator.svelte';
	import Icon from './Icon.svelte';
	import Mask from './Mask.svelte';

	/**
	 * The password row: a mask, and the three things that can be done to it.
	 *
	 * The value is never in this component. Showing it writes it into the input
	 * that displays it and nowhere else, the timer wipes that input again, and
	 * what is in the input when the focus leaves is written back only if the
	 * reader put it there. Showing a value is not an edit, and it must not be
	 * written back: a save is a second of key derivation, a rewrite of the whole
	 * file and one of the ten snapshots beside it, so a reader who pressed
	 * "Show" and then pressed "Copy" would have spent a recovery point on
	 * looking at something. The engine drops a version that changed nothing, but
	 * that is about the entry's history and not about the file.
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

	const secret = new Secret();
	let node = $state<HTMLInputElement>();
	let generating = $state(false);

	// A different entry is a different secret. Whatever is on the screen goes
	// before the next one arrives.
	$effect(() => {
		void entry;
		return () => {
			secret.close(node);
			generating = false;
		};
	});

	/**
	 * Shows the value, and puts it away again.
	 *
	 * The press that reaches this while a value is on the screen is kept from
	 * moving the focus (see the markup), so this runs with the field still live.
	 * Without that the blur got there first: it committed, which saved the file
	 * and derived the key again for a second, and it left `live` false - so the
	 * press this function is answering opened the value again with a fresh half
	 * minute. The button said Hide and did the opposite, slowly.
	 *
	 * Which means the two meanings of closing are decided here rather than by
	 * the order two events happened to arrive in: a value the reader was writing
	 * is written, and a value they were only reading is put away.
	 */
	async function toggle() {
		if (secret.live) {
			settle();
			return;
		}
		if (!node) return;

		// An entry with no password has nothing to reveal, and asking for one
		// would be asking for a field the file may not even carry.
		if (empty) {
			if (readOnly) return;
			secret.open(node);
			await tick();
			node?.focus();
			return;
		}

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

	/**
	 * What closing the field means, which depends on whose value is in it.
	 *
	 * A value the reader wrote is written back; a value Coffer put on the screen
	 * is put away. `Secret` is what tells the two apart, and it answers `null`
	 * for the second.
	 */
	function settle() {
		const written = secret.settle(node);
		if (written === null || readOnly) return;
		// Opening the field on an entry that has no password and typing nothing
		// is not an edit either. This is the one case where the component knows
		// the value it would be writing over, so it is the one it can refuse.
		if (empty && written === '') return;
		onCommit(field, written, protect);
	}

	function keys(event: KeyboardEvent) {
		if (event.key === 'Escape') secret.close(node);
		if (event.key === 'Enter') {
			event.preventDefault();
			settle();
		}
	}

	function insert(made: string) {
		secret.close(node);
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
			{#if !secret.live}
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
				hidden={!secret.live}
				readonly={readOnly}
				oninput={() => secret.written()}
				onblur={settle}
				onkeydown={keys}
				class="min-w-0 flex-1 bg-transparent font-mono text-body text-txt outline-none"
			/>

			{#if secret.showing}
				<!-- Four characters, because the value beside them is what the pane is
				     for and this is three hundred and eighty-four pixels wide. The
				     sentence the mockup writes out is still here for anything that
				     reads the screen aloud, where there is no such shortage. -->
				<span class="shrink-0 animate-fade font-mono text-label text-accent">
					<span class="sr-only">Hides in </span>0:{String(secret.left).padStart(2, '0')}
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
		<!-- While something is on the screen this press does not move the focus,
		     so it reaches `toggle` before the field's own blur does. The other way
		     round, Hide saved the vault and then opened the value again. -->
		<button
			type="button"
			onmousedown={(event) => secret.live && event.preventDefault()}
			onclick={toggle}
			class="flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-hairline px-3 text-fine text-txt2 transition hover:border-txt3 hover:text-txt active:bg-raised"
		>
			<Icon name={secret.live ? 'eye-off' : 'eye'} class="h-3.5 w-3.5" />
			<span>{secret.live ? 'Hide' : empty && !readOnly ? 'Set one' : 'Show'}</span>
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
