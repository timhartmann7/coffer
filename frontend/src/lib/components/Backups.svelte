<script lang="ts">
	import { minute } from '$lib/format';
	import { asFailure } from '$lib/ipc';
	import type { Snapshot } from '$lib/model';

	/**
	 * The backups Coffer keeps beside a vault, newest first, wherever they are
	 * offered: on the screen for a vault file that will not open, and in the
	 * settings.
	 *
	 * Each row is when the vault was as the backup holds it, to the minute -
	 * ten saves an hour apart, or a minute apart, are told apart by nothing
	 * else - with the file name under it, the way the Finder shows it.
	 *
	 * A row is opened by the slot it was shown at, and Rust opens the file that
	 * slot held when this list was read, wherever saves have moved it since.
	 * One that has gone in the meantime is answered `gone`: the list says so
	 * and is read again, and nothing else is opened in its place. A list read
	 * again with nothing in it says that too, under the sentence about the one
	 * that went, so the screen it is on keeps drawing it.
	 */
	let {
		backups,
		act,
		open = null,
		onOpen,
		onAgain
	}: {
		backups: Snapshot[];
		/** What the button on each row says: what pressing it does here. */
		act: string;
		/** The file name of the backup open now, which is drawn as open rather
		 * than offered. */
		open?: string | null;
		/** Opens the backup shown at this slot. A refusal is thrown back here
		 * to be said under the list. */
		onOpen: (index: number) => Promise<void>;
		/** Reads the list again. */
		onAgain: () => Promise<void>;
	} = $props();

	let busy = $state(false);
	let failure = $state<string | null>(null);

	const now = new Date();

	/** One press at a time: either it opens a backup, or the list changes. */
	async function press(index: number) {
		if (busy) return;
		busy = true;
		failure = null;
		try {
			await onOpen(index);
		} catch (thrown) {
			const refused = asFailure(thrown);
			if (refused.code === 'gone') {
				failure = 'That backup is not there any more. The list is as it stands now.';
				await onAgain();
			} else {
				failure = refused.message;
			}
		} finally {
			busy = false;
		}
	}
</script>

{#if backups.length === 0}
	<p class="py-2.5 text-small text-txt3">No backups are left.</p>
{/if}
<ul class="divide-y divide-line">
	{#each backups as backup (backup.index)}
		{@const when = minute(backup.taken, now)}
		<li class="flex items-center gap-3 py-2.5">
			<div class="min-w-0 flex-1">
				{#if when}
					<span class="block text-small text-txt first-letter:uppercase">{when}</span>
				{:else}
					<span class="block text-small text-txt3">Time not kept</span>
				{/if}
				<bdi class="block truncate font-mono text-meta text-txt4">{backup.name}</bdi>
			</div>
			{#if backup.name === open}
				<span class="shrink-0 text-fine text-txt4">Open now</span>
			{:else}
				<button
					type="button"
					onclick={() => press(backup.index)}
					disabled={busy}
					aria-label="{act} {backup.name}"
					class="flex h-7 shrink-0 items-center rounded-full border border-hairline px-3 text-fine text-txt2 transition hover:border-txt3 hover:text-txt active:bg-raised disabled:cursor-not-allowed disabled:text-txt4 disabled:hover:border-hairline"
				>
					{act}
				</button>
			{/if}
		</li>
	{/each}
</ul>
<!-- Always there, so that what arrives in it is read out: the press that
     found a backup gone leaves the focus on a list that has changed. -->
<div role="status">
	{#if failure}
		<p class="mt-2 text-small text-danger">{failure}</p>
	{/if}
</div>
