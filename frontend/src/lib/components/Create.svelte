<script lang="ts">
	import {
		asFailure,
		calibrate,
		chooseNewDatabase,
		createDatabase,
		defaultNewDatabase
	} from '$lib/ipc';
	import type { Calibration, Database } from '$lib/model';
	import Icon from './Icon.svelte';

	/**
	 * Making a vault, which is the one screen where a mistake cannot be undone.
	 *
	 * The warning about the password is in plain words and at full size, not in
	 * small type and not behind a checkbox: a forgotten password is the end of
	 * the data and there is no way back from it.
	 */
	let { onMade, onCancel }: { onMade: () => Promise<void>; onCancel: () => void } = $props();

	let where = $state<Database | null>(null);
	let first = $state<HTMLInputElement>();
	let second = $state<HTMLInputElement>();
	let measured = $state<Calibration | null>(null);
	let busy = $state(false);
	let failure = $state<string | null>(null);
	let mismatch = $state(false);

	/**
	 * The measurement runs as soon as the screen opens, so that it is over by
	 * the time anybody has typed a password twice. There is no progress to
	 * report from Rust - the bar is three steps of this screen's own state - and
	 * the number is only true at the end.
	 */
	$effect(() => {
		void (async () => {
			try {
				where = await defaultNewDatabase();
				measured = await calibrate();
			} catch (thrown) {
				failure = asFailure(thrown).message;
			}
		})();
	});

	// The password is the only thing this screen asks anybody to type, so that is
	// where the cursor starts. The measurement runs beside it and needs nobody.
	$effect(() => {
		first?.focus();
	});

	/**
	 * Return in the first field would submit with the second one still empty, and
	 * `make` empties both before it compares them: one stray press and a typed
	 * password is gone under a message saying the two did not match. Return in
	 * the second field still submits, so typing it twice and pressing return
	 * stays one action.
	 */
	function onward(event: KeyboardEvent) {
		if (event.key !== 'Enter') return;
		event.preventDefault();
		second?.focus();
	}

	async function choose() {
		failure = null;
		try {
			const picked = await chooseNewDatabase();
			if (picked) where = picked;
		} catch (thrown) {
			failure = asFailure(thrown).message;
		}
	}

	async function make(event: SubmitEvent) {
		event.preventDefault();
		if (busy || !where || !first || !second) return;

		// Compared as bytes rather than as strings. Two different passwords can
		// look the same after a lossy conversion, and a vault opened by neither
		// of them is not something anybody would find out until later.
		const wanted = new TextEncoder().encode(first.value);
		const again = new TextEncoder().encode(second.value);
		first.value = '';
		second.value = '';

		mismatch = wanted.length !== again.length || wanted.some((byte, at) => byte !== again[at]);
		again.fill(0);

		if (mismatch || wanted.length === 0) {
			if (wanted.length === 0) failure = 'A vault needs a master password.';
			wanted.fill(0);
			first.focus();
			return;
		}

		busy = true;
		failure = null;
		try {
			await createDatabase(wanted);
			await onMade();
		} catch (thrown) {
			failure = asFailure(thrown).message;
			first.focus();
		} finally {
			wanted.fill(0);
			busy = false;
		}
	}
</script>

<div class="flex flex-1 items-center justify-center overflow-y-auto px-10 py-10">
	<form onsubmit={make} class="w-full max-w-[620px] animate-rise">
		<div class="grid gap-5">
			<div>
				<span class="mb-2 block font-mono text-label tracking-label text-txt3 uppercase">
					Where it goes
				</span>
				<div
					class="flex items-center gap-2 rounded-sm border border-hairline bg-surface2 px-3 py-2.5"
				>
					<Icon name="folder" class="h-4 w-4 shrink-0 text-txt4" />
					<span class="truncate font-mono text-small {where ? 'text-txt' : 'text-txt4'}">
						{where ? where.path : 'Working out where…'}
					</span>
					<button
						type="button"
						onclick={choose}
						disabled={busy}
						class="ml-auto shrink-0 text-fine text-txt3 transition-colors hover:text-txt disabled:cursor-not-allowed"
					>
						Somewhere else
					</button>
				</div>
			</div>
		</div>

		<div class="mt-5 grid grid-cols-2 gap-5">
			<label class="block">
				<span class="mb-2 block font-mono text-label tracking-label text-txt3 uppercase">
					Vault password
				</span>
				<span
					class="flex items-center gap-2 rounded-sm border bg-surface2 px-3 py-2.5 {mismatch
						? 'border-danger/60'
						: 'border-hairline focus-within:border-accent focus-within:ring-4 focus-within:ring-accent/15'} transition"
				>
					<input
						bind:this={first}
						type="password"
						autocomplete="new-password"
						spellcheck="false"
						oninput={() => (mismatch = false)}
						onkeydown={onward}
						class="min-w-0 flex-1 bg-transparent tracking-mask text-txt outline-none"
					/>
				</span>
			</label>
			<label class="block">
				<span class="mb-2 block font-mono text-label tracking-label text-txt3 uppercase">
					Once more
				</span>
				<span
					class="flex items-center gap-2 rounded-sm border bg-surface2 px-3 py-2.5 {mismatch
						? 'border-danger/60'
						: 'border-hairline focus-within:border-accent focus-within:ring-4 focus-within:ring-accent/15'} transition"
				>
					<input
						bind:this={second}
						type="password"
						autocomplete="new-password"
						spellcheck="false"
						oninput={() => (mismatch = false)}
						class="min-w-0 flex-1 bg-transparent tracking-mask text-txt outline-none"
					/>
				</span>
			</label>
		</div>

		{#if mismatch}
			<p class="mt-3 animate-rise text-small text-danger">Those two are not the same.</p>
		{/if}

		<div class="mt-6 rounded-sm border border-warn/35 bg-warnwash p-5">
			<div class="flex gap-3">
				<Icon name="warn" class="mt-0.5 h-5 w-5 shrink-0 text-warn" />
				<div>
					<p class="text-lead font-medium text-txt">The password cannot be recovered.</p>
					<p class="mt-2 max-w-[52ch] text-body leading-relaxed text-txt2">
						It is not kept anywhere and it is not sent anywhere. If it is forgotten the vault
						becomes a file of random bytes, and nobody can get the data back - not us, not anybody.
					</p>
				</div>
			</div>
		</div>

		{#if !measured}
			<div class="mt-6 border-t border-line pt-5">
				<span class="font-mono text-label tracking-label text-txt3 uppercase">
					Fitting the lock to this Mac
				</span>
				<div class="mt-3 h-[2px] w-full overflow-hidden rounded-full bg-line">
					<!-- Two steps of this screen's own state, as literal classes. A width
					     computed into a class compiles to nothing, and the window may not
					     set one from script. -->
					<div class="h-full rounded-full bg-accent {where ? 'w-2/3' : 'w-1/3'}"></div>
				</div>
				<p class="mt-3 text-fine text-txt3">
					A second or so, once. It is what makes opening this vault take about a second here rather
					than on somebody else's machine.
				</p>
			</div>
		{/if}

		{#if failure}
			<p class="mt-5 text-small text-danger">{failure}</p>
		{/if}

		<div class="mt-8 flex items-center justify-end gap-3">
			<button
				type="button"
				onclick={onCancel}
				disabled={busy}
				class="h-[46px] rounded-full px-5 text-base text-txt3 transition-colors hover:text-txt2"
			>
				Never mind
			</button>
			<button
				type="submit"
				disabled={busy || !where || !measured}
				class="h-[46px] rounded-full px-7 text-base font-medium transition-colors {busy ||
				!where ||
				!measured
					? 'cursor-not-allowed bg-surface2 text-txt4'
					: 'bg-accent text-canvas hover:bg-accenthi active:bg-accenthi'}"
			>
				{busy ? 'Making it…' : 'Make the vault'}
			</button>
		</div>
	</form>
</div>
