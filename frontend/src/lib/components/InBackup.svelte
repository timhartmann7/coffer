<script lang="ts">
	import { at } from '$lib/format';
	import type { SnapshotOf } from '$lib/model';
	import { refusal } from '$lib/replacing';
	import { keepCopy } from '$lib/saving.svelte';
	import Strip from './Strip.svelte';

	/**
	 * What a vault opened from one of its backups says about itself, across the
	 * top of the window for as long as it is open, and the three ways out.
	 *
	 * Nothing in a backup can be changed - the next save of the vault would
	 * push the change out of the chain - so the strip says which backup this is,
	 * to the minute, and why it is open: the vault's own file would not open or
	 * is not there, or the reader asked to look.
	 *
	 * "Use this copy as my vault" is the accent, because it is what a backup
	 * opened in place of a vault that would not open is there to decide. It
	 * goes over the vault's file, so the second line says which file, when that
	 * file last changed, what becomes of it - the newest backup when it opens
	 * with this backup's password, a file of its own beside the vault when it
	 * does not - and that nothing is deleted. A backup opens with the password
	 * the vault had when it was taken, which after a change is not the one it
	 * has now, so the line says the vault opens with this one from then on: a
	 * reader who changed it because it leaked must hear that before the press.
	 *
	 * Rust holds the press to the file this strip described, as it holds the
	 * copy's. A file that changed since is refused, the window reads it again,
	 * and the strip says why nothing happened above a sentence that is true now.
	 */
	let {
		snapshot,
		onAdopt,
		onBack
	}: {
		snapshot: SnapshotOf;
		/** Makes the backup the vault. A refusal is thrown back here to be said
		 * where the press was, once `snapshot` says how the vault's file stands
		 * now. */
		onAdopt: () => Promise<void>;
		/** Goes back to the vault, which locks the backup on the way. */
		onBack: () => Promise<void>;
	} = $props();

	let busy = $state<'adopting' | 'leaving' | 'saving' | null>(null);
	let failure = $state<string | null>(null);
	/** Where a copy of the backup went, once one was kept. */
	let kept = $state<string | null>(null);

	const now = new Date();
	/** Which backup this is, with when the vault was as it holds it when the
	 * filesystem kept that, and why it is open. */
	const heading = $derived.by(() => {
		const taken = at(snapshot.taken, now);
		const which = taken
			? `You’re looking at a backup: your vault as it was ${taken}.`
			: 'You’re looking at a backup Coffer took before one of its saves.';
		if (!snapshot.vaultFile.there) return `${which} Your vault file is not there any more.`;
		if (snapshot.because === 'unopened') return `${which} Your vault file could not be opened.`;
		return `${which} Nothing in it can be changed.`;
	});
	const changed = $derived(at(snapshot.vaultFile.written, now));

	/** One press at a time: the first two end what this strip is about, and a
	 * copy is written from the vault the others would close. */
	async function run(which: 'adopting' | 'leaving' | 'saving', action: () => Promise<void>) {
		if (busy) return;
		busy = which;
		failure = null;
		try {
			await action();
		} catch (thrown) {
			failure = refusal(thrown);
		} finally {
			busy = null;
		}
	}

	/** Writes the backup to a file of its own, and says where. A panel closed
	 * without a choice says nothing. */
	async function copy() {
		const said = await keepCopy();
		if (said) kept = said;
	}
</script>

<Strip>
	<p class="text-small text-txt">{heading}</p>
	<p class="mt-1 text-fine leading-relaxed text-txt2">
		{#if !snapshot.vaultFile.there}
			Using it as your vault puts it back as <bdi class="font-mono">{snapshot.vault}</bdi>.
		{:else}
			Using it as your vault puts it in place of <bdi class="font-mono">{snapshot.vault}</bdi
			>{#if changed}, last changed {changed}{/if}. If that file opens with this backup’s password,
			it is kept as <bdi class="font-mono">{snapshot.keptAs}</bdi> until later saves push it out of the
			backups; if not, it is kept beside it under a name of its own. Nothing is deleted.
		{/if}
		From then on, your vault opens with the password this backup opens with.
	</p>
	{#if kept}
		<p class="mt-1 text-fine text-txt2">{kept}</p>
	{/if}
	{#if failure}
		<p class="mt-1 line-clamp-3 text-fine leading-relaxed break-words text-danger">{failure}</p>
	{/if}
	{#snippet actions()}
		<button
			type="button"
			onclick={() => run('leaving', onBack)}
			disabled={busy !== null}
			class="h-9 rounded-full border border-hairline px-4 text-small text-txt transition-colors hover:border-txt3 active:bg-surface2 disabled:cursor-not-allowed"
		>
			Back to my vault
		</button>
		<button
			type="button"
			onclick={() => run('saving', copy)}
			disabled={busy !== null}
			class="h-9 rounded-full border border-hairline px-4 text-small text-txt transition-colors hover:border-txt3 active:bg-surface2 disabled:cursor-not-allowed"
		>
			Save a copy as…
		</button>
		<button
			type="button"
			onclick={() => run('adopting', onAdopt)}
			disabled={busy !== null}
			class="h-9 rounded-full px-4 text-small font-medium transition-colors {busy === 'adopting'
				? 'cursor-not-allowed bg-surface2 text-txt4'
				: 'bg-accent text-canvas hover:bg-accenthi active:bg-accenthi disabled:cursor-not-allowed'}"
		>
			{busy === 'adopting' ? 'Making it your vault…' : 'Use this copy as my vault'}
		</button>
	{/snippet}
</Strip>
