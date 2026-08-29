<script lang="ts">
	import { SvelteSet } from 'svelte/reactivity';
	import {
		asFailure,
		copy as copyToClipboard,
		createEntry,
		createGroup,
		deleteEntry,
		deleteGroup,
		emptyRecycleBin,
		entry as loadEntry,
		reload,
		renameGroup,
		rival,
		save,
		saveCopy,
		saveOver,
		tree as loadTree,
		versions as loadVersions
	} from '$lib/ipc';
	import type { Database, Entry, EntryRow, Group, Rival, Version } from '$lib/model';
	import { index, search } from '$lib/search';
	import { entriesOf, liveEntries, pathTo, recycleBin, shownEntries } from '$lib/tree';
	import Conflict from './Conflict.svelte';
	import Empty from './Empty.svelte';
	import EntryList from './EntryList.svelte';
	import EntryListCompact from './EntryListCompact.svelte';
	import EntryView from './EntryView.svelte';
	import Icon from './Icon.svelte';
	import Toast from './Toast.svelte';
	import Tree from './Tree.svelte';

	let {
		database,
		root,
		readOnly,
		onTree
	}: {
		database: Database;
		root: Group;
		/** A snapshot, or a format Coffer reads and does not write. Nothing on
		 * the screen offers a change it would only be refused. */
		readOnly: boolean;
		onTree: (tree: Group) => void;
	} = $props();

	/** The group being shown, or `null` for everything the vault holds. */
	let group = $state<string | null>(null);
	let expanded = new SvelteSet<string>();
	let query = $state('');
	let opened = $state<Entry | null>(null);
	let versions = $state<Version[]>([]);
	let field = $state<HTMLInputElement>();
	let naming = $state(false);
	let named = $state<HTMLInputElement>();
	let renaming = $state(false);
	let emptying = $state(false);
	let deleting = $state(false);
	let saving = $state(false);
	let conflict = $state<Rival | null>(null);
	let changedAt = $state<Date | null>(null);
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
	/** Where a new folder or entry goes: the folder being shown, or the top of
	 * the vault when the list is showing everything. */
	const inside = $derived(group === null ? root.id : shown.id);
	const inBin = $derived(bin !== null && group === bin.id);

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
		naming = false;
		renaming = false;
		emptying = false;
		deleting = false;
	}

	async function open(id: string) {
		try {
			opened = await loadEntry(id);
			versions = await loadVersions(id);
		} catch (thrown) {
			failed(thrown);
		}
	}

	/**
	 * Writes the vault back after a change.
	 *
	 * There is no save button, so this is what one means: the change is already
	 * in the window, and this is the moment it reaches the file. A file somebody
	 * else wrote in the meantime stops here and asks.
	 */
	async function persist() {
		saving = true;
		try {
			await save();
		} catch (thrown) {
			if (asFailure(thrown).code === 'externalChange') {
				conflict = await rival().catch(() => ({ modified: null, entries: null }));
			} else {
				failed(thrown);
			}
		} finally {
			saving = false;
		}
	}

	/**
	 * An entry came back changed.
	 *
	 * The tree comes back with it. A row in the list is drawn from the tree, and
	 * a screen that changed a title in one pane and not in the other would go on
	 * filtering and searching on a value that is no longer in the file.
	 */
	async function changed(entry: Entry) {
		opened = entry;
		changedAt = new Date();
		await persist();
		await redraw(entry.id);
	}

	/**
	 * Reads back what the save left.
	 *
	 * After the save, not before it: a save brings every entry's history inside
	 * the database's limits, and a version is addressed by its position, so the
	 * list read before a save can name versions that are no longer there.
	 */
	async function redraw(id: string) {
		try {
			versions = await loadVersions(id);
			onTree(await loadTree());
		} catch (thrown) {
			failed(thrown);
		}
	}

	/** The versions of the open entry came back changed, which is a change to
	 * the file like any other. */
	async function versionsChanged(found: Version[]) {
		versions = found;
		changedAt = new Date();
		await persist();
		if (opened) await redraw(opened.id);
	}

	/** The tree came back changed. */
	async function reshaped(tree: Group) {
		onTree(tree);
		changedAt = new Date();
		await persist();
	}

	async function addEntry() {
		try {
			const made = await createEntry(inside);
			onTree(made.tree);
			changedAt = new Date();
			await open(made.entry);
			await persist();
		} catch (thrown) {
			failed(thrown);
		}
	}

	async function removeEntry() {
		if (!opened) return;
		const id = opened.id;
		try {
			const tree = await deleteEntry(id);
			opened = null;
			versions = [];
			await reshaped(tree);
		} catch (thrown) {
			failed(thrown);
		}
	}

	/** Opens the field that asks for a name, and closes it again. The press is
	 * kept from moving the focus, so the open field does not write a folder on
	 * its way out and leave the button looking as though it had done nothing. */
	function addFolder() {
		renaming = false;
		naming = !naming;
		if (!naming) return;
		queueMicrotask(() => named?.focus());
	}

	async function makeFolder() {
		if (!named) return;
		const name = named.value.trim();
		named.value = '';
		naming = false;
		if (name === '') return;
		try {
			await reshaped(await createGroup(inside, name));
		} catch (thrown) {
			failed(thrown);
		}
	}

	async function rename(name: string) {
		renaming = false;
		if (group === null || name.trim() === '') return;
		try {
			await reshaped(await renameGroup(group, name.trim()));
		} catch (thrown) {
			failed(thrown);
		}
	}

	async function removeFolder() {
		deleting = false;
		if (group === null) return;
		try {
			const tree = await deleteGroup(group);
			select(null);
			await reshaped(tree);
		} catch (thrown) {
			failed(thrown);
		}
	}

	async function empty() {
		emptying = false;
		try {
			const tree = await emptyRecycleBin();
			opened = null;
			await reshaped(tree);
		} catch (thrown) {
			failed(thrown);
		}
	}

	async function takeTheirs() {
		try {
			saving = true;
			const tree = await reload();
			onTree(tree);
			opened = null;
			versions = [];
			changedAt = null;
			conflict = null;
		} catch (thrown) {
			failed(thrown);
		} finally {
			saving = false;
		}
	}

	async function keepBoth() {
		try {
			saving = true;
			const beside = await saveCopy();
			if (!beside) return;
			conflict = null;
			notice = { message: `Kept as ${beside.name}`, kind: 'copied' };
			copies += 1;
			fade();
			await takeTheirs();
		} catch (thrown) {
			failed(thrown);
		} finally {
			saving = false;
		}
	}

	async function keepOurs() {
		try {
			saving = true;
			await saveOver();
			conflict = null;
		} catch (thrown) {
			failed(thrown);
		} finally {
			saving = false;
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
		fade();
	}

	function fade() {
		fading = setTimeout(() => {
			notice = null;
			fading = null;
		}, 6000);
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

	/** Whether the key went to somewhere the reader is writing. */
	function typing(target: EventTarget | null): boolean {
		return (
			target instanceof HTMLInputElement ||
			target instanceof HTMLTextAreaElement ||
			(target instanceof HTMLElement && target.isContentEditable)
		);
	}

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
		// not the entry's - and a selection inside a field is not something
		// `getSelection` reports at all, so the field itself is the answer there.
		if (typing(event.target)) return;
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
		<div class="flex shrink-0 items-center gap-1 px-4 py-3">
			<span class="flex-1 font-mono text-label tracking-label text-txt3 uppercase">Folders</span>
			{#if !readOnly}
				{#if group !== null && !inBin}
					<button
						type="button"
						onmousedown={(event) => event.preventDefault()}
						onclick={() => {
							naming = false;
							renaming = !renaming;
						}}
						class="text-txt4 transition-colors hover:text-txt2"
						aria-label={renaming ? 'Leave the name as it is' : 'Rename this folder'}
					>
						<Icon name="check" class="h-4 w-4" />
					</button>
					<button
						type="button"
						onclick={() => (deleting = true)}
						class="text-txt4 transition-colors hover:text-danger"
						aria-label="Delete this folder"
					>
						<Icon name="trash" class="h-4 w-4" />
					</button>
				{/if}
				<button
					type="button"
					onmousedown={(event) => event.preventDefault()}
					onclick={addFolder}
					class="text-txt4 transition-colors hover:text-txt2"
					aria-label={naming ? 'Never mind the new folder' : 'New folder'}
				>
					<Icon name="plus" class="h-4 w-4" />
				</button>
			{/if}
		</div>

		{#if naming}
			<input
				bind:this={named}
				type="text"
				autocomplete="off"
				spellcheck="false"
				aria-label="The name of the new folder"
				placeholder="What is it called?"
				onblur={makeFolder}
				onkeydown={(event) => {
					if (event.key === 'Escape') naming = false;
					if (event.key === 'Enter') {
						event.preventDefault();
						makeFolder();
					}
				}}
				class="mx-2 mb-2 shrink-0 rounded-sm border border-accent bg-surface px-2 py-1.5 text-body text-txt ring-4 ring-accent/15 outline-none placeholder:text-txt4"
			/>
		{/if}

		{#if renaming && group !== null}
			<input
				type="text"
				autocomplete="off"
				spellcheck="false"
				aria-label="A new name for this folder"
				value={shown.name}
				onblur={(event) => rename(event.currentTarget.value)}
				onkeydown={(event) => {
					if (event.key === 'Escape') renaming = false;
					if (event.key === 'Enter') {
						event.preventDefault();
						rename(event.currentTarget.value);
					}
				}}
				class="mx-2 mb-2 shrink-0 rounded-sm border border-accent bg-surface px-2 py-1.5 text-body text-txt ring-4 ring-accent/15 outline-none"
			/>
		{/if}

		{#if deleting && group !== null}
			<div class="mx-2 mb-2 shrink-0 rounded-sm border border-hairline bg-surface p-3">
				<p class="text-fine leading-relaxed text-txt2">
					Delete “{shown.name}” and everything in it?
				</p>
				<div class="mt-3 flex gap-2">
					<button
						type="button"
						onclick={() => (deleting = false)}
						class="h-9 rounded-full px-3 text-small text-txt3 transition-colors hover:text-txt2"
					>
						Keep it
					</button>
					<button
						type="button"
						onclick={removeFolder}
						class="h-9 rounded-full px-3 text-small text-danger transition-colors hover:bg-dangerwash"
					>
						Delete
					</button>
				</div>
			</div>
		{/if}

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

				{#if inBin && !readOnly && shownEntries(deleted).length > 0}
					{#if emptying}
						<div class="mt-2 rounded-sm border border-hairline bg-surface p-3">
							<p class="text-fine leading-relaxed text-txt2">
								Take all of it out of the file? This is the one deletion nothing comes back from.
							</p>
							<div class="mt-3 flex gap-2">
								<button
									type="button"
									onclick={() => (emptying = false)}
									class="h-9 rounded-full px-3 text-small text-txt3 transition-colors hover:text-txt2"
								>
									Keep it
								</button>
								<button
									type="button"
									onclick={empty}
									class="h-9 rounded-full px-3 text-small text-danger transition-colors hover:bg-dangerwash"
								>
									Empty it
								</button>
							</div>
						</div>
					{:else}
						<button
							type="button"
							onclick={() => (emptying = true)}
							class="mt-1 w-full rounded-sm px-2 py-1.5 text-left text-fine text-txt4 transition-colors hover:text-txt3"
						>
							Empty the bin
						</button>
					{/if}
				{/if}
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
					aria-label="Filter the list"
					class="min-w-0 flex-1 bg-transparent text-body text-txt outline-none placeholder:text-txt4"
				/>
				<kbd
					class="shrink-0 rounded-xs border border-hairline px-1.5 py-0.5 font-mono text-label text-txt4"
				>
					⌘F
				</kbd>
			</span>
			{#if !readOnly && !inBin}
				<button
					type="button"
					onclick={addEntry}
					class="flex h-9 shrink-0 items-center gap-2 rounded-full border border-hairline px-4 text-small text-txt2 transition-colors hover:border-txt4 hover:text-txt"
				>
					<Icon name="plus" class="h-4 w-4" /> Entry
				</button>
			{/if}
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
			>
				{#snippet action()}
					{#if !readOnly}
						<button
							type="button"
							onclick={addEntry}
							class="h-9 rounded-full border border-hairline px-5 text-small text-txt transition-colors hover:border-txt3"
						>
							Add an entry
						</button>
					{/if}
				{/snippet}
			</Empty>
		{:else if found.length === 0 && inBin}
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
			>
				{#snippet action()}
					{#if !readOnly}
						<button
							type="button"
							onclick={addEntry}
							class="h-9 rounded-full border border-hairline px-5 text-small text-txt transition-colors hover:border-txt3"
						>
							Add an entry
						</button>
					{/if}
				{/snippet}
			</Empty>
		{:else if opened}
			<EntryListCompact rows={found} open={opened.id} onOpen={open} />
		{:else}
			<EntryList rows={found} {now} onOpen={open} onCopy={copyFrom} />
		{/if}
	</div>

	{#if opened}
		<EntryView
			entry={opened}
			{path}
			{versions}
			{now}
			{readOnly}
			onCopy={copy}
			onChanged={changed}
			onVersions={versionsChanged}
			onDelete={removeEntry}
			onFailure={failed}
		/>
	{/if}

	{#if conflict}
		<Conflict
			rival={conflict}
			entries={entriesOf(root).length}
			{changedAt}
			{now}
			busy={saving}
			onReload={takeTheirs}
			onCopy={keepBoth}
			onOverwrite={keepOurs}
		/>
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
	{#if saving}
		<span class="ml-auto shrink-0 text-txt3">Saving…</span>
	{:else if readOnly}
		<span class="ml-auto shrink-0 text-txt3">Read only</span>
	{/if}
</div>
