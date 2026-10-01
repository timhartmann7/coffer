<script lang="ts">
	import type { Snippet } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import {
		asFailure,
		copy as copyToClipboard,
		copyVersion,
		createEntry,
		createGroup,
		entry as loadEntry,
		renameGroup,
		tree as loadTree,
		undoRemoval,
		versions as loadVersions
	} from '$lib/ipc';
	import { deleted as deletedLine, eraseQuestion } from '$lib/bin';
	import { flush } from '$lib/drafts';
	import { named as howLong } from '$lib/duration';
	import { quoted } from '$lib/format';
	import { held } from '$lib/holding';
	import { copying, typing } from '$lib/keys';
	import { composing } from '$lib/lines';
	import { Moves } from '$lib/moves';
	import { COPIED, Notices } from '$lib/notices.svelte';
	import { conceal } from '$lib/reveal.svelte';
	import { Saving } from '$lib/saving.svelte';
	import type { Database, Entry, EntryRow, Group, History, Position, Span } from '$lib/model';
	import { index, search } from '$lib/search';
	import {
		entriesOf,
		find,
		inBin,
		liveEntries,
		pathTo,
		recycleBin,
		rowOf,
		searchedEntries,
		shownEntries
	} from '$lib/tree';
	import BinFolders from './BinFolders.svelte';
	import Confirm from './Confirm.svelte';
	import Conflict from './Conflict.svelte';
	import Empty from './Empty.svelte';
	import EntryList from './EntryList.svelte';
	import EntryListCompact from './EntryListCompact.svelte';
	import EntryView from './EntryView.svelte';
	import Icon from './Icon.svelte';
	import InBin from './InBin.svelte';
	import Opening from './Opening.svelte';
	import Toast from './Toast.svelte';
	import Tree from './Tree.svelte';

	let {
		database,
		root,
		readOnly,
		settings,
		onSettings,
		onTree
	}: {
		database: Database;
		root: Group;
		/** A snapshot, or a format Coffer reads and does not write. Nothing on
		 * the screen offers a change it would only be refused. */
		readOnly: boolean;
		/**
		 * The settings screen, when it is the one being read.
		 *
		 * Drawn here rather than in place of this whole screen so that the way in
		 * to it stays where it was pressed. It sits in the status bar, and a
		 * settings screen that replaced the status bar as well moved its own
		 * button to the other corner of the window the moment it opened - which
		 * is a button walking away from the finger that is still on it.
		 */
		settings?: Snippet;
		/** Opens the settings screen, and closes it again. */
		onSettings: () => void;
		onTree: (tree: Group) => void;
	} = $props();

	/** The group being shown, or `null` for everything the vault holds. */
	let group = $state<string | null>(null);
	let expanded = new SvelteSet<string>();
	let query = $state('');

	/**
	 * The entry in the pane, from the moment its row is pressed.
	 *
	 * Rust answers one command at a time, and a save holds it for a whole key
	 * derivation and the encryption of the vault. An entry chosen in that second
	 * is read once the save is done, and one that waited with the previous entry
	 * still in the pane - and its versions set by whichever answer came last -
	 * showed one entry's history under another's name, with Restore and Delete
	 * aimed at positions in the wrong list.
	 *
	 * So the pane is the entry's id first, drawn from the row that was pressed,
	 * and the entry and its versions arrive into it. Every answer about an entry
	 * goes through `land` or `listed`, which put it into the pane only while the
	 * pane is still on that entry; choosing another makes a new pane, so nothing
	 * the last one held can be drawn under the next. Replaced whole rather than
	 * changed in place, so it is not watched field by field.
	 */
	type Pane = {
		id: string;
		/** The row the entry was chosen from, which is what is drawn while the
		 * entry is being read. */
		row: EntryRow;
		/** The entry as Rust last answered, or `null` until it has. */
		entry: Entry | null;
		/** Its previous versions, once Rust has listed them. */
		history: History | null;
	};

	let pane = $state.raw<Pane | null>(null);
	/** Which entry the pane is on, read or not. */
	const showing = $derived(pane?.id ?? null);
	/** The entry in the pane, once Rust has read it. Nothing is offered on an
	 * entry before that: nothing of it but its row is known. */
	const opened = $derived(pane?.entry ?? null);
	let field = $state<HTMLInputElement>();
	let naming = $state(false);
	let named = $state<HTMLInputElement>();
	let renaming = $state(false);
	let emptying = $state(false);
	let deleting = $state(false);

	const notices = new Notices(failed);
	const file = new Saving({
		reread,
		reloaded: (tree) => {
			onTree(tree);
			pane = null;
		},
		failed,
		notices
	});
	const moves = new Moves({
		root: () => root,
		showing: () => showing,
		unsaved: () => file.unsaved,
		close: () => (pane = null),
		select,
		open,
		redraw,
		read,
		unread,
		expand: (id) => expanded.add(id),
		reshaped,
		failed,
		notices
	});

	// Timestamps are written against the moment the vault was opened rather than
	// against a clock that ticks, so that a list of a thousand rows is not
	// redrawn once a second to move one of them from "23:59" to "yesterday".
	const now = new Date();

	const shown = $derived(group === null ? root : (find(root, group) ?? root));
	// Walked once. The tree is the whole vault, and three walks of fifty
	// thousand entries to draw one screen is three too many.
	const live = $derived(liveEntries(root));
	const rows = $derived(
		group === null ? live : query === '' ? shownEntries(shown) : searchedEntries(shown)
	);
	const indexed = $derived(index(rows));
	const found = $derived(search(indexed, query));
	const bin = $derived(recycleBin(root));
	const path = $derived(pane ? (pathTo(root, (pane.entry ?? pane.row).group) ?? []).slice(1) : []);
	/** Where a new folder or entry goes: the folder being shown, or the top of
	 * the vault when the list is showing everything. */
	const inside = $derived(group === null ? root.id : shown.id);
	/** Whether the list is showing the recycle bin or a folder inside it, where
	 * nothing is made or changed and everything is read one folder at a time. */
	const binned = $derived(group !== null && inBin(shown));
	/** The folders drawn above the entries in the bin. A search is for entries,
	 * and a folder row among its answers would be one it did not look inside. */
	const folders = $derived(binned && query === '' ? shown.sections : []);
	/** The line under a row in the bin: when it went in, and where from or with
	 * which deleted folder, which is what a row a search found further down
	 * says about where it is. */
	const whence = $derived(
		binned ? (row: EntryRow) => (row.binned ? deletedLine(row.binned, root, now) : '') : undefined
	);

	/**
	 * Puts the pane away, unless it has to stay: a new value typed into it that
	 * nobody has answered for, or one on its way to the vault. The field that
	 * holds it puts its question, with the focus on it, and nothing here moves
	 * until the reader has answered (`holding.ts`).
	 *
	 * `held` is asked by everything the reader presses that would take the pane
	 * away or put another entry in it, and before it does anything else: a
	 * folder moved to the bin with the entry inside it, and only then refused,
	 * would have moved the entry out from under the question.
	 */
	function dismiss() {
		if (!held()) pane = null;
	}

	/** Shows a folder the reader chose, unless the pane has to stay. */
	function choose(id: string | null) {
		if (!held()) select(id);
	}

	function select(id: string | null) {
		group = id;
		pane = null;
		query = '';
		naming = false;
		renaming = false;
		emptying = false;
		deleting = false;
	}

	/**
	 * Puts an entry in the pane at once, and reads it.
	 *
	 * The row is on the screen before Rust has answered, and whatever the pane
	 * was showing goes with it: the versions under the name are only ever this
	 * entry's, because there are none until Rust lists them. The row of the
	 * entry already in the pane reads it again where it stands, with nothing
	 * taken off the screen in the meantime.
	 */
	async function open(row: EntryRow) {
		if (showing !== row.id) {
			if (held()) return;
			pane = { id: row.id, row, entry: null, history: null };
		}
		await read(row.id);
		await list(row.id);
	}

	/**
	 * Reads the entry in the pane from Rust.
	 *
	 * An entry that cannot be read - gone from the vault while it was waiting
	 * its turn - takes the pane with it, and the reader is told why. An answer
	 * about an entry the pane has left is nobody's, whatever it says.
	 */
	async function read(id: string) {
		if (showing !== id) return;
		try {
			land(await loadEntry(id));
		} catch (thrown) {
			if (showing !== id) return;
			pane = null;
			failed(thrown);
		}
	}

	/** Lists the versions of the entry in the pane, while it is still there. */
	async function list(id: string) {
		if (showing !== id) return;
		try {
			listed(await loadVersions(id));
		} catch (thrown) {
			if (showing === id) failed(thrown);
		}
	}

	/** Puts an entry Rust answered with into the pane, if the pane is on it. */
	function land(entry: Entry) {
		if (pane?.id === entry.id) pane = { ...pane, entry };
	}

	/** Puts a list of versions under the entry it is the list of, if the pane is
	 * on that entry. */
	function listed(history: History) {
		if (pane?.id === history.entry) pane = { ...pane, history };
	}

	/**
	 * Takes the versions of the entry in the pane off the screen until they are
	 * read again.
	 *
	 * For a list that is known to be out of date: after a change to the entry
	 * has landed and before its save is back, and after Rust refused a position
	 * from it. A save prunes, a version is addressed by its position, and the
	 * list from before one was drawn and pressable for the whole second the save
	 * held Rust: its trash dropped the version next to the one on its row, for
	 * good. With no list there is nothing to press, and no number beside
	 * "Versions".
	 */
	function unread(id: string) {
		if (pane?.id === id) pane = { ...pane, history: null };
	}

	/**
	 * Reads the versions of the entry in the pane back after a write.
	 *
	 * After the save, not before it: a save brings every entry's history inside
	 * the database's limits, and a version is addressed by its position, so the
	 * list read before a save can name versions that are no longer there. Rust
	 * refuses a position read before any change since (see `Position`), so this
	 * follows every write, whichever entry it was to - a list not read again is
	 * one whose every press would only be refused.
	 */
	async function reread() {
		if (showing !== null) await list(showing);
	}

	/**
	 * An entry came back changed.
	 *
	 * Every value of it on the screen goes first: what was shown before the
	 * change is not what the entry holds after it, whether the change was a new
	 * password saved, one made and put in, a version restored or anything else.
	 *
	 * The tree comes back with it. A row in the list is drawn from the tree, and
	 * a screen that changed a title in one pane and not in the other would go on
	 * filtering and searching on a value that is no longer in the file.
	 */
	async function changed(entry: Entry) {
		conceal(entry.id);
		land(entry);
		unread(entry.id);
		file.changedAt = new Date();
		await file.persist();
		await redraw();
	}

	/**
	 * Reads back the tree the save left.
	 *
	 * The tree is the whole vault's rather than the entry's, and a reader who
	 * moved on during the save still has the entry's row in the list, which has
	 * to say what the file now says.
	 */
	async function redraw() {
		try {
			onTree(await loadTree());
		} catch (thrown) {
			failed(thrown);
		}
	}

	/** The versions of an entry came back changed, which is a change to the
	 * file like any other. */
	async function versionsChanged(history: History) {
		listed(history);
		file.changedAt = new Date();
		await file.persist();
		await redraw();
	}

	/** The tree came back changed. */
	async function reshaped(tree: Group) {
		onTree(tree);
		file.changedAt = new Date();
		await file.persist();
	}

	/**
	 * Makes an entry and opens it.
	 *
	 * Opened only when the pane is where it was when the button was pressed: an
	 * entry the reader chose while Rust was making this one is the later choice,
	 * and the new entry waits in the list rather than taking the pane from it.
	 */
	async function addEntry() {
		if (held()) return;
		const at = showing;
		try {
			const made = await createEntry(inside);
			onTree(made.tree);
			file.changedAt = new Date();
			const row = rowOf(made.tree, made.entry);
			if (row && showing === at) await open(row);
			await file.persist();
		} catch (thrown) {
			failed(thrown);
		}
	}

	/** Moves the entry in the pane to the bin, or out of the file. */
	function removeEntry() {
		if (opened && !held()) void moves.removeEntry(opened);
	}

	/** Takes the entry in the pane out of the bin. */
	function putBack() {
		if (opened) void moves.putBackEntry(opened.id);
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

	/** Deletes the folder being shown, after the question in the folders pane,
	 * and offers it back when it went to the bin. */
	function removeFolder() {
		if (held()) return;
		deleting = false;
		if (group !== null) void moves.removeFolder(group, quoted(shown.name), shown.deletion);
	}

	/** Deletes the folder being shown in the bin for good. */
	function eraseFolder() {
		if (group !== null && !held()) void moves.eraseFolder(group, quoted(shown.name));
	}

	/** Empties the bin, after the question under it. */
	function empty() {
		if (held()) return;
		emptying = false;
		void moves.empty();
	}

	/**
	 * Copies a value in Rust and says so: a field of an entry, or of one of its
	 * previous versions, whole or the part of it the reader selected. Every copy
	 * in the window comes through here, so every one of them gets the same
	 * concealed, self-clearing pasteboard write and the same notice.
	 */
	async function copy(entry: string, name: string, range: Span | null = null, version?: Position) {
		try {
			const seconds =
				version === undefined
					? await copyToClipboard(entry, name, range)
					: await copyVersion(entry, version, name, range);
			announce(seconds);
		} catch (thrown) {
			failed(thrown);
		}
	}

	function copyFrom(row: EntryRow, name: 'UserName' | 'Password') {
		return copy(row.id, name);
	}

	/**
	 * Says what was copied and how long the clipboard will hold it, then goes.
	 *
	 * The seconds are Rust's answer and the sentence states them once. Counting
	 * them down was a notice about something already decided sitting in the
	 * corner for a whole minute.
	 */
	function announce(seconds: number) {
		notices.tell(
			{ message: `Copied. The clipboard clears in ${howLong(seconds)}.`, kind: 'copied' },
			COPIED
		);
	}

	function failed(thrown: unknown) {
		const refused = asFailure(thrown);
		if (refused.code === 'versionsChanged') {
			void outdated();
			return;
		}
		notices.warn(refused.message);
	}

	/**
	 * A version was pressed in a list the vault has changed since - behind a
	 * save, usually, which Rust answers first - and Rust refused its position
	 * rather than act on whichever version had moved into it. Nothing was done,
	 * so the reader is told why, and the list is read again to choose from.
	 */
	async function outdated() {
		notices.warn(
			'The versions changed while you were choosing, so nothing was done. Choose again from the list as it is now.'
		);
		if (showing === null) return;
		unread(showing);
		await list(showing);
	}

	$effect(() => () => notices.clear());

	/**
	 * A field of the reader's own came off an entry.
	 *
	 * It is offered back only once the removal is in the file. A removal the
	 * save refused has a notice of its own already, and the standing "Not
	 * saved" beside it, and an offer over that notice would push the one
	 * sentence that matters off the screen. One the reader agreed was for good,
	 * because the vault keeps no version to bring it back from, is said to be,
	 * the way an entry deleted forever is, and offered back to nobody.
	 *
	 * The undo is one question to Rust: which version puts the field back, and
	 * its restore, under one lock. Rust refuses when the removal is no longer
	 * the last thing that happened to the entry, and nothing is restored.
	 */
	function fieldRemoved(entry: string, name: string, forever: boolean) {
		if (file.unsaved) return;
		const said = `Field ${quoted(name)} removed`;
		if (forever) {
			notices.tell({ message: `${said} forever`, kind: 'removed' });
			return;
		}
		notices.offer(said, async () => {
			let restored: Entry;
			try {
				restored = await undoRemoval(entry, name);
			} catch (thrown) {
				if (asFailure(thrown).code !== 'superseded') throw thrown;
				notices.warn('The entry has changed since, so that can no longer be undone.');
				return;
			}
			await changed(restored);
		});
	}

	function shortcut(event: KeyboardEvent) {
		// The settings screen is over the list, so nothing that acts on the list
		// is what a key means while it is open. Escape is the way back out of it,
		// which is the same way back out that Escape is everywhere else here -
		// unless a control inside them has already answered it. A chip whose list
		// was open took its own Escape and the whole screen closed behind it.
		if (settings) {
			if (event.key === 'Escape' && !event.defaultPrevented) onSettings();
			return;
		}

		if (!event.metaKey) {
			if (event.key === 'Escape') {
				if (query !== '') query = '';
				else dismiss();
			}
			return;
		}

		if (event.key === 'f') {
			event.preventDefault();
			field?.focus();
			field?.select();
			return;
		}

		// A field's own undo is the field's. Cmd+Z in the search box or in a
		// value being written takes back typing, and the offer waits for a key
		// that is not aimed at one.
		if (event.key.toLowerCase() === 'z' && !event.shiftKey) {
			if (!notices.offering || typing(event.target)) return;
			event.preventDefault();
			notices.takeBack();
			return;
		}

		if (!opened) return;
		const wanted = event.key === 'b' ? 'username' : event.key === 'c' ? 'password' : null;
		if (!wanted) return;

		// A key a row of the pane has already answered: Cmd+C on a protected
		// value's own row copies that value, not the password.
		if (event.defaultPrevented) return;
		// Text the reader is writing is theirs to copy. A selection is copied by
		// the node holding it, and a revealed value's node hands that to Rust
		// itself, with the part that was selected.
		if (wanted === 'password' ? !copying(event) : typing(event.target)) return;

		const chosen = opened.fields.find((entry) => entry.kind === wanted);
		if (!chosen || chosen.empty) return;
		event.preventDefault();
		copy(opened.id, chosen.name);
	}
</script>

<!-- The window losing focus may be the reader reaching for the lid, so what
     they were typing is told to Rust then rather than a moment later. -->
<svelte:window onkeydown={shortcut} onblur={() => void flush()} />

{#snippet deletedFolders()}
	<BinFolders {folders} {root} {now} compact={pane !== null} onOpen={choose} />
{/snippet}

<div class="relative flex flex-1 animate-fade flex-col overflow-hidden">
	<!--
		Inert while the settings are over it, and not merely covered.

		An opaque sheet hides a pane; it does not take it out of the tab order,
		out of hit testing or out of the accessibility tree. Two presses of Tab
		used to land in the covered entry, where every field is a live input that
		commits what is in it the moment focus leaves - so keys aimed at the
		settings rewrote a field of an entry nobody could see, and the next press
		on the settings screen saved it to the file. `inert` is the one attribute
		that answers all three at once.
	-->
	<!--
		Three tracks whether or not an entry is open, and the third one is what
		opens and closes.

		The pane used to be a column that existed or did not, so opening an entry
		was the whole screen relaid out between two frames: the folders narrowed,
		the list narrowed, and a pane appeared in the gap, all at once and with
		nothing to follow. A track can be animated where a column that is not
		there cannot, so the closed state is the same three tracks with the last
		one at nothing, and the width is what moves.

		The pane inside keeps its own three hundred and eighty-four pixels and is
		clipped by the track, so the entry is never laid out at a width nobody
		asked for on its way in.
	-->
	<div
		class="grid flex-1 overflow-hidden transition-[grid-template-columns] duration-200 ease-out {pane
			? 'grid-cols-[200px_minmax(230px,1fr)_384px]'
			: 'grid-cols-[228px_minmax(230px,1fr)_0px]'}"
		inert={settings !== undefined}
	>
		<aside class="flex flex-col overflow-hidden border-r border-hairline bg-surface2">
			<div class="flex shrink-0 items-center gap-1 px-4 py-3">
				<span class="flex-1 font-mono text-label tracking-label text-txt3 uppercase">Folders</span>
				{#if !readOnly && !binned}
					{#if group !== null}
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
					{#if group !== null}
						<!-- Last, in a box of its own and a step further off, like every
						     other trash: it used to sit four pixels between Rename and New
						     folder at the size of each. The box is drawn into the row's
						     padding, so the header is no taller and the icon no further
						     in. -->
						<button
							type="button"
							onclick={() => (deleting = true)}
							class="-my-1.5 -mr-1.5 ml-2 flex h-7 w-7 shrink-0 items-center justify-center rounded-sm text-txt4 transition-colors hover:bg-dangerwash hover:text-danger"
							aria-label="Delete this folder"
						>
							<Icon name="trash" class="h-4 w-4" />
						</button>
					{/if}
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
						if (event.key === 'Enter' && !composing(event)) {
							event.preventDefault();
							makeFolder();
						}
					}}
					class="mx-2 mb-2 shrink-0 animate-rise rounded-sm border border-accent bg-surface px-2 py-1.5 text-body text-txt ring-4 ring-accent/15 outline-none placeholder:text-txt4"
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
						if (event.key === 'Enter' && !composing(event)) {
							event.preventDefault();
							rename(event.currentTarget.value);
						}
					}}
					class="mx-2 mb-2 shrink-0 animate-rise rounded-sm border border-accent bg-surface px-2 py-1.5 text-body text-txt ring-4 ring-accent/15 outline-none"
				/>
			{/if}

			{#if deleting && group !== null}
				<!-- Named by what it does, which Rust has already said: a folder
				     the bin is inside, or one in a vault that keeps no bin, goes
				     for good. -->
				<Confirm
					class="mx-2 mb-2 shrink-0 bg-surface"
					question={shown.deletion === 'bin'
						? `Move ${quoted(shown.name)} and everything in it to the Recycle Bin?`
						: eraseQuestion(quoted(shown.name), true)}
					act={shown.deletion === 'bin' ? 'Move to Recycle Bin' : 'Delete forever'}
					onKeep={() => (deleting = false)}
					onAct={removeFolder}
				/>
			{/if}

			<!--
			The empty part under the folders puts the open entry away. A press that
			landed on a folder is a press on that folder and arrives here on its way
			up, which is what `currentTarget` tells the two of them apart by.
		-->
			<div
				role="presentation"
				onclick={(event) => event.target === event.currentTarget && dismiss()}
				class="flex-1 overflow-y-auto px-2 pb-2 text-body"
			>
				<button
					type="button"
					onclick={() => choose(null)}
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
					onSelect={choose}
					onToggle={(id) => (expanded.has(id) ? expanded.delete(id) : expanded.add(id))}
				/>
			</div>

			{#if bin}
				{@const deleted = bin}
				<div class="shrink-0 border-t border-hairline px-2 py-2">
					<button
						type="button"
						onclick={() => choose(deleted.id)}
						class="relative flex w-full items-center gap-2 rounded-sm px-2 py-[7px] text-body transition-colors {binned
							? 'bg-raised text-txt'
							: 'text-txt3 hover:bg-raised/60 hover:text-txt2'}"
					>
						{#if binned}
							<span
								class="absolute top-1 left-0 h-[calc(100%-8px)] w-[2px] rounded-full bg-accent"
								aria-hidden="true"
							></span>
						{/if}
						<Icon name="trash" class="h-4 w-4 shrink-0" />
						<span class="flex-1 text-left">{deleted.name}</span>
						<span class="font-mono text-meta text-txt4">{entriesOf(deleted).length}</span>
					</button>

					{#if group === deleted.id && !readOnly && (deleted.entries.length > 0 || deleted.sections.length > 0)}
						{#if emptying}
							<Confirm
								class="mt-2 bg-surface"
								question="Delete everything in the bin forever? Nothing in it can be put back afterwards, and this can’t be undone."
								act="Empty it"
								onKeep={() => (emptying = false)}
								onAct={empty}
							/>
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

		<div class="flex flex-col overflow-hidden {pane ? 'border-r border-hairline' : ''}">
			<div class="flex shrink-0 items-center gap-3 border-b border-hairline px-5 py-3">
				<span
					class="flex flex-1 items-center gap-2 rounded-sm border border-hairline bg-surface2 px-3 py-2 transition focus-within:border-accent focus-within:ring-4 focus-within:ring-accent/15"
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
				{#if !readOnly && !binned}
					<button
						type="button"
						onclick={addEntry}
						class="flex h-9 shrink-0 items-center gap-2 rounded-full border border-hairline px-4 text-small text-txt2 transition-colors hover:border-txt4 hover:text-txt active:bg-raised"
					>
						<Icon name="plus" class="h-4 w-4" /> Entry
					</button>
				{/if}
			</div>

			{#if binned && shown.binned}
				{#key shown.id}
					<InBin
						class="mx-5 mt-3 shrink-0"
						binned={shown.binned}
						{root}
						{now}
						name={shown.name}
						{readOnly}
						ways={{
							question: eraseQuestion(quoted(shown.name), true),
							onPutBack: () => void moves.putBackFolder(shown.id),
							onDelete: eraseFolder
						}}
					/>
				{/key}
			{/if}

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
					detail="Entries added in Coffer, or in any other app that opens this file, show up here."
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
			{:else if found.length === 0 && folders.length === 0 && binned}
				{#if shown.isRecycleBin}
					<Empty
						icon="trash"
						title="The recycle bin is empty"
						detail="Anything moved here waits until it is put back or deleted forever."
					/>
				{:else}
					<Empty
						icon="folder"
						title="There is nothing in {quoted(shown.name)}"
						detail="It is in the recycle bin with nothing left inside it."
					/>
				{/if}
			{:else if found.length === 0 && !binned}
				<Empty
					icon="folder"
					title="There is nothing in {quoted(shown.name)} yet"
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
			{:else if pane}
				<EntryListCompact
					rows={found}
					open={pane.id}
					note={whence}
					before={folders.length > 0 ? deletedFolders : undefined}
					onOpen={open}
					onDismiss={dismiss}
				/>
			{:else}
				<EntryList
					rows={found}
					{now}
					note={whence}
					before={folders.length > 0 ? deletedFolders : undefined}
					onOpen={open}
					onCopy={copyFrom}
				/>
			{/if}
		</div>

		<div class="overflow-hidden">
			{#if pane}
				<!--
					The pane fades in when it opens, and not again when it moves from
					one entry to the next or from the row to the entry it was waiting
					for: the name and the login are already on the screen by then, and
					only what was missing arrives.

					Another entry is a pane with nothing read, so the entry that was
					drawn gives way to the row at once, and its questions, its revealed
					values and its versions go with it rather than waiting for the next
					entry to be read.
				-->
				<div class="h-full w-[384px] animate-fade">
					{#if pane.entry}
						<EntryView
							entry={pane.entry}
							{root}
							{path}
							history={pane.history}
							{now}
							{readOnly}
							onCopy={copy}
							onChanged={changed}
							onVersions={versionsChanged}
							onClose={dismiss}
							onDelete={removeEntry}
							onPutBack={putBack}
							onFieldRemoved={fieldRemoved}
							onFailure={failed}
						/>
					{:else}
						<Opening row={pane.row} {root} {path} {now} {readOnly} onClose={dismiss} />
					{/if}
				</div>
			{/if}
		</div>
	</div>

	{#if settings}
		<!-- Over the panes and not over the status bar, which is where the button
		     that opened this is and where it stays. -->
		<div class="absolute inset-0 z-20 flex animate-fade flex-col overflow-hidden bg-surface">
			{@render settings()}
		</div>
	{/if}

	{#if file.conflict}
		<Conflict
			rival={file.conflict}
			missing={file.missing}
			entries={entriesOf(root).length}
			changedAt={file.changedAt}
			{now}
			busy={file.saving}
			onReload={() => file.takeTheirs()}
			onCopy={() => file.keepBoth()}
			onOverwrite={() => file.keepOurs()}
		/>
	{/if}

	<!-- Always in the document, so that what arrives in it is read out: a
	     region that comes into being with its words already in it is one a
	     screen reader may never announce. Polite, because nothing here is worth
	     interrupting a sentence for. -->
	<div role="status" aria-live="polite" aria-atomic="true">
		{#if notices.notice}
			{#key notices.told}
				<Toast
					message={notices.notice.message}
					kind={notices.notice.kind}
					leaving={notices.leaving}
					onUndo={notices.notice.undo}
				/>
			{/key}
		{/if}
	</div>
</div>

<div
	class="flex shrink-0 items-center gap-4 border-t border-hairline bg-surface2 px-4 py-2 font-mono text-label tracking-label text-txt4 uppercase"
>
	<span>
		{found.length}
		{found.length === 1 ? 'entry' : 'entries'} here · {live.length} in the vault
	</span>
	<span class="truncate">{database.path}</span>

	<!-- One group, because two `ml-auto` siblings in a flex row do not both push
	     right. -->
	<span class="ml-auto flex shrink-0 items-center gap-4">
		{#if file.saving}
			<span class="text-txt3">Saving…</span>
		{:else if file.unsaved}
			<!-- A state design.html does not draw, built from its own tokens: the
			     mockup has no vault whose writes are failing, and a reader with
			     one has to be told for as long as it is true. -->
			<span class="text-warn">Not saved</span>
		{:else if readOnly}
			<span class="text-txt3">Read only</span>
		{/if}
		<!-- The settings go over the pane, which is the pane gone from where the
		     reader can answer a question in it. -->
		<button
			type="button"
			onclick={() => (settings || !held()) && onSettings()}
			aria-label={settings ? 'Back to the vault' : 'Settings'}
			aria-expanded={settings !== undefined}
			class="flex items-center gap-2 tracking-label uppercase transition-colors active:text-txt4 {settings
				? 'text-txt2'
				: 'hover:text-txt2'}"
		>
			<Icon name={settings ? 'x' : 'sliders'} class="h-3.5 w-3.5" />
			Settings
		</button>
	</span>
</div>
