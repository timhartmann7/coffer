<script lang="ts">
	import { NO_LOGIN, UNTITLED } from '$lib/format';
	import { untouchable, type EntryRow, type Group } from '$lib/model';
	import Bare from './Bare.svelte';
	import Field from './Field.svelte';
	import FolderLine from './FolderLine.svelte';
	import Heading from './Heading.svelte';
	import InBin from './InBin.svelte';
	import Mask from './Mask.svelte';

	/**
	 * An entry chosen from the list that Rust has not read yet.
	 *
	 * Rust answers one command at a time, and a save holds it for a whole key
	 * derivation and the encryption of the vault, about a second. An entry
	 * chosen in that second used to wait for it with the one before still on the
	 * screen. The row that was pressed already says what the entry is called and
	 * who it logs in as, so the pane says that at once, and says that the rest is
	 * on its way.
	 *
	 * Nothing here can be changed, copied or revealed, because none of it has
	 * been read. The heading, the box the name sits in, the card of an entry in
	 * the bin and the login's box are the ones the entry is drawn with, in the
	 * same words, so nothing moves or changes when it arrives.
	 */
	let {
		row,
		root,
		path,
		now,
		readOnly,
		onClose
	}: {
		/** The row that was pressed. */
		row: EntryRow;
		/** The tree, which names the folders the bin's card speaks of. */
		root: Group;
		/** The folders from the vault down to the entry. */
		path: Group[];
		now: Date;
		/** Whether the entry will be drawn with no way to change it, and so
		 * with no buttons on its card in the bin. */
		readOnly: boolean;
		/** Puts the pane away, and the entry's answer with it. */
		onClose: () => void;
	} = $props();
</script>

<!-- The line above the title is drawn as the entry will draw it once read,
     with nothing to press yet: an entry that can be moved says where it is. -->
{#snippet where()}
	<FolderLine {path} {root} current={row.group} />
{/snippet}

<section class="flex h-full flex-col overflow-hidden bg-surface" aria-busy="true">
	<Heading
		{path}
		place={untouchable(row, readOnly) ? undefined : where}
		masked={row.title === null}
		duplicable={!untouchable(row, readOnly)}
		{onClose}
	>
		<Bare>
			<span class="min-w-0 flex-1 truncate {row.title === '' ? 'text-txt4' : 'text-txt'}">
				{row.title || UNTITLED}
			</span>
		</Bare>
	</Heading>

	<div class="flex-1 overflow-y-auto px-6 py-5">
		<!-- The row knows the entry is in the bin, so the card that will stand
		     above the login stands there already, and the login does not drop by
		     its height when the entry arrives. -->
		{#if row.binned}
			<InBin class="mb-5" binned={row.binned} {root} {now} {readOnly} />
		{/if}

		<div class="block">
			<span class="font-mono text-label tracking-label text-txt3 uppercase">Login</span>
			<span class="mt-1.5 flex items-center gap-2">
				<Field>
					{#if row.username === null}
						<Mask />
					{:else}
						<span
							class="min-w-0 flex-1 truncate font-mono text-body {row.username === ''
								? 'text-txt4'
								: 'text-txt'}"
						>
							{row.username || NO_LOGIN}
						</span>
					{/if}
				</Field>
			</span>
		</div>

		<!-- Faded in rather than drawn at once: an entry Rust answers for in the
		     same frame replaces this before it has been seen, and only one that
		     waits on a save lets it arrive. -->
		<p class="mt-5 animate-fade text-fine text-txt4">Opening…</p>
	</div>
</section>
