<script lang="ts">
	import {
		asFailure,
		chooseDatabase,
		chooseKeyFile,
		chooseRescue,
		chooseSnapshot,
		discardRescue,
		forgetKeyFile,
		snapshots,
		unlock,
		unlockTakingOver
	} from '$lib/ipc';
	import { fully } from '$lib/format';
	import type { Database, Failure, Rescued, Snapshot } from '$lib/model';
	import Icon from './Icon.svelte';
	import Mark from './Mark.svelte';

	let {
		database,
		keyFile = null,
		reason = null,
		rescue = null,
		lost = false,
		onChoose,
		onKeyFile,
		onCreate,
		onUnlocked
	}: {
		database: Database | null;
		/**
		 * The key file this vault needs beside the password, when one has been
		 * chosen. Rust's answer rather than this window's: a lock destroys the
		 * window and the vault it is about is still the same one.
		 */
		keyFile?: Database | null;
		/**
		 * Why the vault that was open is not open any more. Rust's word, not the
		 * window's: the window that knew was destroyed, which is what locking
		 * means here.
		 */
		reason?: string | null;
		/**
		 * The copy a lock left beside this vault, when it could not save what
		 * was in the window. Rust's answer rather than this window's, for the
		 * same reason `reason` is: the window that knew was destroyed, and a
		 * copy an earlier run left is still worth offering.
		 */
		rescue?: Rescued | null;
		/** Whether the last lock had work to write and nowhere to put it. */
		lost?: boolean;
		onChoose: (database: Database) => void;
		onKeyFile: (chosen: Database | null) => void;
		/** Offered on the first run, where there is nothing to open yet. */
		onCreate: () => void;
		onUnlocked: () => Promise<void>;
	} = $props();

	/** What happened, in a sentence. Every one of them ends the same way,
	 * because what matters is that the data is out of memory rather than which
	 * of three things put it there. */
	const said: Record<string, string> = {
		idle: 'Nothing happened here for a while.',
		sleeping: 'This Mac went to sleep.',
		screenLocked: 'The screen locked.',
		sessionSwitched: 'Somebody else signed in on this Mac.'
	};
	const why = $derived(reason ? (said[reason] ?? 'The vault was locked.') : null);

	let field = $state<HTMLInputElement>();
	let busy = $state(false);
	let failure = $state<Failure | null>(null);
	let snapshot = $state<Snapshot | null>(null);

	/**
	 * The one failure the field itself is about. Everything else - a file that
	 * will not open, a database somebody else has open, a file that is gone -
	 * is about the vault, and painting the password field red for those sends
	 * the reader back to a password that was right all along.
	 */
	const wrongPassword = $derived(failure?.code === 'wrongCredentials');

	/** A file that will not open at all. The offer to open a snapshot belongs to
	 * this and to nothing else. */
	const unreadable = $derived(
		failure !== null && (failure.code === 'damaged' || failure.code === 'notADatabase')
	);

	/** Somebody's lock file sits beside the vault. Coffer respects it, and the
	 * only thing that can tell a live one from one a power cut left behind is a
	 * person reading the machine and the hour out of the message. */
	const held = $derived(failure?.code === 'heldByAnother');

	/**
	 * Whether the next attempt takes that lock over.
	 *
	 * Set by a press, never by Coffer, and it costs the reader the password
	 * again: the first one is already derived and gone, and keeping it here to
	 * retry with would be the one thing this screen exists not to do.
	 */
	let takingOver = $state(false);

	/** Whether the reader has just taken the copy away. Rust is asked for the
	 * status once per window, so the offer has to be taken off the screen here
	 * rather than by reading it again. */
	let dropped = $state(false);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (busy || !database || !field) return;

		// The password is read out of the field and turned into bytes here. It
		// is never bound to a variable the framework keeps, and the field is
		// emptied before the call that could take a second comes back.
		const bytes = new TextEncoder().encode(field.value);
		field.value = '';

		const over = takingOver;
		busy = true;
		failure = null;
		snapshot = null;

		try {
			await (over ? unlockTakingOver(bytes) : unlock(bytes));
			takingOver = false;
			await onUnlocked();
		} catch (thrown) {
			failure = asFailure(thrown);
			if (unreadable) {
				snapshot = (await snapshots().catch(() => []))[0] ?? null;
			}
			field.focus();
		} finally {
			busy = false;
		}
	}

	/**
	 * Opens the copy a lock left, which is a vault of its own at another path.
	 *
	 * `sameVault` because it is the same database under the same credentials: a
	 * copy taken from a vault that wants a key file needs the same one, and
	 * forgetting it would leave it impossible to open.
	 */
	async function openRescue() {
		if (busy) return;
		try {
			chose(await chooseRescue(), true);
		} catch (thrown) {
			failure = asFailure(thrown);
		}
	}

	/** Takes the copy away, once the reader says they are done with it. */
	async function dropRescue() {
		if (busy) return;
		try {
			await discardRescue();
			dropped = true;
		} catch (thrown) {
			failure = asFailure(thrown);
		}
	}

	/** Asks for the key file, and says so upwards so that the screen and Rust
	 * hold one answer between them. */
	async function pickKeyFile() {
		if (busy) return;
		try {
			const picked = await chooseKeyFile();
			if (picked) {
				onKeyFile(picked);
				failure = null;
				field?.focus();
			}
		} catch (thrown) {
			failure = asFailure(thrown);
		}
	}

	async function dropKeyFile() {
		if (busy) return;
		await forgetKeyFile().catch(() => {});
		onKeyFile(null);
		field?.focus();
	}

	async function choose() {
		// Not while a key is deriving: the unlock in flight would finish over
		// whatever is picked here, and Rust would then refuse it as stale.
		if (busy) return;

		const picked = await chooseDatabase().catch((thrown) => {
			failure = asFailure(thrown);
			return null;
		});
		if (picked) chose(picked);
	}

	async function openSnapshot(index: number) {
		try {
			chose(await chooseSnapshot(index), true);
		} catch (thrown) {
			failure = asFailure(thrown);
		}
	}

	/**
	 * Points the screen at another file.
	 *
	 * `sameVault` is true for a snapshot, which is a copy of the vault that was
	 * already chosen and opens with the same credentials. Rust keeps the key file
	 * for one of those and forgets it for anything else, and this has to say the
	 * same thing: a screen that showed no key file while Rust was still using one
	 * would be a screen nobody could reason about.
	 *
	 * A lock somebody said to take over is this file's lock and nothing else's,
	 * so that goes either way.
	 */
	function chose(picked: Database, sameVault = false) {
		onChoose(picked);
		failure = null;
		snapshot = null;
		takingOver = false;
		if (!sameVault) onKeyFile(null);
		field?.focus();
	}

	$effect(() => {
		field?.focus();
	});
</script>

<div class="flex flex-1 items-center justify-center overflow-y-auto px-10 py-10">
	<div class="w-full max-w-[320px] animate-rise">
		<Mark class="mx-auto h-11 w-11 text-txt" />
		{#if why}
			<h1 class="mt-6 text-center text-title font-medium tracking-tight text-txt">Locked</h1>
			<p class="mx-auto mt-3 max-w-[38ch] text-center text-body leading-relaxed text-txt2">
				{why} What was in the vault has been wiped out of memory - the password opens it again.
			</p>
		{:else}
			<div class="mt-5 text-center text-lead font-bold tracking-wordmark">COFFER</div>
		{/if}

		<!-- Standing news about a file beside the vault, so it sits above the
		     password rather than inside the panel that reports a failure to open
		     this one. It is as true on a first launch as it is straight after
		     the lock that wrote it. -->
		{#if lost}
			<p class="mx-auto mt-6 max-w-[38ch] text-center text-small leading-relaxed text-danger">
				There were changes the vault had not taken, and Coffer could not write them anywhere.
			</p>
		{/if}

		{#if rescue && !dropped}
			<div class="mt-6 rounded-sm border border-hairline bg-surface2 px-4 py-4">
				<div class="flex items-center gap-2">
					<Icon name="warn" class="h-4 w-4 shrink-0 text-warn" />
					<span class="text-body text-txt">Work that never reached the vault</span>
				</div>
				<p class="mt-2 text-fine leading-relaxed text-txt2">
					A lock could not save it, so Coffer put it in
					<span class="font-mono text-txt">{rescue.name}</span>
					{#if rescue.written}{fully(rescue.written, new Date())}{/if}. It opens with the same
					password. It is a file of its own: copy what you need back into your vault, then remove
					it.
				</p>
				<div class="mt-4 flex gap-2">
					<button
						type="button"
						onclick={openRescue}
						disabled={busy}
						class="h-9 rounded-full border border-hairline px-4 text-small text-txt transition-colors hover:border-txt3 active:bg-surface2 disabled:cursor-not-allowed"
					>
						Open it
					</button>
					<button
						type="button"
						onclick={dropRescue}
						disabled={busy}
						class="h-9 rounded-full px-4 text-small text-txt3 transition-colors hover:text-txt disabled:cursor-not-allowed"
					>
						Remove it
					</button>
				</div>
			</div>
		{/if}

		{#if database}
			<button
				type="button"
				onclick={choose}
				disabled={busy}
				class="mt-8 flex w-full items-center gap-3 rounded-sm border border-hairline bg-surface2 px-3 py-2.5 text-left transition-colors hover:border-txt4 disabled:cursor-not-allowed disabled:hover:border-hairline"
			>
				<Icon name="disk" class="h-4 w-4 shrink-0 text-txt4" />
				<span class="min-w-0 flex-1">
					<span class="block truncate text-body text-txt">{database.name}</span>
					<span class="block truncate font-mono text-meta text-txt4">{database.path}</span>
				</span>
				<Icon name="chev-d" class="h-4 w-4 shrink-0 text-txt4" />
			</button>

			<form onsubmit={submit}>
				<label class="mt-4 block">
					<span class="mb-2 block font-mono text-label tracking-label text-txt3 uppercase">
						Vault password
					</span>
					<span
						class="flex items-center gap-2 rounded-sm border bg-surface2 px-3 py-3 {wrongPassword
							? 'border-danger/60'
							: 'border-hairline focus-within:border-accent focus-within:ring-4 focus-within:ring-accent/15'} transition"
					>
						<Icon name="lock" class="h-4 w-4 shrink-0 text-txt3" />
						<input
							bind:this={field}
							type="password"
							autocomplete="off"
							spellcheck="false"
							class="min-w-0 flex-1 bg-transparent tracking-mask text-txt outline-none"
						/>
					</span>
				</label>

				{#if wrongPassword && failure}
					<p class="mt-3 text-small text-danger">{failure.message}</p>
				{/if}

				<!-- Coffer never makes a vault that wants a key file, so this is
				     folded away until it is asked for. A vault whose owner chose
				     one in another client has no other way in. -->
				{#if keyFile}
					<div
						class="mt-3 flex items-center gap-2 rounded-sm border border-hairline bg-surface2 px-3 py-2"
					>
						<Icon name="key" class="h-4 w-4 shrink-0 text-txt4" />
						<span class="min-w-0 flex-1 truncate font-mono text-meta text-txt2">
							{keyFile.name}
						</span>
						<button
							type="button"
							onclick={dropKeyFile}
							disabled={busy}
							aria-label="Do not use a key file"
							class="shrink-0 text-txt4 transition-colors hover:text-txt2 disabled:cursor-not-allowed"
						>
							<Icon name="x" class="h-4 w-4" />
						</button>
					</div>
				{:else}
					<button
						type="button"
						onclick={pickKeyFile}
						disabled={busy}
						class="mt-3 text-fine text-txt3 transition-colors hover:text-txt2 disabled:cursor-not-allowed"
					>
						This vault also needs a key file
					</button>
				{/if}

				<button
					type="submit"
					disabled={busy}
					class="mt-5 h-[46px] w-full rounded-full px-6 text-base font-medium transition-colors {busy
						? 'cursor-not-allowed bg-surface2 text-txt4'
						: 'bg-accent text-canvas hover:bg-accenthi active:bg-accenthi'}"
				>
					{#if busy}
						Unlocking…
					{:else if takingOver}
						Open anyway
					{:else}
						Unlock
					{/if}
				</button>
			</form>

			{#if failure && !wrongPassword}
				<div class="mt-6 border-t border-line pt-5 text-center">
					<p class="flex items-center justify-center gap-2 text-lead text-txt">
						<Icon name="warn" class="h-5 w-5 shrink-0 text-warn" />
						{failure.message}
					</p>
					{#if snapshot}
						<p class="mx-auto mt-3 max-w-[38ch] text-small leading-relaxed text-txt2">
							{#if snapshot.taken}
								A snapshot from {fully(snapshot.taken, new Date())} sits beside it.
							{:else}
								A snapshot Coffer took before one of its own saves sits beside it.
							{/if}
							It opens with the same password.
						</p>
						{@const only = snapshot}
						<button
							type="button"
							onclick={() => openSnapshot(only.index)}
							class="mt-5 h-9 rounded-full border border-hairline px-5 text-small text-txt transition-colors hover:border-txt3 active:bg-surface2"
						>
							Open {only.name}
						</button>
					{/if}

					<!-- The lock file says who and when, and the reader is the only
					     one who can tell a Coffer running right now from a Mac that
					     lost power with the vault open. Without this the second is a
					     vault nothing here could ever open again. -->
					{#if held}
						{#if takingOver}
							<p class="mx-auto mt-3 max-w-[38ch] text-small leading-relaxed text-txt2">
								Type the password again and Coffer will open the vault and take the lock. If the
								other Coffer is really running, whichever saves last wins.
							</p>
						{:else}
							<p class="mx-auto mt-3 max-w-[38ch] text-small leading-relaxed text-txt2">
								If that Mac lost power or Coffer was force-quit, the lock is left over and nothing
								is holding the vault.
							</p>
							<button
								type="button"
								onclick={() => {
									takingOver = true;
									field?.focus();
								}}
								class="mt-5 h-9 rounded-full border border-hairline px-5 text-small text-txt transition-colors hover:border-txt3 active:bg-surface2"
							>
								Open it anyway
							</button>
						{/if}
					{/if}
				</div>
			{/if}

			<!-- The same two ways out, wherever the screen got to. A vault that
			     will not open is the moment somebody most wants to make another
			     one, and that is where the offer used to disappear. -->
			<div class="mt-6 flex justify-center gap-6 text-fine text-txt3">
				<button
					type="button"
					onclick={choose}
					class="transition-colors hover:text-txt2 active:text-txt3"
				>
					Open another database
				</button>
				<button
					type="button"
					onclick={onCreate}
					class="transition-colors hover:text-txt2 active:text-txt3"
				>
					Create new
				</button>
			</div>
		{:else}
			<h1
				class="mx-auto mt-7 max-w-[22ch] text-center text-display leading-snug font-medium tracking-tight text-txt"
			>
				Your passwords will live here
			</h1>
			<p class="mx-auto mt-4 max-w-[46ch] text-center text-base leading-relaxed text-txt2">
				All of it goes into one file on this Mac. One password opens it, and that is the one you
				choose next.
			</p>
			<button
				type="button"
				onclick={onCreate}
				class="mx-auto mt-9 block h-[46px] w-full max-w-[300px] rounded-full bg-accent px-6 text-base font-medium text-canvas transition-colors hover:bg-accenthi"
			>
				Make a vault
			</button>
			<div class="mt-4 text-center">
				<button
					type="button"
					onclick={choose}
					class="text-small text-txt2 underline decoration-hairline underline-offset-4 transition-colors hover:text-txt"
				>
					I already have a vault file
				</button>
				<p class="mt-2 text-fine text-txt4">
					A .kdbx file from another app, or from another computer, will do
				</p>
			</div>
			{#if failure}
				<p class="mt-4 text-center text-small text-danger">{failure.message}</p>
			{/if}
		{/if}
	</div>
</div>
