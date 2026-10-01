<script lang="ts">
	import type { Snippet } from 'svelte';
	import { when } from '$lib/format';
	import type { EntryRow } from '$lib/model';
	import Icon from './Icon.svelte';
	import Mask from './Mask.svelte';

	/** The list as it is drawn when no entry is open: every column the mockup
	 * gives it, and a copy button for the two fields worth copying. */
	let {
		rows,
		now,
		note,
		before,
		onOpen,
		onCopy
	}: {
		rows: EntryRow[];
		now: Date;
		/** A line under a row's name, for a list that has something to say about
		 * each row the columns do not: in the bin, when and where from. */
		note?: (row: EntryRow) => string;
		/** Rows of another kind at the top of the list, scrolled with it. */
		before?: Snippet;
		onOpen: (id: string) => void;
		onCopy: (row: EntryRow, field: 'UserName' | 'Password') => void;
	} = $props();

	/**
	 * The column holds two tags. A row with more says how many more rather than
	 * cutting the third one off in the middle of a word.
	 */
	const SHOWN = 2;
</script>

<div
	class="grid grid-cols-[1.5fr_0.85fr_0.85fr_78px_66px] gap-4 border-b border-line px-5 py-2 font-mono text-label tracking-label text-txt4 uppercase"
>
	<span>Name</span><span>Login</span><span>Tags</span><span class="text-right">Changed</span><span
	></span>
</div>

<div class="flex-1 overflow-y-auto text-body">
	{@render before?.()}
	{#each rows as row (row.id)}
		<!--
			The whole row opens the entry, not the name in it. The row is what
			lights up under the pointer, so the row is what a press has to answer:
			aiming at the top or the bottom of a lit row and getting nothing is the
			list saying one thing and doing another.

			So the row is the button, and the two copy buttons at the end are laid
			over the column left empty for them: a button cannot hold a button, and
			the other way round - a button stretched behind the row - is a row whose
			name a screen reader never reads out.
		-->
		<div class="relative border-b border-line transition-colors hover:bg-raised/50">
			<button
				type="button"
				onclick={() => onOpen(row.id)}
				class="grid w-full grid-cols-[1.5fr_0.85fr_0.85fr_78px_66px] items-center gap-4 px-5 py-[11px] text-left"
			>
				<span class="block min-w-0">
					<span class="flex min-w-0 items-center gap-2 truncate text-txt2">
						<Icon
							name={row.hasPassword || row.attachments === 0 ? 'key' : 'clip'}
							class="h-3.5 w-3.5 shrink-0 text-txt4"
						/>
						{#if row.title === null}
							<Mask />
						{:else}
							<span class="truncate">{row.title}</span>
						{/if}
					</span>
					{#if note}
						<span class="mt-1 block truncate pl-[22px] font-mono text-sub text-txt4">
							{note(row)}
						</span>
					{/if}
				</span>

				<span class="truncate font-mono text-fine text-txt3">
					{#if row.username === null}
						<Mask />
					{:else}
						{row.username}
					{/if}
				</span>

				<span class="flex min-w-0 gap-1.5 overflow-hidden">
					<!-- Keyed by position, not by the tag: a file may hold the same tag
					     twice, or two empty ones, and a duplicate key throws the whole
					     list away. A tag is a value; its place in the row is its
					     identity. -->
					{#each row.tags.slice(0, SHOWN) as tag, at (at)}
						<em
							class="truncate rounded-full bg-surface2 px-2 py-[3px] font-mono text-label tracking-label text-txt3 uppercase not-italic"
						>
							{tag}
						</em>
					{/each}
					{#if row.tags.length > SHOWN}
						<em
							class="shrink-0 rounded-full bg-surface2 px-2 py-[3px] font-mono text-label tracking-label text-txt4 uppercase not-italic"
						>
							+{row.tags.length - SHOWN}
						</em>
					{/if}
				</span>

				<span class="text-right font-mono text-meta text-txt4">{when(row.modified, now)}</span>

				<span aria-hidden="true"></span>
			</button>

			<!--
				The strip itself lets a press through to the row under it, and so does
				a copy button with nothing to copy. Otherwise the last column is
				sixty-six pixels of lit row that answers nothing, which is the defect
				this row was rebuilt for, turned on its side.
			-->
			<span
				class="pointer-events-none absolute inset-y-0 right-5 flex w-[66px] items-center justify-end gap-1"
			>
				<button
					type="button"
					onclick={() => onCopy(row, 'UserName')}
					disabled={row.username === ''}
					class="pointer-events-auto rounded-sm p-1 text-txt4 transition-colors hover:bg-surface2 hover:text-txt disabled:pointer-events-none disabled:cursor-not-allowed disabled:hover:bg-transparent disabled:hover:text-txt4"
					aria-label="Copy login"
					title="Copy login · ⌘B"
				>
					<Icon name="copy" class="h-4 w-4" />
				</button>
				<button
					type="button"
					onclick={() => onCopy(row, 'Password')}
					disabled={!row.hasPassword}
					class="pointer-events-auto rounded-sm p-1 text-txt4 transition-colors hover:bg-surface2 hover:text-txt disabled:pointer-events-none disabled:cursor-not-allowed disabled:hover:bg-transparent disabled:hover:text-txt4"
					aria-label="Copy password"
					title="Copy password · ⌘C"
				>
					<Icon name="key" class="h-4 w-4" />
				</button>
			</span>
		</div>
	{/each}
</div>
