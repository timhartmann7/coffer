<script lang="ts">
	import type { Attachment } from 'svelte/attachments';
	import { SvelteSet } from 'svelte/reactivity';
	import { settle, type Place } from '$lib/drafts';
	import { called, fully, size } from '$lib/format';
	import {
		addAttachment,
		asFailure,
		exportAttachment,
		keepBothAttachments,
		openUrl,
		removeAttachment,
		removeAttachmentAndVersions,
		removeField,
		replaceAttachment,
		setField,
		setTags,
		withdrawAttachment
	} from '$lib/ipc';
	import { composing } from '$lib/lines';
	import type { Clash, Entry, Field, Group, History, Position, Span } from '$lib/model';
	import Change from './Change.svelte';
	import Confirm from './Confirm.svelte';
	import Editable from './Editable.svelte';
	import Heading from './Heading.svelte';
	import Icon from './Icon.svelte';
	import InBin from './InBin.svelte';
	import Mask from './Mask.svelte';
	import PasswordField from './PasswordField.svelte';
	import ProtectedValue from './ProtectedValue.svelte';
	import Tags from './Tags.svelte';
	import Versions from './Versions.svelte';

	/**
	 * One entry, edited where it stands.
	 *
	 * Every change goes to Rust and comes back as the entry Rust now holds, so
	 * the screen never draws a picture it assembled itself. Failures go up to
	 * the window, which is the one place that turns one into something a reader
	 * can read.
	 */
	let {
		entry,
		root,
		path,
		history,
		now,
		readOnly,
		onCopy,
		onChanged,
		onVersions,
		onClose,
		onDelete,
		onPutBack,
		onFieldRemoved,
		onFailure
	}: {
		entry: Entry;
		/** The whole tree, which is where an entry in the bin finds the name of
		 * the folder it came from. */
		root: Group;
		/** The folders from the vault down to this entry, for the line above the
		 * title. */
		path: Group[];
		/** The entry's previous versions, once Rust has listed them. A list of
		 * another entry's is not this entry's history and is never drawn. */
		history: History | null;
		now: Date;
		/** A database Coffer will not write back. Nothing here offers a change
		 * that would only be refused. */
		readOnly: boolean;
		/** Copies a value in Rust: a field of the entry, or of one of its
		 * versions, whole or the part of it the reader selected. */
		onCopy: (entry: string, field: string, range?: Span | null, version?: Position) => void;
		onChanged: (entry: Entry) => Promise<void>;
		onVersions: (history: History) => Promise<void>;
		/** Puts the pane away. Escape does the same, and so does a press on the
		 * empty part of either pane to the left of this one. */
		onClose: () => void;
		/** Deletes the entry: to the recycle bin, or for good once the reader has
		 * said so, which is `entry.deletion`'s to decide and the pane's to ask. */
		onDelete: () => void;
		/** Takes the entry out of the recycle bin. */
		onPutBack: () => void;
		/** One of the reader's own fields came off, and the change has been
		 * handed to the window like any other. The window is what can offer it
		 * back, because it is what knows whether the change reached the file.
		 * `forever` is a removal the reader agreed nothing could take back. */
		onFieldRemoved: (entry: string, field: string, forever: boolean) => void;
		onFailure: (thrown: unknown) => void;
	} = $props();

	const of = (kind: Field['kind']) => entry.fields.find((field) => field.kind === kind);

	/**
	 * Whether nothing here may be changed: a database Coffer will not write
	 * back, or an entry in the recycle bin. What is in the bin is there to be
	 * put back or let go, and an edit to it would be a change nobody sees until
	 * it comes back.
	 */
	const locked = $derived(readOnly || entry.binned !== null);
	/** Whether the reader has asked to delete the entry for good and not yet
	 * answered the question that asks whether they mean it. */
	let erasing = $state(false);
	const forever = $derived(`Delete ${called(entry)} forever? This can’t be undone.`);

	const title = $derived(of('title'));
	const username = $derived(of('username'));
	const password = $derived(of('password'));
	const url = $derived(of('url'));
	const notes = $derived(of('notes'));
	const custom = $derived(entry.fields.filter((field) => field.kind === 'custom'));

	let naming = $state(false);
	let named = $state<HTMLInputElement>();
	/** Whether the field being named is to be written in lines. The format has
	 * no such flag - a value is in lines because it has a line break - so this
	 * decides only the field its first value is written in. */
	let inLines = $state(false);
	/** The field just made to be written in lines, until another entry is shown. */
	let longhand = $state<string | null>(null);
	/**
	 * The protected values a new one is being written for, by name.
	 *
	 * Each has a field of its own under its row, opened by its Change and closed
	 * by Save or Cancel. More than one at a time, because a field left with
	 * something typed in it asks what to do with it and waits for the answer.
	 */
	const changing = new SvelteSet<string>();
	/**
	 * A file the entry's own previous versions still hold, and the reason it
	 * cannot go yet. The format keeps those versions inside the entry and the
	 * library gives no way to rewrite one, so the only way the file can leave is
	 * for the versions holding it to leave first - which is the reader's call
	 * and nobody else's.
	 */
	let pinned = $state<{ name: string; message: string } | null>(null);
	/**
	 * A file the reader chose under a name the entry already gives another,
	 * while they say which of the two to keep.
	 *
	 * The file itself waits in Rust, so that an answer never means choosing it
	 * again; this is only what the question says about it. Nothing is on the
	 * entry until the answer is in, and a phone that calls every scan by the
	 * same name no longer takes the first page of a passport with the second.
	 */
	let clash = $state<Clash | null>(null);
	/** Whether the reader asked to replace the file there and earlier versions
	 * are holding it in place. The question is asked again without that answer,
	 * because keeping both needs nothing to go. */
	let held = $state(false);
	/** Whether the file the question is about has left the entry since it was
	 * asked: its own trash is what the question says to use when versions hold
	 * it. There is nothing left to keep both of or to replace, and the new file
	 * goes on under the name itself. */
	const freed = $derived(
		clash !== null && !entry.attachments.some((file) => file.name === clash?.name)
	);
	/**
	 * The file whose removal is being asked about.
	 *
	 * A file is the one thing on an entry that nothing brings back: the format
	 * keeps files out of an entry's versions, and the vault is written the
	 * moment it goes. So the press that removes one asks first, in the file's
	 * own row, and offers the export on the way.
	 */
	let asking = $state<string | null>(null);
	/**
	 * The field of the reader's own whose removal is being asked about.
	 *
	 * A removal is taken back from the version it writes, and a vault whose
	 * limits keep no such version (none kept at all, or a size it does not fit)
	 * loses the field at the next save. Rust refuses that removal until the
	 * reader has said so, and this is the question, in the field's own row.
	 */
	let dropping = $state<string | null>(null);

	/**
	 * Which entry the pane is showing.
	 *
	 * Its own value rather than a read of `entry`, which is a new object after
	 * every change: an effect that read the entry itself ran again for an edit
	 * to the title, and took the reader's unanswered question away with it.
	 */
	const showing = $derived(entry.id);

	/**
	 * The entry this pane stands on while it is on the screen, and `null` once
	 * it has gone or moved to another.
	 *
	 * What an answer that arrives late asks before it is given to anybody. The
	 * entry itself cannot say: a pane the window has taken down still holds the
	 * last entry it was given, and a question drawn there is one nobody will
	 * ever see. Nothing is drawn from it, so it is not state.
	 */
	let standing: string | null = null;

	// Another entry is another set of answers. A banner about a file on the
	// entry that was open would otherwise still be on the screen under the next
	// one, offering to clear the wrong entry's history.
	$effect(() => {
		const id = showing;
		standing = id;
		// Cleared on the way out rather than on the way in: a write inside the
		// body of an effect is a read of what was there, and an effect that
		// reads what it writes runs again the moment anything sets it.
		return () => {
			standing = null;
			// A question nobody can see any more is not one anybody will answer,
			// and the file it was about is the reader's, waiting in Rust. It goes
			// with the question, and Rust lets go of it only for this entry, so a
			// file chosen for the next one since is left alone.
			if (clash) letGo(id);
			pinned = null;
			clash = null;
			held = false;
			naming = false;
			inLines = false;
			longhand = null;
			changing.clear();
			asking = null;
			dropping = null;
			erasing = false;
		};
	});

	/** Runs a change and says whether it was taken. */
	async function change(run: () => Promise<Entry>): Promise<boolean> {
		try {
			await onChanged(await run());
			return true;
		} catch (thrown) {
			onFailure(thrown);
			return false;
		}
	}

	/** Writes a value into a field of this entry, finishes whatever was typed
	 * there, and hands the entry that comes back on. Rejects with what the
	 * vault said when it would not take the value. */
	async function put(field: string, value: string, protect: boolean): Promise<void> {
		const id = entry.id;
		await onChanged(
			await settle({ entry: id, field, protect }, (sequence) =>
				setField(id, field, value, protect, sequence)
			)
		);
	}

	/** The same for a value written where it stands, which is put back when it
	 * is refused: says whether it was taken, and why not when it was not. */
	function write(field: string, value: string, protect: boolean): Promise<boolean> {
		return put(field, value, protect).then(
			() => true,
			(thrown) => {
				onFailure(thrown);
				return false;
			}
		);
	}

	/**
	 * The field whose new value was just put away with the focus in it.
	 *
	 * Its Change comes back in place of the field, and takes the focus back as
	 * it is drawn, rather than the focus falling out of the pane - where the
	 * next Escape closes the entry. Nothing is drawn from it, so it is not
	 * state.
	 */
	let returning: string | null = null;

	/** A field's own Change field closed, with the focus in it or not. */
	function closed(name: string, back: boolean) {
		if (back) returning = name;
		changing.delete(name);
	}

	/** Gives the focus to a field's Change as it is drawn again, when its own
	 * field just closed with the focus in it. */
	function returned(name: string): Attachment<HTMLButtonElement> {
		return (button) => {
			if (returning !== name) return;
			returning = null;
			button.focus();
		};
	}

	/**
	 * Puts a file on the entry, or asks about the one already under its name.
	 *
	 * A new press is a new question, so whatever was being asked goes first;
	 * Rust lets go of the file it was about before the panel opens.
	 */
	async function attach() {
		const id = entry.id;
		clash = null;
		held = false;
		try {
			const answer = await addAttachment(id);
			if (answer === null) return;
			if (answer.outcome === 'added') {
				await onChanged(answer.entry);
				return;
			}
			// The pane moved on or went while the panel was open, so there is
			// nobody left to ask. The file is let go rather than kept for a
			// question that is never drawn.
			if (standing !== id) {
				letGo(id);
				return;
			}
			clash = answer.clash;
		} catch (thrown) {
			onFailure(thrown);
		}
	}

	/** The answer that loses nothing: the file waiting goes on beside the one
	 * already there. */
	async function keepBoth() {
		const id = entry.id;
		clash = null;
		held = false;
		// A refusal takes the question away, so it takes the file too: a file
		// waiting on a question nobody can see is one nobody can answer.
		if (!(await change(() => keepBothAttachments(id)))) letGo(id);
	}

	/**
	 * The destructive answer: the file waiting takes the place of the one there.
	 *
	 * Refused while earlier versions hold the one there, which is the reader's
	 * to act on rather than an error to read: the file is still waiting in Rust,
	 * so the question comes back without the answer that was refused.
	 */
	async function replace() {
		const id = entry.id;
		const asked = clash;
		clash = null;
		try {
			await onChanged(await replaceAttachment(id));
		} catch (thrown) {
			if (asFailure(thrown).code === 'attachmentInHistory' && standing === id) {
				clash = asked;
				held = true;
				return;
			}
			onFailure(thrown);
			letGo(id);
		}
	}

	/** What the question about a taken name says, as the entry stands now. */
	function said(offer: Clash): string {
		if (freed) {
			return `“${offer.name}” is no longer on this entry, so the new one (${size(offer.chosen)}) can go on under that name.`;
		}
		if (held) {
			return `Earlier versions are holding the “${offer.name}” that is here in place, so it can’t be replaced. Keep both, or remove that one first - its trash offers to clear the versions in the way.`;
		}
		return `This entry already has “${offer.name}” (${size(offer.size)}). Keep both to add the new one (${size(offer.chosen)}) as “${offer.free}”. A replaced file is not kept in Versions, so replacing can’t be undone.`;
	}

	/** The way out: the file waiting is let go, and the entry is as it was. */
	function dismiss() {
		clash = null;
		held = false;
		letGo(entry.id);
	}

	/**
	 * Tells Rust to let go of the file waiting on this entry.
	 *
	 * Nothing waits on the answer and nothing is said when it fails. The file
	 * goes anyway at the next pick and at the next lock, and a sentence about
	 * letting go of something the reader already said no to is not one worth
	 * putting in front of them.
	 */
	function letGo(id: string) {
		void withdrawAttachment(id).catch(() => {});
	}

	/** Writes a file out through the save panel Rust opens. */
	function writeOut(name: string) {
		exportAttachment(entry.id, name).catch(onFailure);
	}

	/** Takes a file off, or says why it cannot go yet. */
	async function detach(name: string) {
		asking = null;
		pinned = null;
		try {
			await onChanged(await removeAttachment(entry.id, name));
		} catch (thrown) {
			const failure = asFailure(thrown);
			if (failure.code === 'attachmentInHistory') {
				pinned = { name, message: failure.message };
			} else {
				onFailure(thrown);
			}
		}
	}

	/**
	 * What the reader asked for, once they have said the versions may go.
	 *
	 * The version list is not emptied here. Only the versions that were holding
	 * the file go, and which ones those were is the engine's answer rather than
	 * this pane's - `onChanged` reads the list back with the rest of the entry.
	 * Saying `[]` here drew an entry as having no history when it still had
	 * most of it, and wrote the whole vault a second time to say so.
	 */
	async function detachWithVersions() {
		const name = pinned?.name;
		pinned = null;
		if (name === undefined) return;
		try {
			await onChanged(await removeAttachmentAndVersions(entry.id, name));
		} catch (thrown) {
			onFailure(thrown);
		}
	}

	/**
	 * Takes one of the reader's own fields off, and lets the window say so.
	 *
	 * The first press never takes one for good: Rust answers `forGood` when
	 * nothing would bring the field back, and the row asks. `forever` is the
	 * answer to that question.
	 */
	async function dropField(name: string, forever = false) {
		const id = entry.id;
		dropping = null;
		try {
			await onChanged(await removeField(id, name, forever));
		} catch (thrown) {
			if (asFailure(thrown).code !== 'forGood') onFailure(thrown);
			// A question for a pane that has moved on is one nobody would see,
			// and the field is still there: nothing happened to say anything
			// about.
			else if (standing === id) dropping = name;
			return;
		}
		onFieldRemoved(id, name, forever);
	}

	/** The name a standard field carries in the file. An entry that arrived
	 * without one still has to be editable, so the format's own name is used. */
	function nameOf(field: Field | undefined, standard: string): string {
		return field?.name ?? standard;
	}

	/** Where a value typed into one of this entry's fields goes: the field's
	 * name in the file, and whether the file protects it. */
	function place(field: Field | undefined, standard: string): Place {
		return { entry: entry.id, field: nameOf(field, standard), protect: field?.protected ?? false };
	}

	/**
	 * Opens the field that asks for a name, and closes it again.
	 *
	 * The mouse press that reaches this button is stopped from moving the focus
	 * (see the markup), so the open field does not blur and write a name on the
	 * way out. Without that the second press would close the field and open a
	 * fresh one in the same breath, which is what it looked like from the
	 * outside.
	 */
	function addField() {
		naming = !naming;
		if (!naming) return;
		queueMicrotask(() => named?.focus());
	}

	/** The focus left the name and the option beside it, which is where the
	 * name is finished. Moving from one to the other is not leaving. */
	function leaving(event: FocusEvent) {
		const into = event.relatedTarget;
		const group = event.currentTarget;
		if (into instanceof Node && group instanceof Node && group.contains(into)) return;
		makeField();
	}

	function makeField() {
		if (!named) return;
		const name = named.value.trim();
		const lined = inLines;
		named.value = '';
		naming = false;
		inLines = false;
		if (name === '') return;

		// Writing a field is writing over whatever that name held, so a name the
		// entry already has is refused here rather than emptying a value the
		// reader was not thinking about. The password and the notes are fields
		// like any other by that name.
		if (entry.fields.some((field) => field.name === name)) {
			onFailure({
				code: 'refused',
				message: `this entry already has a field called “${name}”`
			});
			return;
		}

		// A new field of the reader's own is protected: a field somebody adds to
		// a password entry is far more often a secret than not, and the value is
		// shown behind an eye until they say otherwise.
		if (lined) longhand = name;
		void write(name, '', true);
	}
</script>

<!--
	The way to write a new value into a field the database protects, on the line
	under the field: a Change in words, and in its place, once pressed, a field
	of its own with Save and Cancel. A value in lines is replaced in lines, and so
	is one the reader asked to write in lines when they named the field, for as
	long as the entry is open.
-->
{#snippet changer(field: Field)}
	{#if !locked}
		{#if changing.has(field.name)}
			<Change
				class="mt-2 animate-rise"
				label="New value of {field.name}"
				placeholder="New value"
				what="value of “{field.name}”"
				multiline={field.lines || field.name === longhand}
				draft={place(field, field.name)}
				onSave={(value) => put(field.name, value, field.protected)}
				onClose={(back) => closed(field.name, back)}
				{onFailure}
			/>
		{:else}
			<button
				{@attach returned(field.name)}
				type="button"
				onclick={() => changing.add(field.name)}
				class="mt-1 text-fine text-txt3 transition-colors hover:text-txt"
				aria-label="Change {field.name}"
			>
				Change
			</button>
		{/if}
	{/if}
{/snippet}

<section class="flex h-full flex-col overflow-hidden bg-surface">
	<Heading {path} {onClose}>
		{#if title && title.value === null}
			<h1 class="min-w-0 flex-1 truncate text-title font-medium tracking-tight text-txt">
				<Mask />
			</h1>
		{:else}
			{@const at = place(title, 'Title')}
			<h1 class="-ml-2 flex min-w-0 flex-1">
				<Editable
					value={title?.value ?? ''}
					label="Title"
					placeholder="Untitled"
					classes="text-title font-medium tracking-tight text-txt"
					readonly={locked}
					bare
					draft={at}
					onCommit={(value) => write(at.field, value, at.protect)}
				/>
			</h1>
		{/if}
	</Heading>

	<div class="flex-1 overflow-y-auto px-6 py-5">
		{#if entry.binned}
			<!-- Keyed, so a question about deleting one entry for good is never
			     still open over the next one. -->
			{#key entry.id}
				<InBin
					class="mb-5"
					binned={entry.binned}
					{root}
					{now}
					question={forever}
					{readOnly}
					{onPutBack}
					{onDelete}
				/>
			{/key}
		{/if}

		<div class="block">
			<span class="font-mono text-label tracking-label text-txt3 uppercase">Login</span>
			<span class="mt-1.5 flex items-center gap-2">
				{#if username && username.value === null && !username.empty}
					<!-- A database may protect any field, the login included. Coffer
					     does not protect one, so this is a file from another client -
					     and a login is a standard field with no way to delete it, so
					     a row that only revealed left it impossible to change at all.
					     Its Change is on the line under it. -->
					<ProtectedValue
						entry={entry.id}
						field={username.name}
						onCopy={(range) => onCopy(entry.id, username.name, range)}
						{onFailure}
					/>
				{:else}
					{@const at = place(username, 'UserName')}
					<Editable
						value={username?.value ?? ''}
						label="Login"
						placeholder="No login"
						mono
						readonly={locked}
						draft={at}
						onCommit={(value) => write(at.field, value, at.protect)}
					/>
				{/if}
				<!-- Not for a protected login: the row above draws its own, beside
				     the eye that reveals it. -->
				{#if username && !username.empty && username.value !== null}
					<button
						type="button"
						onclick={() => onCopy(entry.id, username.name)}
						class="shrink-0 text-txt4 transition-colors hover:text-txt2"
						aria-label="Copy login"
					>
						<Icon name="copy" class="h-4 w-4" />
					</button>
				{/if}
			</span>
			{#if username && username.value === null && !username.empty}
				{@render changer(username)}
			{/if}
		</div>

		<PasswordField
			entry={entry.id}
			field={nameOf(password, 'Password')}
			empty={password?.empty ?? true}
			lines={password?.lines ?? false}
			protect={password?.protected ?? true}
			readOnly={locked}
			onCopy={(field, range) => onCopy(entry.id, field, range)}
			onCommit={put}
			{onFailure}
		/>

		<div class="mt-5">
			<span class="font-mono text-label tracking-label text-txt3 uppercase">Address</span>
			{#if url && url.value === null && !url.empty}
				<!-- Coffer never protects an address, but a database from another
				     client may, and a protected one carries the same defect any
				     other protected field of the reader's does. There is no opener
				     beside it because `openable` is settled from the value, which
				     never leaves Rust for a protected field. -->
				<span class="mt-1.5 flex items-center gap-2">
					<ProtectedValue
						entry={entry.id}
						field={url.name}
						onCopy={(range) => onCopy(entry.id, url.name, range)}
						{onFailure}
					/>
				</span>
				{@render changer(url)}
			{:else}
				{@const at = place(url, 'URL')}
				<span class="mt-1.5 flex items-center gap-2">
					<Editable
						value={url?.value ?? ''}
						label="Address"
						placeholder="No address"
						readonly={locked}
						draft={at}
						onCommit={(value) => write(at.field, value, at.protect)}
					/>
					{#if url?.openable}
						<button
							type="button"
							onclick={() => openUrl(entry.id).catch(onFailure)}
							class="shrink-0 text-txt4 transition-colors hover:text-txt2"
							aria-label="Open this address"
						>
							<Icon name="export" class="h-4 w-4" />
						</button>
					{/if}
				</span>
			{/if}
		</div>

		<div class="mt-5">
			<span class="font-mono text-label tracking-label text-txt3 uppercase">Tags</span>
			<Tags
				tags={entry.tags}
				readOnly={locked}
				onSet={(tags) => void change(() => setTags(entry.id, tags))}
			/>
		</div>

		<div class="mt-6 border-t border-line pt-5">
			<div class="font-mono text-label tracking-label text-txt3 uppercase">Notes</div>
			{#if notes && notes.value === null && !notes.empty}
				<!-- The one protected value still only readable. Notes is the field
				     that is a textarea rather than a line, so it is not the row the
				     others share, and Coffer never protects one: this is a note a
				     foreign client protected, and it is revealed and copied rather
				     than written into. -->
				<div class="mt-3 flex items-center gap-3">
					<ProtectedValue
						entry={entry.id}
						field={notes.name}
						lines
						onCopy={(range) => onCopy(entry.id, notes.name, range)}
						{onFailure}
					/>
				</div>
			{:else}
				{@const at = place(notes, 'Notes')}
				<div class="mt-3 flex items-start">
					<Editable
						value={notes?.value ?? ''}
						label="Notes"
						placeholder="Nothing written down"
						multiline
						classes="text-small leading-relaxed text-txt2 focus:text-txt"
						readonly={locked}
						draft={at}
						onCommit={(value) => write(at.field, value, at.protect)}
					/>
				</div>
			{/if}
		</div>

		<div class="mt-6 border-t border-line pt-5">
			<div class="flex items-center justify-between">
				<span class="font-mono text-label tracking-label text-txt3 uppercase">Own fields</span>
				{#if !locked}
					<button
						type="button"
						onmousedown={(event) => event.preventDefault()}
						onclick={addField}
						class="text-txt4 transition-colors hover:text-txt2"
						aria-label={naming ? 'Never mind the new field' : 'Add a field'}
					>
						<Icon name="plus" class="h-4 w-4" />
					</button>
				{/if}
			</div>

			<!--
				The trash is the last thing in the row and is there only while the
				row is under the pointer or holds the focus, in a box the size of a
				fingertip and a step further off than the gap between the others.
				It used to sit twelve pixels from Copy at the same size, on every
				row, and a press meant for the value took the field.

				The name is a column of its own and the value another, and a
				protected value's Change goes under the value in the value's
				column. It starts where the value starts because it is in the same
				column, not because a margin was worked out to match the width of
				the name.
			-->
			{#each custom as field (field.name)}
				{@const masked = field.value === null && !field.empty}
				{@const at = place(field, field.name)}
				<div class="mt-3">
					<div class="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-3">
						<span class="w-24 truncate text-small text-txt2">{field.name}</span>
						<div class="group flex min-w-0 items-center gap-3">
							{#if masked}
								<ProtectedValue
									entry={entry.id}
									field={field.name}
									onCopy={(range) => onCopy(entry.id, field.name, range)}
									{onFailure}
								/>
							{:else}
								<!-- A protected field that is empty comes back with no value to
								     reveal, and it goes back protected: what the file says about
								     a field is what is written back, and nothing here decides it
								     again. A field of the reader's own may be given lines, so a
								     paste keeps its breaks. -->
								<Editable
									value={field.value ?? ''}
									label={field.name}
									placeholder="Empty"
									mono
									breaks
									multiline={field.name === longhand}
									readonly={locked}
									draft={at}
									onCommit={(value) => write(at.field, value, at.protect)}
								/>
							{/if}
							{#if !locked && dropping !== field.name}
								<button
									type="button"
									onclick={() => void dropField(field.name)}
									class="ml-2 flex h-7 w-7 shrink-0 items-center justify-center rounded-sm text-txt4 opacity-0 transition group-focus-within:opacity-100 group-hover:opacity-100 hover:bg-dangerwash hover:text-danger"
									aria-label="Remove the field {field.name}"
								>
									<Icon name="trash" class="h-4 w-4" />
								</button>
							{/if}
						</div>
						{#if masked}
							<div class="col-start-2 min-w-0">
								{@render changer(field)}
							</div>
						{/if}
					</div>
					{#if dropping === field.name}
						<Confirm
							class="mt-3 bg-surface2"
							question="Remove “{field.name}”? This vault keeps no version to bring it back from, so this can’t be undone."
							act="Remove"
							onKeep={() => (dropping = null)}
							onAct={() => void dropField(field.name, true)}
						/>
					{/if}
				</div>
			{/each}

			{#if naming}
				<!-- The name, and whether the value is written in lines. Leaving the
				     two of them finishes the name; the option does not take the
				     focus from it, so pressing it is not leaving. -->
				<div class="mt-3 flex animate-rise items-center gap-2" onfocusout={leaving}>
					<input
						bind:this={named}
						type="text"
						autocomplete="off"
						spellcheck="false"
						aria-label="The name of the new field"
						placeholder="What is it called?"
						onkeydown={(event) => {
							if (event.key === 'Escape') naming = false;
							if (event.key === 'Enter' && !composing(event)) {
								event.preventDefault();
								makeField();
							}
						}}
						class="min-w-0 flex-1 rounded-sm border border-accent bg-surface2 px-3 py-2 text-small text-txt ring-4 ring-accent/15 outline-none placeholder:text-txt4"
					/>
					<button
						type="button"
						onmousedown={(event) => event.preventDefault()}
						onclick={() => (inLines = !inLines)}
						aria-pressed={inLines}
						class="flex shrink-0 items-center gap-1.5 rounded-full border border-hairline px-3 py-1.5 font-mono text-meta transition-colors {inLines
							? 'bg-surface2 text-txt'
							: 'text-txt4 hover:text-txt3'}"
					>
						{#if inLines}
							<Icon name="check" class="h-3.5 w-3.5 text-txt2" />
						{/if}
						Multi-line
					</button>
				</div>
			{/if}
		</div>

		<div class="mt-6 border-t border-line pt-5">
			<div class="flex items-center justify-between">
				<span class="font-mono text-label tracking-label text-txt3 uppercase">Attachments</span>
				{#if !locked}
					<button
						type="button"
						onclick={() => void attach()}
						class="text-txt4 transition-colors hover:text-txt2"
						aria-label="Add a file"
					>
						<Icon name="plus" class="h-4 w-4" />
					</button>
				{/if}
			</div>

			<!--
				The question about a name that is taken, under the button that asked
				for the file. "Keep both" is where the focus lands and what Return
				gives, because it is what somebody adding the second page of a
				scan means; replacing is the destructive answer and stands last.
			-->
			{#if clash}
				<Confirm
					class="mt-3 bg-surface2"
					question={said(clash)}
					keep="Don’t add it"
					neutral={{ label: freed ? 'Add it' : 'Keep both', run: () => void keepBoth() }}
					focus="neutral"
					act={held || freed ? undefined : 'Replace'}
					onKeep={dismiss}
					onAct={() => void replace()}
				/>
			{/if}

			<!--
				Writing a file out and removing it used to be two icons of one size
				twelve pixels apart, and the one that was missed was the one that
				could not be taken back. Each is now a box a fingertip wide, the two
				stand a gap further apart, and the removal asks first.
			-->
			{#each entry.attachments as attachment (attachment.name)}
				<div class="mt-3 rounded-sm border border-hairline bg-surface2 px-3 py-2.5">
					<div class="flex items-center gap-3">
						<Icon name="file" class="h-4 w-4 shrink-0 text-txt4" />
						<span class="min-w-0 flex-1">
							<span class="block truncate font-mono text-fine text-txt">{attachment.name}</span>
							<span class="block font-mono text-label text-txt4">{size(attachment.size)}</span>
						</span>
						<button
							type="button"
							onclick={() => writeOut(attachment.name)}
							class="flex h-7 w-7 shrink-0 items-center justify-center rounded-sm text-txt4 transition-colors hover:bg-raised hover:text-txt2"
							aria-label="Write {attachment.fileName} out"
						>
							<Icon name="export" class="h-4 w-4" />
						</button>
						{#if !locked && asking !== attachment.name}
							<button
								type="button"
								onclick={() => (asking = attachment.name)}
								class="ml-3 flex h-7 w-7 shrink-0 items-center justify-center rounded-sm text-txt4 transition-colors hover:bg-dangerwash hover:text-danger"
								aria-label="Remove {attachment.name}"
							>
								<Icon name="trash" class="h-4 w-4" />
							</button>
						{/if}
					</div>
					{#if asking === attachment.name}
						<Confirm
							bare
							class="mt-3 border-t border-hairline pt-3"
							question="Remove {attachment.name} ({size(
								attachment.size
							)})? Files are not kept in Versions, so this can’t be undone."
							act="Remove"
							neutral={{ label: 'Save a copy first…', run: () => writeOut(attachment.name) }}
							onKeep={() => (asking = null)}
							onAct={() => void detach(attachment.name)}
						/>
					{/if}
				</div>
			{:else}
				<p class="mt-3 text-fine leading-relaxed text-txt4">
					Nothing yet. A key, a certificate or any other file lives beside the entry it belongs to.
				</p>
			{/each}
		</div>

		{#if pinned}
			<Confirm
				class="mt-3 bg-surface2"
				question="{pinned.message}. Removing it drops those versions - the ones here, and any on another entry that are holding it in place. The rest of the history stays."
				keep="Keep the file"
				act="Clear those versions and remove it"
				onKeep={() => (pinned = null)}
				onAct={detachWithVersions}
			/>
		{/if}

		<Versions
			entry={entry.id}
			{history}
			{now}
			readOnly={locked}
			onCopy={(at, name, range) => onCopy(entry.id, name, range, at)}
			{onVersions}
			onChanged={(changed) => onChanged(changed)}
			{onFailure}
		/>

		<!--
			The way to lose the entry is at the foot of the pane, in words, where no
			press aimed at anything else lands on it. Moving it to the bin is taken
			back from the notice that follows, so it goes at once; deleting it for
			good cannot be, so that asks first.
		-->
		{#if !locked}
			<div class="mt-6 border-t border-line pt-5">
				{#if erasing}
					<Confirm
						class="bg-surface2"
						question={forever}
						act="Delete forever"
						onKeep={() => (erasing = false)}
						onAct={() => {
							erasing = false;
							onDelete();
						}}
					/>
				{:else}
					<button
						type="button"
						onclick={() => (entry.deletion === 'bin' ? onDelete() : (erasing = true))}
						class="-ml-3 flex h-9 items-center gap-2 rounded-full px-3 text-small text-txt3 transition-colors hover:bg-dangerwash hover:text-danger"
					>
						<Icon name="trash" class="h-4 w-4" />
						{entry.deletion === 'bin' ? 'Move to Recycle Bin' : 'Delete forever…'}
					</button>
				{/if}
			</div>
		{/if}
	</div>

	<footer class="shrink-0 border-t border-hairline px-6 py-3 font-mono text-label text-txt4">
		Created {fully(entry.created, now)} · changed {fully(entry.modified, now)}
	</footer>
</section>
