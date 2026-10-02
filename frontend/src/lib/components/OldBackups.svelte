<script lang="ts">
	import { asFailure, removeOldSnapshots } from '$lib/ipc';
	import Confirm from './Confirm.svelte';

	/**
	 * The question a new master password leaves behind: whether to remove the
	 * automatic backups that still open with the old one.
	 *
	 * They were written under the old password and go on opening with it,
	 * which matters most to a reader changing a password because they believe
	 * somebody has it. Rust removes exactly the files it counted when the
	 * password changed, wherever later saves have moved them; this says how
	 * many and asks, in the window's one question box.
	 */
	let {
		count = $bindable(),
		onDone,
		onRemoved
	}: {
		/** How many still open with the old password. A removal that could not
		 * take them all leaves it at how many are left, and the question stays
		 * about those. */
		count: number;
		/** The question is answered: kept, with nothing to say, or removed, with
		 * a sentence saying how many went. */
		onDone: (said: string | null) => void;
		/** Rust has answered a removal, which may have taken some of them or
		 * all: whatever lists the backups reads them again. */
		onRemoved?: () => void;
	} = $props();

	let removing = $state(false);
	let failure = $state<string | null>(null);

	const question = $derived(
		count === 1
			? 'Master password changed. Your 1 automatic backup of an earlier save still opens with the old password until a later save replaces it. Removing it can’t be undone.'
			: `Master password changed. Your ${count} automatic backups of earlier saves still open with the old password until later saves replace them. Removing them can’t be undone.`
	);

	/** Why the question is still up, in one sentence for both ways a removal
	 * stops short: some went and the rest would not, or none went at all - which
	 * is every one of them when the call never reached the disk. */
	function unremoved(some: boolean, why: string) {
		failure = some
			? `Not every old backup could be removed: ${why}.`
			: `The old backups could not be removed: ${why}.`;
	}

	/** Removes them, once however often it is pressed. One that will not go
	 * keeps the question up, about the ones that are left, and says why. */
	async function remove() {
		if (removing) return;
		removing = true;
		failure = null;
		try {
			const { gone, left, refused } = await removeOldSnapshots();
			onRemoved?.();
			if (refused) {
				count = left;
				unremoved(gone > 0, refused.message);
				return;
			}
			onDone(
				gone === 0
					? 'No old backups were left to remove.'
					: gone === 1
						? 'Removed the old backup.'
						: `Removed ${gone} old backups.`
			);
		} catch (thrown) {
			unremoved(false, asFailure(thrown).message);
		} finally {
			removing = false;
		}
	}
</script>

<Confirm
	class="mt-4 bg-surface2"
	{question}
	keep={count === 1 ? 'Keep it' : 'Keep them'}
	act={count === 1 ? 'Remove the old backup' : 'Remove old backups'}
	onKeep={() => onDone(null)}
	onAct={remove}
/>
<!-- Always there while the question is, so that what arrives in it is read
     out: the focus stays on the answers, and nothing else would say it. -->
<div role="status">
	{#if failure}
		<p class="mt-3 text-small text-danger">{failure}</p>
	{/if}
</div>
