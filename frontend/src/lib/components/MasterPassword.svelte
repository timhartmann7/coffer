<script lang="ts">
	import { tick } from 'svelte';
	import { hold } from '$lib/holding';
	import { asFailure } from '$lib/ipc';
	import { cancels } from '$lib/lines';
	import { EMPTY, MISMATCH, same } from '$lib/master';
	import type { Rekey } from '$lib/saving.svelte';
	import MasterInput from './MasterInput.svelte';
	import OldBackups from './OldBackups.svelte';
	import Unrecoverable from './Unrecoverable.svelte';

	/**
	 * The settings' row for the master password: a form under it that gives the
	 * vault a new one.
	 *
	 * Three fields - the password the vault has, the new one, and the new one
	 * again - over the creation screen's warning, because a new password is as
	 * unrecoverable as the first. Whether the two new ones agree is asked here,
	 * as bytes; whether the first is the vault's is Rust's question, asked of
	 * the key the vault holds. What is typed is read once, at the press, and
	 * wiped as soon as it has gone. A change that left automatic backups under
	 * the old password hands the row to the question about them.
	 */
	let {
		rekey,
		copyBeside = false,
		onBackups
	}: {
		/** Gives the vault the new password, through the vault screen's own
		 * writing of the file, and answers with how many snapshots still open
		 * with the old one. */
		rekey: Rekey;
		/** The backups beside the vault changed - the new password's write
		 * took one, a removal took the old ones - and whatever lists them reads
		 * them again. */
		onBackups?: () => void;
		/** Whether the copy a lock left sits beside the vault. It opens with
		 * the password the vault has now and would bring it back if it were
		 * made the vault after a change, so the row says what to do with it
		 * first instead of offering one. */
		copyBeside?: boolean;
	} = $props();

	const id = $props.id();

	let open = $state(false);
	/** Whether Rust is changing the password. */
	let busy = $state(false);
	let mismatch = $state(false);
	/** Whether the two new passwords were the same when last compared. */
	let matched = $state(false);
	/** Whether the refusal is about the password typed as the current one. */
	let wrongCurrent = $state(false);
	let failure = $state<string | null>(null);
	/** How many snapshots still open with the old password, while the reader
	 * is asked about them. */
	let outdated = $state(0);
	/** What the change, or the removal after it, came to. */
	let said = $state<string | null>(null);

	let current = $state<HTMLInputElement>();
	let fresh = $state<HTMLInputElement>();
	let again = $state<HTMLInputElement>();
	let change = $state<HTMLButtonElement>();

	const encode = (text: string) => new TextEncoder().encode(text);

	$effect(() => {
		if (open) current?.focus();
	});

	// While Rust changes the password nothing takes the settings away: the
	// answer is the one place that says which password now opens the vault.
	$effect(() => {
		if (!busy) return;
		return hold({ holds: () => busy, ask: () => false });
	});

	function begin() {
		open = true;
		said = null;
		failure = null;
	}

	/** Puts the form away with everything typed in it, unless that is on its
	 * way, and gives the focus back to the button that opened it. */
	async function stop() {
		if (busy) return;
		open = false;
		mismatch = false;
		matched = false;
		wrongCurrent = false;
		failure = null;
		await tick();
		change?.focus();
	}

	/** Escape puts the form away and leaves the settings open: the window's own
	 * Escape closes them, and is told this one has been answered. */
	function escape(event: KeyboardEvent) {
		if (!cancels(event)) return;
		event.preventDefault();
		void stop();
	}

	/** Return in a field before the last moves on rather than sending a form
	 * that is not filled in yet. */
	function onward(event: KeyboardEvent, next: HTMLInputElement | undefined) {
		if (event.key !== 'Enter') return;
		event.preventDefault();
		next?.focus();
	}

	/**
	 * Whether the two new passwords agree, asked when the focus leaves either
	 * of them rather than on every key. Only agreement is drawn, as the check:
	 * a mismatch is said when the change is asked for, not while the second is
	 * still being typed.
	 */
	function compare() {
		if (!fresh?.value || !again?.value) {
			matched = false;
			return;
		}
		const one = encode(fresh.value);
		const other = encode(again.value);
		matched = same(one, other);
		one.fill(0);
		other.fill(0);
	}

	/** A key in either new field takes back what was said about the two. */
	function retyped() {
		mismatch = false;
		matched = false;
	}

	/** The row says what came of it, with the focus back on the way in. */
	async function settled(sentence: string | null) {
		outdated = 0;
		said = sentence;
		await tick();
		change?.focus();
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (busy || !current || !fresh || !again) return;
		// Whatever was said about the last attempt is about that attempt: left
		// beside what this one says, it would send the reader to the wrong field.
		failure = null;

		const wanted = encode(fresh.value);
		const twice = encode(again.value);
		mismatch = !same(wanted, twice);
		twice.fill(0);
		if (mismatch) {
			wanted.fill(0);
			again.value = '';
			matched = false;
			// Once the sentence is there, so that the field is read out with it.
			await tick();
			again.focus();
			return;
		}
		if (wanted.length === 0) {
			failure = EMPTY;
			fresh.focus();
			return;
		}

		const held = encode(current.value);
		busy = true;
		wrongCurrent = false;
		try {
			const left = await rekey(held, wanted);
			onBackups?.();
			current.value = '';
			fresh.value = '';
			again.value = '';
			open = false;
			matched = false;
			if (left > 0) outdated = left;
			else await settled('Master password changed.');
		} catch (thrown) {
			// Everything typed is kept: a refusal is a reason to fix one field,
			// not to type three again.
			const refused = asFailure(thrown);
			failure = `The password was not changed: ${refused.message}.`;
			if (refused.code === 'wrongCredentials') {
				wrongCurrent = true;
				current.focus();
				current.select();
			} else if (refused.code === 'refused') {
				fresh.focus();
			}
		} finally {
			busy = false;
			// Wiped once they are framed, on the way to Rust. Wiped here as well,
			// for the call that never got as far as framing them.
			held.fill(0);
			wanted.fill(0);
		}
	}
</script>

<div class="px-8 py-5">
	<div class="flex items-center justify-between gap-6">
		<div>
			<div class="text-row text-txt">Master password</div>
			<div class="mt-1 text-fine text-txt3">
				What you type to unlock this vault - nothing, if its key file alone opens it
			</div>
		</div>
		{#if !open && !copyBeside}
			<button
				bind:this={change}
				type="button"
				onclick={begin}
				class="h-9 shrink-0 rounded-full border border-hairline px-4 text-small text-txt2 transition-colors hover:border-txt4 hover:text-txt active:bg-raised"
			>
				Change…
			</button>
		{/if}
	</div>

	{#if copyBeside}
		<p class="mt-3 text-small leading-relaxed text-txt2">
			Not while the copy a lock left sits beside this vault, which opens with the same password.
			Lock the vault, then make the copy your vault or remove it, and the password can be changed
			here.
		</p>
	{:else if open}
		<!-- Escape is heard around the form rather than in each field, so that it
		     puts the form away wherever in it the focus is: a field, its eye,
		     or a button. -->
		<div role="presentation" class="mt-5 animate-rise" onkeydown={escape}>
			<form onsubmit={submit}>
				<div class="grid grid-cols-2 gap-5">
					<MasterInput
						bind:input={current}
						label="Current password"
						autocomplete="off"
						wrong={wrongCurrent}
						locked={busy}
						oninput={() => (wrongCurrent = false)}
						onkeydown={(event) => onward(event, fresh)}
					/>
				</div>
				<div class="mt-5 grid grid-cols-2 gap-5">
					<MasterInput
						bind:input={fresh}
						label="New password"
						autocomplete="new-password"
						locked={busy}
						oninput={retyped}
						onkeydown={(event) => onward(event, again)}
						onleave={compare}
					/>
					<MasterInput
						bind:input={again}
						label="Once more"
						autocomplete="new-password"
						describedby={mismatch ? `${id}-mismatch` : undefined}
						{matched}
						locked={busy}
						oninput={retyped}
						onleave={compare}
					/>
				</div>

				<!-- Each sentence below arrives in a region that was already there,
				     which is what gets it read out: one that comes into being with
				     its words in it may never be announced, and Return pressed in
				     "Once more" leaves the focus where it was. -->
				<div role="status">
					{#if mismatch}
						<p id="{id}-mismatch" class="mt-3 animate-rise text-small text-danger">{MISMATCH}</p>
					{/if}
				</div>

				<Unrecoverable class="mt-6" />

				<div role="status">
					{#if failure}
						<p class="mt-5 text-small text-danger">{failure}</p>
					{/if}
				</div>

				<div class="mt-6 flex items-center justify-end gap-3">
					<button
						type="button"
						onclick={stop}
						disabled={busy}
						class="h-9 rounded-full px-4 text-small text-txt3 transition-colors hover:text-txt2"
					>
						Never mind
					</button>
					<button
						type="submit"
						disabled={busy}
						class="h-9 rounded-full bg-accent px-4 text-small font-medium text-canvas transition-colors hover:bg-accenthi active:bg-accenthi disabled:cursor-not-allowed disabled:hover:bg-accent"
					>
						{busy ? 'Changing it…' : 'Change the password'}
					</button>
				</div>
			</form>
		</div>
	{:else if outdated > 0}
		<OldBackups bind:count={outdated} onDone={settled} onRemoved={onBackups} />
	{/if}

	<!-- What the change or the removal came to, said where the focus is not:
	     it goes back to "Change…", which says nothing of either. -->
	<div role="status">
		{#if said}
			<p class="mt-3 text-fine text-txt3">{said}</p>
		{/if}
	</div>
</div>
