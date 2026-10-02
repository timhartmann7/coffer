<script lang="ts">
	import { at } from '$lib/format';
	import type { CopyOf } from '$lib/model';
	import { refusal } from '$lib/replacing';
	import Strip from './Strip.svelte';

	/**
	 * What a vault opened from the copy a lock left says about itself, across
	 * the top of the window for as long as it is open, and the two ways out.
	 *
	 * Everything changed here changes the copy, and the strip says so (see
	 * `Strip.svelte`).
	 *
	 * "Make this my vault" is the accent, because it is what the copy was
	 * opened to decide. It goes over the vault's file, so the sentence beside
	 * it says what becomes of that file and when it last changed, which is how
	 * the reader tells whether somebody else wrote it after the copy was made.
	 * The file it names is a snapshot, and later saves push it out like any
	 * other, so the sentence says that too.
	 *
	 * Rust holds the press to the file this sentence described. A file that
	 * changed since is refused, the window reads it again, and the banner says
	 * why nothing happened above a sentence that is true now.
	 */
	let {
		copy,
		onPromote,
		onBack
	}: {
		copy: CopyOf;
		/** Makes the copy the vault. A refusal is thrown back here to be said
		 * where the press was, once `copy` says how the vault's file stands
		 * now. */
		onPromote: () => Promise<void>;
		/** Goes back to the vault, which locks the copy on the way. */
		onBack: () => Promise<void>;
	} = $props();

	let busy = $state<'promoting' | 'leaving' | null>(null);
	let failure = $state<string | null>(null);

	const now = new Date();
	/** What the copy is, with when it was written when the filesystem kept
	 * that. */
	const heading = $derived.by(() => {
		const saved = at(copy.saved, now);
		return saved ? `This is the copy saved ${saved}.` : 'This is the copy a lock saved.';
	});
	const changed = $derived(at(copy.vaultFile.written, now));

	/** One press at a time: either answer ends what this banner is about. */
	async function run(which: 'promoting' | 'leaving', action: () => Promise<void>) {
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
</script>

<Strip>
	<p class="text-small text-txt">
		{heading} What you change here changes the copy, not your vault.
	</p>
	<p class="mt-1 text-fine leading-relaxed text-txt2">
		{#if !copy.vaultFile.there}
			<bdi class="font-mono">{copy.vault}</bdi> is not there any more, so making this your vault puts
			the copy in its place.
		{:else}
			Making this your vault puts it in place of <bdi class="font-mono">{copy.vault}</bdi
			>{#if changed}, last changed {changed}{/if}, which is kept as
			<bdi class="font-mono">{copy.keptAs}</bdi> until later saves push it out of the snapshots.
		{/if}
	</p>
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
			onclick={() => run('promoting', onPromote)}
			disabled={busy !== null}
			class="h-9 rounded-full px-4 text-small font-medium transition-colors {busy === 'promoting'
				? 'cursor-not-allowed bg-surface2 text-txt4'
				: 'bg-accent text-canvas hover:bg-accenthi active:bg-accenthi disabled:cursor-not-allowed'}"
		>
			{busy === 'promoting' ? 'Making it your vault…' : 'Make this my vault'}
		</button>
	{/snippet}
</Strip>
