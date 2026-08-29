<script lang="ts">
	import { asFailure, chooseDatabase, chooseSnapshot, snapshots, unlock } from '$lib/ipc';
	import { fully } from '$lib/format';
	import type { Database, Failure, Snapshot } from '$lib/model';
	import Icon from './Icon.svelte';

	let {
		database,
		onChoose,
		onUnlocked
	}: {
		database: Database | null;
		onChoose: (database: Database) => void;
		onUnlocked: () => Promise<void>;
	} = $props();

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
		<img src="/coffer-logo-light.svg" alt="" class="mx-auto h-11 w-11" />
		<div class="mt-5 text-center text-lead font-bold tracking-wordmark">COFFER</div>

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
						: 'bg-accent text-canvas hover:bg-accenthi'}"
				>
					{busy ? 'Unlocking…' : 'Unlock'}
				</button>
			</form>

			{#if failure && !wrongPassword}
				<div class="mt-6 flex gap-4 border-t border-line pt-5">
					<Icon name="warn" class="mt-0.5 h-5 w-5 shrink-0 text-warn" />
					<div>
						<p class="text-lead text-txt">{failure.message}</p>
						{#if snapshot}
							<p class="mt-2 text-small leading-relaxed text-txt2">
								{#if snapshot.taken}
									A snapshot from {fully(snapshot.taken, new Date())} sits beside it.
								{:else}
									A snapshot Coffer took before one of its own saves sits beside it.
								{/if}
								It opens with the same password.
							</p>
						{/if}
						<div class="mt-5 flex flex-wrap gap-2">
							{#if snapshot}
								{@const only = snapshot}
								<button
									type="button"
									onclick={() => openSnapshot(only.index)}
									class="h-9 rounded-full border border-hairline px-5 text-small text-txt transition-colors hover:border-txt3"
								>
									Open {only.name}
								</button>
							{/if}
							<button
								type="button"
								onclick={choose}
								class="h-9 rounded-full px-4 text-small text-txt3 transition-colors hover:text-txt2"
							>
								Choose another file
							</button>
						</div>
					</div>
				</div>
			{:else}
				<div class="mt-6 flex justify-center gap-6 text-fine text-txt3">
					<button type="button" onclick={choose} class="transition-colors hover:text-txt2">
						Open another database
					</button>
				</div>
			{/if}
		{:else}
			<p class="mt-8 text-center text-body leading-relaxed text-txt2">
				Coffer opens one file and keeps everything in it. Choose the vault you already have.
			</p>
			<button
				type="button"
				onclick={choose}
				class="mt-8 h-[46px] w-full rounded-full bg-accent px-6 text-base font-medium text-canvas transition-colors hover:bg-accenthi"
			>
				Choose a vault file
			</button>
			<p class="mt-4 text-center text-fine leading-relaxed text-txt4">
				A .kdbx file from KeePassXC, KeePass or another computer will do
			</p>
			{#if failure}
				<p class="mt-4 text-center text-small text-danger">{failure.message}</p>
			{/if}
		{/if}
	</div>
</div>
