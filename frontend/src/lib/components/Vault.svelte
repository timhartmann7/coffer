<script lang="ts">
	import { SvelteSet } from 'svelte/reactivity';
	import { asFailure, copy as copyToClipboard, entry as loadEntry } from '$lib/ipc';
	import type { Database, Entry, EntryRow, Group } from '$lib/model';
	import { index, search } from '$lib/search';
	import { liveEntries, pathTo, recycleBin, shownEntries } from '$lib/tree';
	import Empty from './Empty.svelte';
	import EntryList from './EntryList.svelte';
	import EntryListCompact from './EntryListCompact.svelte';
	import EntryView from './EntryView.svelte';
	import Icon from './Icon.svelte';
	import Toast from './Toast.svelte';
	import Tree from './Tree.svelte';

	let { database, root }: { database: Database; root: Group } = $props();

	/** The group being shown, or `null` for everything the vault holds. */
	let group = $state<string | null>(null);
	let expanded = new SvelteSet<string>();
	let query = $state('');
	let opened = $state<Entry | null>(null);
	let field = $state<HTMLInputElement>();
	let notice = $state<{ message: string; kind: 'copied' | 'failed' } | null>(null);
	let countdown: ReturnType<typeof setInterval> | null = null;
	let fading: ReturnType<typeof setTimeout> | null = null;
	/** Which copy the toast is about. The bar that drains is a CSS animation and
	 * an animation does not start again on its own, so the toast is rebuilt. */
	let copies = $state(0);

	// Timestamps are written against the moment the vault was opened rather than
	// against a clock that ticks, so that a list of a thousand rows is not
	// redrawn once a second to move one of them from "23:59" to "yesterday".
	const now = new Date();

	const shown = $derived(group === null ? root : (findGroup(root, group) ?? root));
	// Walked once. The tree is the whole vault, and three walks of fifty
	// thousand entries to draw one screen is three too many.
	const live = $derived(liveEntries(root));
	const rows = $derived(group === null ? live : shownEntries(shown));
	const indexed = $derived(index(rows));
	const found = $derived(search(indexed, query));
	const bin = $derived(recycleBin(root));
	const path = $derived(opened ? (pathTo(root, opened.group) ?? []).slice(1) : []);

	function findGroup(group: Group, id: string): Group | null {
		if (group.id === id) return group;
		for (const section of group.sections) {
			const here = findGroup(section, id);
			if (here) return here;
		}
		return null;
	}

	function select(id: string | null) {
		group = id;
		opened = null;
		query = '';
	}

	async function open(id: string) {
		try {
			opened = await loadEntry(id);
		} catch (thrown) {
			failed(thrown);
		}
	}

	async function copy(entry: string, name: string) {
		try {
			const seconds = await copyToClipboard(entry, name);
			announce(seconds);
		} catch (thrown) {
			failed(thrown);
		}
	}

	function copyFrom(row: EntryRow, name: 'UserName' | 'Password') {
		return copy(row.id, name);
	}

	/** The clipboard holds the value for as long as Rust says it will, and the
	 * message counts the same seconds down. */
	function announce(seconds: number) {
		clear();
		copies += 1;
		let left = seconds;
		notice = { message: message(left), kind: 'copied' };
		countdown = setInterval(() => {
			left -= 1;
			if (left <= 0) {
				clear();
				notice = null;
				return;
			}
			notice = { message: message(left), kind: 'copied' };
		}, 1000);
	}

	function message(left: number): string {
		const minutes = Math.floor(left / 60);
		return `Copied. Clipboard clears in ${minutes}:${String(left % 60).padStart(2, '0')}`;
	}

	function failed(thrown: unknown) {
		clear();
		notice = { message: asFailure(thrown).message, kind: 'failed' };
		fading = setTimeout(() => {
			notice = null;
			fading = null;
		}, 4000);
	}

	function clear() {
		if (countdown !== null) {
			clearInterval(countdown);
			countdown = null;
		}
		if (fading !== null) {
			clearTimeout(fading);
			fading = null;
		}
	}

	$effect(() => () => clear());

	function shortcut(event: KeyboardEvent) {
		if (!event.metaKey) {
			if (event.key === 'Escape') {
				if (query !== '') query = '';
				else opened = null;
			}
			return;
		}

		if (event.key === 'f') {
			event.preventDefault();
			field?.focus();
			field?.select();
			return;
		}

		if (!opened) return;
		const wanted = event.key === 'b' ? 'username' : event.key === 'c' ? 'password' : null;
		if (!wanted) return;

		// A revealed value, a login and a note are all selectable on purpose. If
		// the reader has selected something, the copy they pressed is theirs and
		// not the entry's.
		if (wanted === 'password' && document.getSelection()?.isCollapsed === false) return;

		const chosen = opened.fields.find((entry) => entry.kind === wanted);
		if (!chosen || chosen.empty) return;
		event.preventDefault();
		copy(opened.id, chosen.name);
	}
</script>

<svelte:window onkeydown={shortcut} />

<div
	class="relative grid flex-1 overflow-hidden {opened
		? 'grid-cols-[200px_minmax(230px,1fr)_384px]'
		: 'grid-cols-[228px_1fr]'}"
>
	<aside class="flex flex-col overflow-hidden border-r border-hairline bg-surface2">
		<div class="shrink-0 px-4 py-3 font-mono text-label tracking-label text-txt3 uppercase">
			Folders
		</div>

		<div class="flex-1 overflow-y-auto px-2 pb-2 text-body">
			<button
				type="button"
				onclick={() => select(null)}
				class="relative flex w-full items-center gap-2 rounded-sm px-2 py-[7px] transition-colors {group ===
				null
					? 'bg-raised text-txt'
					: 'text-txt2 hover:bg-raised/60'}"
			>
				{#if group === null}
					<span
						class="absolute top-1 left-0 h-[calc(100%-8px)] w-[2px] rounded-full bg-accent"
						aria-hidden="true"
					></span>
				{/if}
				<Icon
					name="folder"
					class="h-4 w-4 shrink-0 {group === null ? 'text-accent' : 'text-txt4'}"
				/>
				<span class="flex-1 text-left">All entries</span>
				<span class="font-mono text-meta {group === null ? 'text-txt3' : 'text-txt4'}">
					{live.length}
				</span>
			</button>

			<Tree
				{root}
				selected={group}
				{expanded}
				onSelect={select}
				onToggle={(id) => (expanded.has(id) ? expanded.delete(id) : expanded.add(id))}
			/>
		</div>

		{#if bin}
			{@const deleted = bin}
			<div class="shrink-0 border-t border-hairline px-2 py-2">
				<button
					type="button"
					onclick={() => select(deleted.id)}
					class="relative flex w-full items-center gap-2 rounded-sm px-2 py-[7px] text-body transition-colors {group ===
					deleted.id
						? 'bg-raised text-txt'
						: 'text-txt3 hover:bg-raised/60 hover:text-txt2'}"
				>
					{#if group === deleted.id}
						<span
							class="absolute top-1 left-0 h-[calc(100%-8px)] w-[2px] rounded-full bg-accent"
							aria-hidden="true"
						></span>
					{/if}
					<Icon name="trash" class="h-4 w-4 shrink-0" />
					<span class="flex-1 text-left">{deleted.name}</span>
					<span class="font-mono text-meta text-txt4">{shownEntries(deleted).length}</span>
				</button>
			</div>
		{/if}
	</aside>

	<div class="flex flex-col overflow-hidden {opened ? 'border-r border-hairline' : ''}">
		<div class="flex shrink-0 items-center gap-3 border-b border-hairline px-5 py-3">
			<span
				class="flex flex-1 items-center gap-2 rounded-sm border border-hairline bg-surface2 px-3 py-2 focus-within:border-accent focus-within:ring-4 focus-within:ring-accent/15"
			>
				<Icon name="search" class="h-4 w-4 shrink-0 text-txt4" />
				<input
					bind:this={field}
					bind:value={query}
					type="text"
					autocomplete="off"
					spellcheck="false"
					placeholder="Title, login, address, tag"
					class="min-w-0 flex-1 bg-transparent text-body text-txt outline-none placeholder:text-txt4"
				/>
				<kbd
					class="shrink-0 rounded-xs border border-hairline px-1.5 py-0.5 font-mono text-label text-txt4"
				>
					⌘F
				</kbd>
			</span>
		</div>

		{#if found.length === 0 && query !== ''}
			<Empty
				icon="search"
				title="Nothing matches “{query}”"
				detail="Coffer searches titles, logins, addresses and tags. Notes and custom fields are not searched: they never leave the vault."
			>
				{#snippet action()}
					<button
						type="button"
						onclick={() => (query = '')}
						class="h-9 rounded-full border border-hairline px-5 text-small text-txt transition-colors hover:border-txt3"
					>
						Clear the search
					</button>
				{/snippet}
			</Empty>
		{:else if found.length === 0 && group === null}
			<Empty
				icon="folder"
				title="This vault has nothing in it yet"
				detail="Entries added in Coffer or in any other KeePass client show up here."
			/>
		{:else if found.length === 0 && bin && group === bin.id}
			<Empty
				icon="trash"
				title="The recycle bin is empty"
				detail="Deleted entries stay here until the bin is emptied by hand."
			/>
		{:else if found.length === 0}
			<Empty
				icon="folder"
				title="There is nothing in “{shown.name}” yet"
				detail="Entries live in folders. This one has none of its own."
			/>
		{:else if opened}
			<EntryListCompact rows={found} open={opened.id} onOpen={open} />
		{:else}
			<EntryList rows={found} {now} onOpen={open} onCopy={copyFrom} />
		{/if}
	</div>

	{#if opened}
		<EntryView entry={opened} {path} {now} onCopy={copy} onFailure={failed} />
	{/if}

	{#if notice}
		{#key copies}
			<Toast message={notice.message} kind={notice.kind} />
		{/key}
	{/if}
</div>

<div
	class="flex shrink-0 items-center gap-4 border-t border-hairline bg-surface2 px-4 py-2 font-mono text-label tracking-label text-txt4 uppercase"
>
	<span>
		{found.length}
		{found.length === 1 ? 'entry' : 'entries'} here · {live.length} in the vault
	</span>
	<span class="truncate">{database.path}</span>
</div>
