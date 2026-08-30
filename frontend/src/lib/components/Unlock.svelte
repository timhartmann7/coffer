<script lang="ts">
	import { asFailure, chooseDatabase, chooseSnapshot, snapshots, unlock } from '$lib/ipc';
	import { fully } from '$lib/format';
	import type { Database, Failure, Snapshot } from '$lib/model';
	import Icon from './Icon.svelte';
	import Mark from './Mark.svelte';

	let {
		database,
		reason = null,
		onChoose,
		onCreate,
		onUnlocked
	}: {
		database: Database | null;
		/**
		 * Why the vault that was open is not open any more. Rust's word, not the
		 * window's: the window that knew was destroyed, which is what locking
		 * means here.
		 */
		reason?: string | null;
		onChoose: (database: Database) => void;
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

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (busy || !database || !field) return;

		// The password is read out of the field and turned into bytes here. It
		// is never bound to a variable the framework keeps, and the field is
		// emptied before the call that could take a second comes back.
		const bytes = new TextEncoder().encode(field.value);
		field.value = '';

		busy = true;
		failure = null;
		snapshot = null;

		try {
			await unlock(bytes);
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
			chose(await chooseSnapshot(index));
		} catch (thrown) {
			failure = asFailure(thrown);
		}
	}

	function chose(picked: Database) {
		onChoose(picked);
		failure = null;
		snapshot = null;
		field?.focus();
	}

	$effect(() => {
		field?.focus();
	});
</script>

<div class="flex flex-1 items-center justify-center overflow-y-auto px-10 py-10">
	<div class="w-full max-w-[320px]">
		<Mark class="mx-auto h-11 w-11 text-txt" />
		{#if why}
			<h1 class="mt-6 text-center text-title font-medium tracking-tight text-txt">Locked</h1>
			<p class="mx-auto mt-3 max-w-[38ch] text-center text-body leading-relaxed text-txt2">
				{why} What was in the vault has been wiped out of memory - the password opens it again.
			</p>
		{:else}
			<div class="mt-5 text-center text-lead font-bold tracking-wordmark">COFFER</div>
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
							: 'border-hairline focus-within:border-accent focus-within:ring-4 focus-within:ring-accent/15'}"
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

				<button
					type="submit"
					disabled={busy}
					class="mt-5 h-[46px] w-full rounded-full px-6 text-base font-medium transition-colors {busy
						? 'cursor-not-allowed bg-surface2 text-txt4'
						: 'bg-accent text-canvas hover:bg-accenthi active:bg-accenthi'}"
				>
					{busy ? 'Unlocking…' : 'Unlock'}
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
