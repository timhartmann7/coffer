<script lang="ts">
	import { standing } from '$lib/bin';
	import type { Binned, Group } from '$lib/model';
	import Confirm from './Confirm.svelte';
	import Icon from './Icon.svelte';

	/**
	 * What an entry or a folder opened in the recycle bin says about itself,
	 * and the two ways out of the bin.
	 *
	 * Putting it back is the call this card exists for, so it is the accent.
	 * Deleting it forever is the one deletion here nothing comes back from, so
	 * it is asked about where it was pressed, and the press that asks is plain
	 * until the pointer is on it: the only danger-coloured answer is the last
	 * one in the question.
	 *
	 * A database Coffer will not write back gets the sentence and no buttons.
	 * An entry still being read gets the buttons, so the card is the height it
	 * will be, and none of them can be pressed yet.
	 */
	let {
		binned,
		root,
		now,
		name,
		readOnly,
		class: classes = '',
		ways
	}: {
		binned: Binned;
		/** The tree, which is where the folder it came from gets its name. */
		root: Group;
		now: Date;
		/** A heading, for a folder: an entry's pane has its own title above. */
		name?: string;
		readOnly: boolean;
		class?: string;
		/** The two ways out, and what deleting it forever asks, naming what
		 * goes. Absent while the entry is still being read: there is nothing yet
		 * to put back or delete. */
		ways?: { question: string; onPutBack: () => void; onDelete: () => void };
	} = $props();

	let asking = $state(false);
</script>

<div data-binned class="rounded-sm border border-hairline bg-surface2 p-3 {classes}">
	<div class="flex items-start gap-2">
		<Icon name="trash" class="mt-0.5 h-4 w-4 shrink-0 text-txt4" />
		<div class="min-w-0 flex-1">
			{#if name !== undefined}
				<!-- A folder name is a value out of somebody's database and can be a
				     megabyte long, and so can the one the sentence names. -->
				<p class="truncate text-body text-txt">{name}</p>
			{/if}
			<p class="line-clamp-3 text-fine leading-relaxed break-words text-txt2">
				{standing(binned, root, now)}
			</p>
		</div>
	</div>

	{#if !readOnly}
		{#if asking && ways}
			{@const { question, onDelete } = ways}
			<Confirm
				bare
				class="mt-3 border-t border-hairline pt-3"
				{question}
				act="Delete forever"
				onKeep={() => (asking = false)}
				onAct={() => {
					asking = false;
					onDelete();
				}}
			/>
		{:else}
			<div class="mt-3 flex flex-wrap gap-2">
				<button
					type="button"
					onclick={ways?.onPutBack}
					disabled={!ways}
					class="h-9 rounded-full bg-accent px-4 text-small font-medium text-canvas transition-colors hover:bg-accenthi active:bg-accenthi disabled:cursor-not-allowed disabled:hover:bg-accent"
				>
					Put back
				</button>
				<button
					type="button"
					onclick={() => (asking = true)}
					disabled={!ways}
					class="h-9 rounded-full px-3 text-small text-txt3 transition-colors hover:bg-dangerwash hover:text-danger disabled:cursor-not-allowed disabled:hover:bg-transparent disabled:hover:text-txt3"
				>
					Delete forever…
				</button>
			</div>
		{/if}
	{/if}
</div>
