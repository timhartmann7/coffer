<script lang="ts">
	import type { Snippet } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import {
		asFailure,
		beforeRemoval,
		copy as copyToClipboard,
		copyVersion,
		createEntry,
		createGroup,
		deleteEntry,
		deleteGroup,
		emptyRecycleBin,
		entry as loadEntry,
		putBackEntry,
		putBackGroup,
		reload,
		renameGroup,
		restoreVersion,
		rival,
		save,
		saveCopy,
		saveOver,
		tree as loadTree,
		versions as loadVersions
	} from '$lib/ipc';
	import { deleted as deletedLine } from '$lib/bin';
	import { flush, release } from '$lib/drafts';
	import { named as howLong } from '$lib/duration';
	import { called } from '$lib/format';
	import { copying, typing } from '$lib/keys';
	import { RISE, span } from '$lib/motion';
	import type { Database, Entry, EntryRow, Group, Rival, Span, Version } from '$lib/model';
	import { index, search } from '$lib/search';
	import { entriesOf, find, inBin, liveEntries, pathTo, recycleBin, shownEntries } from '$lib/tree';
	import BinFolders from './BinFolders.svelte';
	import Confirm from './Confirm.svelte';
	import Conflict from './Conflict.svelte';
	import Empty from './Empty.svelte';
	import EntryList from './EntryList.svelte';
	import EntryListCompact from './EntryListCompact.svelte';
	import EntryView from './EntryView.svelte';
	import Icon from './Icon.svelte';
	import InBin from './InBin.svelte';
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
	let opened = $state<Entry | null>(null);
	let versions = $state<Version[]>([]);
	let field = $state<HTMLInputElement>();
	let naming = $state(false);
	let named = $state<HTMLInputElement>();
	let renaming = $state(false);
	let emptying = $state(false);
	let deleting = $state(false);
	let saving = $state(false);
	/**
	 * Whether the vault is holding a change the file has not got.
	 *
	 * A save refused with anything but a conflict raises a notice that fades
	 * after six seconds, and a reader who missed it goes on editing into a
	 * window whose every write is failing. This is the standing version of that
	 * notice, and the status bar is where it sits.
	 */
	let unsaved = $state(false);
	let conflict = $state<Rival | null>(null);
	/** Whether the dialog is about a file somebody rewrote or one that is not
	 * there any more. The two ask different questions and offer different ways
	 * out: nothing can be reloaded from a file that is gone. */
	let missing = $state(false);
	let changedAt = $state<Date | null>(null);

	type Notice = {
		message: string;
		kind: 'copied' | 'failed' | 'removed';
		/** Takes back what the notice is about, while that is still on offer. */
		undo?: () => void;
	};

	/** Something the reader just did, and the way to take it back. */
	type Offer = {
		run: () => Promise<void>;
		/** The entry in the pane when the offer was made, or `null` for none. */
		entry: string | null;
	};

	/**
	 * How long a change can be taken back from the notice that reports it.
	 *
	 * Longer than a notice that only reports, because this one asks for a
	 * decision, and the pointer has the width of the window to cross to reach
	 * it.
	 */
	const UNDOABLE = 8000;

	let notice = $state<Notice | null>(null);
	/**
	 * What the notice on the screen offers to take back, while it still can.
	 *
	 * Nothing is drawn from it, so it is not state: it is the answer to whether
	 * the offer still stands, asked by the notice's button, by Cmd+Z and by
	 * everything that withdraws it. An undo clears it before its first wait, so
	 * the button and the key pressed together run it once.
	 */
	let offered: Offer | null = null;
	/** The toast's own clock, counting the seconds the clipboard still holds a
	 * copied value. Not the vault's: that one is Rust's and arrives as a prop. */
	let ticking: ReturnType<typeof setInterval> | null = null;
	let fading: ReturnType<typeof setTimeout> | null = null;
	/** Whether the notice on the screen is on its way out. Nothing animates an
	 * element that has already gone, so it says so first and goes after. */
	let leaving = $state(false);
	/** Which notice the toast is. Each is drawn afresh, so that a new sentence
	 * rises into the corner rather than changing its words in place. */
	let told = $state(0);

	// Timestamps are written against the moment the vault was opened rather than
	// against a clock that ticks, so that a list of a thousand rows is not
	// redrawn once a second to move one of them from "23:59" to "yesterday".
	const now = new Date();

	const shown = $derived(group === null ? root : (find(root, group) ?? root));
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
	/** Whether the list is showing the recycle bin or a folder inside it, where
	 * nothing is made or changed and everything is read one folder at a time. */
	const binned = $derived(group !== null && inBin(shown));
	/** The folders drawn above the entries in the bin. A search is for entries,
	 * and a folder row among its answers would be one it did not look inside. */
	const folders = $derived(binned && query === '' ? shown.sections : []);
	/** The line under a row in the bin: when it went in, and where from. */
	const whence = $derived(
		binned ? (row: EntryRow) => (row.binned ? deletedLine(row.binned, root, now) : '') : undefined
	);

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
	 * else wrote in the meantime stops here and asks, and so does one that is not
	 * there any more.
	 *
	 * Both have to ask rather than report. A save that only raised a notice left
	 * the reader editing into a window whose every write failed - a renamed file,
	 * an unmounted disk - with a whole session's work in memory and nothing in
	 * the application able to put it anywhere.
	 */
	async function persist() {
		// Any change that reaches the file is a newer thing than whatever the
		// notice offers to take back, and an undo from before it would take that
		// back too.
		retire();
		saving = true;
		try {
			await save();
			unsaved = false;
		} catch (thrown) {
			unsaved = true;
			const refused = asFailure(thrown);
			if (refused.code === 'externalChange' || refused.code === 'gone') {
				missing = refused.code === 'gone';
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

	/**
	 * Whether a move into the bin, out of it or out of the file is on its way.
	 *
	 * The button that asked for it stays on the screen until Rust answers, and
	 * a second press of "Move to Recycle Bin" in that second would find the
	 * entry already in the bin and take it out of the file. Nothing is drawn
	 * from it, so it is not state.
	 */
	let moving = false;

	/** Runs one move at a time, and drops a press that arrives during one. */
	async function once(move: () => Promise<void>) {
		if (moving) return;
		moving = true;
		try {
			await move();
		} finally {
			moving = false;
		}
	}

	/**
	 * Deletes the entry in the pane, and says what became of it.
	 *
	 * What became of it is read off the tree that comes back, not off what the
	 * pane expected: an entry still in the file went to the bin and is offered
	 * back, and one that is not went for good. A move whose save failed has a
	 * notice of its own already, and an offer over it would push the one
	 * sentence that matters off the screen.
	 */
	async function removeEntry() {
		if (!opened) return;
		const id = opened.id;
		const name = called(opened);
		let tree: Group;
		try {
			tree = await deleteEntry(id, release(id));
		} catch (thrown) {
			failed(thrown);
			return;
		}
		opened = null;
		versions = [];
		await reshaped(tree);
		if (unsaved) return;

		if (!entriesOf(tree).some((row) => row.id === id)) {
			tell({ message: `Deleted ${name} forever`, kind: 'removed' }, 6000);
			return;
		}
		offer(`Moved ${name} to the Recycle Bin`, async () => {
			await reshaped(await putBackEntry(id));
			await open(id);
		});
	}

	/** Takes the entry in the pane out of the bin. The pane stays on it and
	 * reads it again: back out of the bin it is an entry like any other, and the
	 * line above its title says where it went. */
	async function putBack() {
		if (!opened) return;
		const id = opened.id;
		try {
			await reshaped(await putBackEntry(id));
			await reread();
		} catch (thrown) {
			failed(thrown);
		}
	}

	/**
	 * Reads the entry in the pane again after a change that moved it without
	 * touching it.
	 *
	 * A folder put back takes the entries in it along, and an entry that was
	 * open inside it would otherwise go on being drawn read only, with a banner
	 * offering to put it back from a bin it has left.
	 */
	async function reread() {
		if (!opened) return;
		const id = opened.id;
		try {
			const fresh = await loadEntry(id);
			// The save before this takes a second, and a reader who opened
			// another entry in it is looking at that one now.
			if (opened?.id === id) opened = fresh;
		} catch (thrown) {
			if (opened?.id === id) opened = null;
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

	/** Deletes the folder being shown, after the question in the folders pane,
	 * and offers it back when it went to the bin. */
	async function removeFolder() {
		deleting = false;
		if (group === null) return;
		const id = group;
		const name = `“${shown.name}”`;
		try {
			const tree = await deleteGroup(id);
			select(null);
			await reshaped(tree);
			if (unsaved) return;
			if (find(tree, id) === null) {
				tell({ message: `Deleted ${name} forever`, kind: 'removed' }, 6000);
				return;
			}
			offer(`Moved ${name} to the Recycle Bin`, async () => {
				await reshaped(await putBackGroup(id));
			});
		} catch (thrown) {
			failed(thrown);
		}
	}

	/**
	 * Takes the folder being shown out of the bin with everything in it.
	 *
	 * The list stays on it, and the folders above it are opened in the tree, so
	 * that the place it went back to is on the screen rather than folded away.
	 */
	async function putBackFolder() {
		if (group === null) return;
		const id = group;
		try {
			const tree = await putBackGroup(id);
			for (const step of pathTo(tree, id)?.slice(0, -1) ?? []) expanded.add(step.id);
			await reshaped(tree);
			await reread();
		} catch (thrown) {
			failed(thrown);
		}
	}

	/** Deletes the folder being shown in the bin for good, and goes up to the
	 * folder it was in. */
	async function eraseFolder() {
		if (group === null) return;
		const id = group;
		const name = `“${shown.name}”`;
		const above = pathTo(root, id)?.at(-2)?.id ?? null;
		try {
			const tree = await deleteGroup(id);
			select(above);
			await reshaped(tree);
			if (!unsaved) tell({ message: `Deleted ${name} forever`, kind: 'removed' }, 6000);
		} catch (thrown) {
			failed(thrown);
		}
	}

	/**
	 * Empties the bin, and says what to do about anything that would not go.
	 *
	 * A file that a previous version of some other entry names has to keep the
	 * number it has, and the pool of files has to stay an unbroken run from
	 * zero, so now and then an entry cannot be erased until those versions go.
	 * The message has to name the way out, because nothing on this screen shows
	 * which entry is in the way: open the one still in the bin and take its file
	 * off, which clears the versions holding it wherever they are.
	 *
	 * Whatever could go has gone by the time this is read, so pressing it again
	 * after that is a shorter list every time and never a longer one.
	 */
	async function empty() {
		emptying = false;
		try {
			const tree = await emptyRecycleBin();
			opened = null;
			await reshaped(tree);
		} catch (thrown) {
			if (asFailure(thrown).code === 'attachmentInHistory') {
				// The refusal is about what stayed, not about what went: emptying
				// the bin is all-or-nothing per entry and the ones that could go
				// are already out of the vault in memory. So this is read back and
				// written like any other change - a screen that only reported the
				// refusal drew a bin that was emptier than the file, and lost the
				// erasures at the next lock.
				opened = null;
				await reshaped(await loadTree().catch(() => root));
				warn('Some of it stayed: open what is left in the bin and remove its file first.');
			} else {
				failed(thrown);
			}
		}
	}

	async function takeTheirs() {
		try {
			saving = true;
			const tree = await reload(release(null));
			onTree(tree);
			opened = null;
			versions = [];
			changedAt = null;
			conflict = null;
			missing = false;
			// Reading the file again is throwing the change away, which is a
			// thing the reader chose. There is nothing left unsaved either way.
			unsaved = false;
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
			tell({ message: `Kept as ${beside.name}`, kind: 'copied' }, 6000);
			// Only where there is a file to take instead. When the vault itself
			// is gone there is nothing to read back, and the window goes on
			// holding the version the copy was made from - which is still the
			// only one, and can still be written back where it belongs.
			if (!missing) await takeTheirs();
			missing = false;
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
			missing = false;
			unsaved = false;
		} catch (thrown) {
			failed(thrown);
		} finally {
			saving = false;
		}
	}

	/**
	 * Copies a value in Rust and says so: a field of an entry, or of one of its
	 * previous versions, whole or the part of it the reader selected. Every copy
	 * in the window comes through here, so every one of them gets the same
	 * concealed, self-clearing pasteboard write and the same notice.
	 */
	async function copy(entry: string, name: string, range: Span | null = null, version?: number) {
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
		tell({ message: `Copied. The clipboard clears in ${howLong(seconds)}.`, kind: 'copied' }, 5000);
	}

	function warn(message: string) {
		tell({ message, kind: 'failed' }, 6000);
	}

	function failed(thrown: unknown) {
		warn(asFailure(thrown).message);
	}

	/**
	 * Puts a sentence on the screen and takes it away again.
	 *
	 * The clock of whatever notice was there is stopped first: without that, a
	 * timer left over from the last one takes this one away early, and the class
	 * that was fading it out arrives already on it. Whatever that notice offered
	 * to take back goes with it, because an undo belongs to the sentence that
	 * says what it undoes.
	 */
	function tell(next: Notice, after: number) {
		clear();
		told += 1;
		notice = next;
		fade(after);
	}

	/**
	 * Says what was just done and offers to take it back, for eight seconds.
	 *
	 * The offer is withdrawn by whatever makes it stale: the notice going, a
	 * newer notice, another change reaching the file, and the pane showing
	 * another entry or none - an undo is of something the reader can still see,
	 * and Cmd+Z in another entry would reach back into one they have left. A
	 * lock takes the whole window down, and the offer with it.
	 */
	function offer(message: string, undo: () => Promise<void>) {
		const made: Offer = { run: undo, entry: opened?.id ?? null };
		tell({ message, kind: 'removed', undo: () => void takeBack(made) }, UNDOABLE);
		offered = made;
	}

	/** Runs an undo once, and only while it is still the one on offer. */
	async function takeBack(which: Offer) {
		if (offered !== which) return;
		retire();
		try {
			await which.run();
		} catch (thrown) {
			failed(thrown);
		}
	}

	/**
	 * Withdraws the offer, and the notice making it.
	 *
	 * The notice goes too rather than staying without its button: a sentence
	 * that ended in "Undo" a moment ago and no longer does reads as though the
	 * press had been taken.
	 */
	function retire() {
		if (offered === null) return;
		clear();
		go();
	}

	/**
	 * Takes the notice away, once it has been read and once it has finished
	 * going.
	 *
	 * Two waits rather than one: the first is how long the sentence is worth
	 * reading, the second is the length of the movement that takes it off the
	 * screen, and `motion.test.ts` is what keeps that second one and the
	 * stylesheet saying the same number.
	 */
	function fade(after: number) {
		fading = setTimeout(go, after);
	}

	/** The second of the two waits. A notice that has started going offers
	 * nothing any more, whatever it said. */
	function go() {
		offered = null;
		leaving = true;
		fading = setTimeout(() => {
			notice = null;
			leaving = false;
			fading = null;
		}, span(RISE));
	}

	function clear() {
		offered = null;
		if (ticking !== null) {
			clearInterval(ticking);
			ticking = null;
		}
		if (fading !== null) {
			clearTimeout(fading);
			fading = null;
		}
		// A notice that is replaced while it is going arrives fully faded out
		// otherwise, because the class that is taking it away is still on it.
		leaving = false;
	}

	$effect(() => () => clear());

	const showing = $derived(opened?.id ?? null);

	// The entry in the pane is read before anything else, whether or not an
	// offer stands: an effect only runs again for what it read, and one that
	// looked at the offer first and found none would never look at the pane.
	$effect(() => {
		const pane = showing;
		if (offered !== null && offered.entry !== pane) retire();
	});

	/**
	 * A field of the reader's own came off the open entry.
	 *
	 * It is offered back only once the removal is in the file, and only when
	 * Rust names the version that holds it: the save may have pruned that
	 * version, and whatever is newest after that is older, and its restore
	 * would take back more than the field. A removal the save refused has a
	 * notice of its own already, and the standing "Not saved" beside it, and an
	 * offer over that notice would push the one sentence that matters off the
	 * screen.
	 *
	 * The undo asks Rust again rather than keeping the answer: a position is an
	 * answer about the history as it stood when it was given.
	 */
	async function fieldRemoved(entry: string, name: string) {
		if (unsaved) return;
		const said = `Field “${name}” removed`;
		let holding: number | null;
		try {
			holding = await beforeRemoval(entry, name);
		} catch (thrown) {
			failed(thrown);
			return;
		}
		if (opened?.id !== entry) return;
		if (holding === null) {
			tell({ message: said, kind: 'removed' }, 6000);
			return;
		}
		offer(said, async () => {
			const still = await beforeRemoval(entry, name);
			if (still === null) {
				warn('The entry has changed since, so that can no longer be undone.');
				return;
			}
			await changed(await restoreVersion(entry, still));
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

		// A field's own undo is the field's. Cmd+Z in the search box or in a
		// value being written takes back typing, and the offer waits for a key
		// that is not aimed at one.
		if (event.key.toLowerCase() === 'z' && !event.shiftKey) {
			if (offered === null || typing(event.target)) return;
			event.preventDefault();
			void takeBack(offered);
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
	<BinFolders {folders} {root} {now} compact={opened !== null} onOpen={select} />
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
		class="grid flex-1 overflow-hidden transition-[grid-template-columns] duration-200 ease-out {opened
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
						if (event.key === 'Enter') {
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
						? `Move “${shown.name}” and everything in it to the Recycle Bin?`
						: `Delete “${shown.name}” and everything in it forever? This can’t be undone.`}
					act={shown.deletion === 'bin' ? 'Move to Recycle Bin' : 'Delete forever'}
					onKeep={() => (deleting = false)}
					onAct={() => void once(removeFolder)}
				/>
			{/if}

			<!--
			The empty part under the folders puts the open entry away. A press that
			landed on a folder is a press on that folder and arrives here on its way
			up, which is what `currentTarget` tells the two of them apart by.
		-->
			<div
				role="presentation"
				onclick={(event) => event.target === event.currentTarget && (opened = null)}
				class="flex-1 overflow-y-auto px-2 pb-2 text-body"
			>
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

		<div class="flex flex-col overflow-hidden {opened ? 'border-r border-hairline' : ''}">
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
						question="Delete “{shown.name}” and everything in it forever? This can’t be undone."
						{readOnly}
						onPutBack={() => void once(putBackFolder)}
						onDelete={() => void once(eraseFolder)}
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
						title="There is nothing in “{shown.name}”"
						detail="It is in the recycle bin with nothing left inside it."
					/>
				{/if}
			{:else if found.length === 0 && !binned}
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
				<EntryListCompact
					rows={found}
					open={opened.id}
					note={whence}
					before={folders.length > 0 ? deletedFolders : undefined}
					onOpen={open}
					onDismiss={() => (opened = null)}
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
			{#if opened}
				<div class="h-full w-[384px]">
					<EntryView
						entry={opened}
						{root}
						{path}
						{versions}
						{now}
						{readOnly}
						onCopy={copy}
						onChanged={changed}
						onVersions={versionsChanged}
						onClose={() => (opened = null)}
						onDelete={() => void once(removeEntry)}
						onPutBack={() => void once(putBack)}
						onFieldRemoved={(entry, name) => void fieldRemoved(entry, name)}
						onFailure={failed}
					/>
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

	{#if conflict}
		<Conflict
			rival={conflict}
			{missing}
			entries={entriesOf(root).length}
			{changedAt}
			{now}
			busy={saving}
			onReload={takeTheirs}
			onCopy={keepBoth}
			onOverwrite={keepOurs}
		/>
	{/if}

	<!-- Always in the document, so that what arrives in it is read out: a
	     region that comes into being with its words already in it is one a
	     screen reader may never announce. Polite, because nothing here is worth
	     interrupting a sentence for. -->
	<div role="status" aria-live="polite" aria-atomic="true">
		{#if notice}
			{#key told}
				<Toast message={notice.message} kind={notice.kind} {leaving} onUndo={notice.undo} />
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
		{#if saving}
			<span class="text-txt3">Saving…</span>
		{:else if unsaved}
			<!-- A state design.html does not draw, built from its own tokens: the
			     mockup has no vault whose writes are failing, and a reader with
			     one has to be told for as long as it is true. -->
			<span class="text-warn">Not saved</span>
		{:else if readOnly}
			<span class="text-txt3">Read only</span>
		{/if}
		<button
			type="button"
			onclick={onSettings}
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
