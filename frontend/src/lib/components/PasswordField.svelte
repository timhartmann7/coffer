<script lang="ts">
	import { typing } from '$lib/keys';
	import type { Span } from '$lib/model';
	import { Revealed } from '$lib/reveal.svelte';
	import Change from './Change.svelte';
	import Generator from './Generator.svelte';
	import Icon from './Icon.svelte';
	import Mask from './Mask.svelte';
	import Shown from './Shown.svelte';

	/**
	 * The password row: a mask, and the things that can be done to it.
	 *
	 * The value is never in this component. Showing it writes it into the text
	 * node that displays it and nowhere else, and the timer wipes that node
	 * again. Showing is reading and nothing else: the value is text, not a
	 * field, and no key reaches it. Changing the password is a press of its own,
	 * Change, which opens a field for the new one under the buttons and leaves
	 * the old one where it was (`Change.svelte`).
	 *
	 * Nothing here changes size when an eye opens. The mask and the value share
	 * one box, the buttons have a line of their own whether or not anything is
	 * revealed, and the countdown is the box's own bottom edge rather than a row
	 * that arrives under it (`Shown.svelte`). What the reader used to get
	 * instead was the whole entry moving down the screen on every press, and
	 * back up again half a minute later without being asked.
	 */
	let {
		entry,
		field,
		empty,
		lines,
		protect,
		readOnly,
		onCopy,
		onCommit,
		onFailure
	}: {
		entry: string;
		field: string;
		empty: boolean;
		/** Whether the password has a line break in it, so that a new one is
		 * written in lines too. */
		lines: boolean;
		/** Whether the database keeps this value protected. It goes back with the
		 * edit so the file does not quietly lose the protection. */
		protect: boolean;
		/** A database Coffer will not write back: the value can still be shown
		 * and copied, and nothing here offers to change it. */
		readOnly: boolean;
		/** Copies the password in Rust, whole or the part of it selected. */
		onCopy: (field: string, range: Span | null) => void;
		/** Writes a new password. Rejects with what the vault said when it would
		 * not take it. */
		onCommit: (field: string, value: string, protect: boolean) => Promise<void>;
		onFailure: (thrown: unknown) => void;
	} = $props();

	const revealed = new Revealed();
	let node = $state<HTMLElement>();
	let toggler = $state<HTMLButtonElement>();
	let opener = $state<HTMLButtonElement>();
	let editor = $state<ReturnType<typeof Change>>();
	let generating = $state(false);
	let changing = $state(false);

	/**
	 * Which entry the row is on.
	 *
	 * Its own value rather than a read of `entry`, which the pane hands on from
	 * an entry that is a new object after every change: an effect that followed
	 * it ran again for an edit to the title, and took a new password being
	 * typed away with it. A change that lands hides what is shown by itself
	 * (`conceal`).
	 */
	const showing = $derived(entry);

	// A different entry is a different secret. Whatever is on the screen goes
	// before the next one arrives, and so does a new password half typed for
	// the one that was open - which says so as it goes.
	$effect(() => {
		void showing;
		return () => {
			revealed.hide();
			generating = false;
			changing = false;
		};
	});

	/**
	 * Shows the value, and puts it away again.
	 *
	 * Once it is on the screen the focus goes to the button that hides it: not
	 * into anything that takes a key, so a stray one changes nothing, and onto
	 * this row, so Cmd+C copies it through Rust. Never out of the field a new
	 * password is being written in, though - the old one is being looked at
	 * while the new one is typed.
	 */
	async function toggle() {
		if (revealed.showing) {
			revealed.hide();
			return;
		}
		if (!node) return;
		try {
			await revealed.show(node, entry, field);
			if (!typing(document.activeElement)) toggler?.focus();
		} catch (thrown) {
			onFailure(thrown);
		}
	}

	/** Opens the field for a new password, or puts it away again: at once when
	 * nothing is typed in it, and after asking when something is. */
	function change() {
		if (changing) editor?.leave();
		else changing = true;
	}

	/** The field closed. The focus comes back here when it was in the field,
	 * rather than falling out of the pane, where the next Escape closes it. */
	function closed(back: boolean) {
		changing = false;
		if (back) opener?.focus();
	}

	/** A made password goes straight in: putting it there is the press that
	 * asked for it. A new one half typed by hand is put away, unwritten,
	 * because the reader chose the made one over it. */
	function insert(made: string) {
		editor?.close();
		onCommit(field, made, protect).catch(onFailure);
	}

	/** None of the buttons takes the focus from a new password being written,
	 * so pressing one is not leaving that field. */
	function kept(event: MouseEvent) {
		event.preventDefault();
	}
</script>

<div class="mt-5">
	<span class="font-mono text-label tracking-label text-txt3 uppercase">Password</span>

	<div class="mt-1.5 flex items-center">
		<Shown
			{revealed}
			bind:node
			classes="text-body"
			onCopy={(range) => onCopy(field, range)}
			{onFailure}
		>
			{#snippet hidden()}
				{#if empty}
					<span class="min-w-0 flex-1 truncate text-body text-txt4">
						No password on this entry
					</span>
				{:else}
					<span class="min-w-0 flex-1 overflow-hidden"><Mask /></span>
				{/if}
			{/snippet}
		</Shown>
	</div>

	<!-- Their own line, whether or not anything is revealed. Sharing one with
	     the value put them on a second line in a pane this narrow anyway, and
	     which line they landed on then depended on how long the value was. -->
	<div class="mt-2 flex flex-wrap items-center gap-2">
		{#if !empty}
			<button
				bind:this={toggler}
				type="button"
				onmousedown={kept}
				onclick={toggle}
				class="flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-hairline px-3 text-fine text-txt2 transition hover:border-txt3 hover:text-txt active:bg-raised"
			>
				<Icon name={revealed.showing ? 'eye-off' : 'eye'} class="h-3.5 w-3.5" />
				<span>{revealed.showing ? 'Hide' : 'Show'}</span>
			</button>
		{/if}
		<button
			type="button"
			onmousedown={kept}
			onclick={() => onCopy(field, null)}
			disabled={empty}
			class="flex h-7 shrink-0 items-center gap-1.5 rounded-full border border-hairline px-3 text-fine text-txt2 transition hover:border-txt3 hover:text-txt active:bg-raised disabled:cursor-not-allowed disabled:text-txt4 disabled:hover:border-hairline"
		>
			<Icon name="copy" class="h-3.5 w-3.5" /> Copy
		</button>
		{#if !readOnly}
			<button
				bind:this={opener}
				type="button"
				onmousedown={kept}
				onclick={change}
				aria-expanded={changing}
				class="flex h-7 shrink-0 items-center rounded-full border px-3 text-fine transition active:bg-raised {changing
					? 'border-txt4 text-txt'
					: 'border-hairline text-txt2 hover:border-txt3 hover:text-txt'}"
			>
				{empty ? 'Set one' : 'Change'}
			</button>
			<button
				type="button"
				onmousedown={kept}
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

	{#if changing}
		<Change
			bind:this={editor}
			class="mt-3 animate-rise"
			label="New password"
			placeholder="New password"
			what="password"
			multiline={lines}
			draft={{ entry, field, protect }}
			onSave={(value) => onCommit(field, value, protect)}
			onClose={closed}
			{onFailure}
		/>
	{/if}

	{#if generating}
		<div class="mt-3 animate-rise">
			<Generator onInsert={insert} onClose={() => (generating = false)} {onFailure} />
		</div>
	{/if}
</div>
