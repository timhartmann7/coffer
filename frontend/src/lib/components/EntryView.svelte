<script lang="ts">
	import { fully, size } from '$lib/format';
	import {
		addAttachment,
		asFailure,
		exportAttachment,
		openUrl,
		removeAttachment,
		removeAttachmentAndVersions,
		removeField,
		setField,
		setTags
	} from '$lib/ipc';
	import type { Entry, Field, Group, Version } from '$lib/model';
	import Editable from './Editable.svelte';
	import Icon from './Icon.svelte';
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
		path,
		versions,
		now,
		readOnly,
		onCopy,
		onChanged,
		onVersions,
		onClose,
		onDelete,
		onFailure
	}: {
		entry: Entry;
		/** The folders from the vault down to this entry, for the line above the
		 * title. */
		path: Group[];
		versions: Version[];
		now: Date;
		/** A database Coffer will not write back. Nothing here offers a change
		 * that would only be refused. */
		readOnly: boolean;
		onCopy: (entry: string, field: string) => void;
		onChanged: (entry: Entry) => Promise<void>;
		onVersions: (versions: Version[]) => void;
		/** Puts the pane away. Escape does the same, and so does a press on the
		 * empty part of either pane to the left of this one. */
		onClose: () => void;
		onDelete: () => void;
		onFailure: (thrown: unknown) => void;
	} = $props();

	const of = (kind: Field['kind']) => entry.fields.find((field) => field.kind === kind);

	const title = $derived(of('title'));
	const username = $derived(of('username'));
	const password = $derived(of('password'));
	const url = $derived(of('url'));
	const notes = $derived(of('notes'));
	const custom = $derived(entry.fields.filter((field) => field.kind === 'custom'));

	let naming = $state(false);
	let named = $state<HTMLInputElement>();
	/**
	 * A file the entry's own previous versions still hold, and the reason it
	 * cannot go yet. The format keeps those versions inside the entry and the
	 * library gives no way to rewrite one, so the only way the file can leave is
	 * for the versions holding it to leave first - which is the reader's call
	 * and nobody else's.
	 */
	let pinned = $state<{ name: string; message: string } | null>(null);

	// Another entry is another set of answers. A banner about a file on the
	// entry that was open would otherwise still be on the screen under the next
	// one, offering to clear the wrong entry's history.
	$effect(() => {
		void entry.id;
		// Cleared on the way out rather than on the way in: a write inside the
		// body of an effect is a read of what was there, and an effect that
		// reads what it writes runs again the moment anything sets it.
		return () => {
			pinned = null;
			naming = false;
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

	function write(field: string, value: string, protect: boolean): Promise<boolean> {
		return change(() => setField(entry.id, field, value, protect));
	}

	/** Takes a file off, or says why it cannot go yet. */
	async function detach(name: string) {
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

	/** What the reader asked for, once they have said the versions may go. */
	async function detachWithVersions() {
		const name = pinned?.name;
		pinned = null;
		if (name === undefined) return;
		try {
			await onChanged(await removeAttachmentAndVersions(entry.id, name));
			onVersions([]);
		} catch (thrown) {
			onFailure(thrown);
		}
	}

	/** The name a standard field carries in the file. An entry that arrived
	 * without one still has to be editable, so the format's own name is used. */
	function nameOf(field: Field | undefined, standard: string): string {
		return field?.name ?? standard;
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

	function makeField() {
		if (!named) return;
		const name = named.value.trim();
		named.value = '';
		naming = false;
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
		void write(name, '', true);
	}
</script>

<section class="flex h-full animate-fade flex-col overflow-hidden bg-surface">
	<header class="shrink-0 border-b border-hairline px-6 py-5">
		<!--
			The folders down to this entry, and only when there are any. An entry at
			the top of a vault has none, and the empty line it used to leave was
			what pushed the title below the two buttons beside it.
		-->
		{#if path.length > 0}
			<div class="mb-1.5 truncate font-mono text-label tracking-label text-txt4 uppercase">
				{path.map((group) => group.name).join(' · ')}
			</div>
		{/if}

		<!-- The name and the two things that can be done to the entry, on one
		     line and centred against each other. The way out first and the way to
		     lose the entry last, with a gap between them: the mockup's rule is
		     that a destructive action stands at the end of a row so that missing
		     it costs a movement. -->
		<div class="flex items-center gap-4">
			{#if title && title.value === null}
				<h1 class="min-w-0 flex-1 truncate text-title font-medium tracking-tight text-txt">
					<Mask />
				</h1>
			{:else}
				<h1 class="-ml-2 flex min-w-0 flex-1">
					<Editable
						value={title?.value ?? ''}
						label="Title"
						placeholder="Untitled"
						classes="text-title font-medium tracking-tight text-txt"
						readonly={readOnly}
						bare
						onCommit={(value) => write(nameOf(title, 'Title'), value, title?.protected ?? false)}
					/>
				</h1>
			{/if}
			<button
				type="button"
				onclick={onClose}
				class="shrink-0 text-txt4 transition-colors hover:text-txt2"
				aria-label="Close this entry"
			>
				<Icon name="x" class="h-4 w-4" />
			</button>
			{#if !readOnly}
				<button
					type="button"
					onclick={onDelete}
					class="text-txt4 transition-colors hover:text-danger"
					aria-label="Delete this entry"
				>
					<Icon name="trash" class="h-4 w-4" />
				</button>
			{/if}
		</div>
	</header>

	<div class="flex-1 overflow-y-auto px-6 py-5">
		<div class="block">
			<span class="font-mono text-label tracking-label text-txt3 uppercase">Login</span>
			<span class="mt-1.5 flex items-center gap-2">
				{#if username && username.value === null && !username.empty}
					<!-- A database may protect any field, the login included. It comes
					     back the same way a protected custom field does: one reveal at
					     a time. -->
					<ProtectedValue entry={entry.id} field={username.name} {onFailure} />
				{:else}
					<Editable
						value={username?.value ?? ''}
						label="Login"
						placeholder="No login"
						mono
						readonly={readOnly}
						onCommit={(value) =>
							write(nameOf(username, 'UserName'), value, username?.protected ?? false)}
					/>
				{/if}
				{#if username && !username.empty}
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
		</div>

		<PasswordField
			entry={entry.id}
			field={nameOf(password, 'Password')}
			empty={password?.empty ?? true}
			protect={password?.protected ?? true}
			{readOnly}
			onCopy={(field) => onCopy(entry.id, field)}
			onCommit={write}
			{onFailure}
		/>

		<div class="mt-5">
			<span class="font-mono text-label tracking-label text-txt3 uppercase">Address</span>
			{#if url && url.value === null && !url.empty}
				<span class="mt-1.5 flex items-center gap-2">
					<ProtectedValue entry={entry.id} field={url.name} {onFailure} />
				</span>
			{:else}
				<span class="mt-1.5 flex items-center gap-2">
					<Editable
						value={url?.value ?? ''}
						label="Address"
						placeholder="No address"
						readonly={readOnly}
						onCommit={(value) => write(nameOf(url, 'URL'), value, url?.protected ?? false)}
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
				{readOnly}
				onSet={(tags) => void change(() => setTags(entry.id, tags))}
			/>
		</div>

		<div class="mt-6 border-t border-line pt-5">
			<div class="font-mono text-label tracking-label text-txt3 uppercase">Notes</div>
			{#if notes && notes.value === null && !notes.empty}
				<div class="mt-3 flex items-center gap-3">
					<ProtectedValue entry={entry.id} field={notes.name} {onFailure} />
				</div>
			{:else}
				<div class="mt-3 flex items-start">
					<Editable
						value={notes?.value ?? ''}
						label="Notes"
						placeholder="Nothing written down"
						multiline
						readonly={readOnly}
						onCommit={(value) => write(nameOf(notes, 'Notes'), value, notes?.protected ?? false)}
					/>
				</div>
			{/if}
		</div>

		<div class="mt-6 border-t border-line pt-5">
			<div class="flex items-center justify-between">
				<span class="font-mono text-label tracking-label text-txt3 uppercase">Own fields</span>
				{#if !readOnly}
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

			{#each custom as field (field.name)}
				<div class="mt-3 flex items-center gap-3">
					<span class="w-24 shrink-0 truncate text-small text-txt2">{field.name}</span>
					{#if field.value === null && !field.empty}
						<ProtectedValue entry={entry.id} field={field.name} {onFailure} />
					{:else}
						<!-- A protected field that is empty comes back with no value to
						     reveal, and it goes back protected: what the file says about
						     a field is what is written back, and nothing here decides it
						     again. -->
						<Editable
							value={field.value ?? ''}
							label={field.name}
							placeholder="Empty"
							mono
							readonly={readOnly}
							onCommit={(value) => write(field.name, value, field.protected)}
						/>
					{/if}
					{#if !readOnly}
						<button
							type="button"
							onclick={() => void change(() => removeField(entry.id, field.name))}
							class="shrink-0 text-txt4 transition-colors hover:text-danger"
							aria-label="Remove the field {field.name}"
						>
							<Icon name="trash" class="h-4 w-4" />
						</button>
					{/if}
				</div>
			{/each}

			{#if naming}
				<input
					bind:this={named}
					type="text"
					autocomplete="off"
					spellcheck="false"
					aria-label="The name of the new field"
					placeholder="What is it called?"
					onblur={makeField}
					onkeydown={(event) => {
						if (event.key === 'Escape') naming = false;
						if (event.key === 'Enter') {
							event.preventDefault();
							makeField();
						}
					}}
					class="mt-3 w-full animate-rise rounded-sm border border-accent bg-surface2 px-3 py-2 text-small text-txt ring-4 ring-accent/15 outline-none placeholder:text-txt4"
				/>
			{/if}
		</div>

		<div class="mt-6 border-t border-line pt-5">
			<div class="flex items-center justify-between">
				<span class="font-mono text-label tracking-label text-txt3 uppercase">Attachments</span>
				{#if !readOnly}
					<button
						type="button"
						onclick={() => void change(() => addAttachment(entry.id))}
						class="text-txt4 transition-colors hover:text-txt2"
						aria-label="Add a file"
					>
						<Icon name="plus" class="h-4 w-4" />
					</button>
				{/if}
			</div>

			{#each entry.attachments as attachment (attachment.name)}
				<div
					class="mt-3 flex items-center gap-3 rounded-sm border border-hairline bg-surface2 px-3 py-2.5"
				>
					<Icon name="file" class="h-4 w-4 shrink-0 text-txt4" />
					<span class="min-w-0 flex-1">
						<span class="block truncate font-mono text-fine text-txt">{attachment.name}</span>
						<span class="block font-mono text-label text-txt4">{size(attachment.size)}</span>
					</span>
					<button
						type="button"
						onclick={() => exportAttachment(entry.id, attachment.name).catch(onFailure)}
						class="shrink-0 text-txt4 transition-colors hover:text-txt2"
						aria-label="Write {attachment.fileName} out"
					>
						<Icon name="export" class="h-4 w-4" />
					</button>
					{#if !readOnly}
						<button
							type="button"
							onclick={() => void detach(attachment.name)}
							class="shrink-0 text-txt4 transition-colors hover:text-danger"
							aria-label="Remove {attachment.name}"
						>
							<Icon name="trash" class="h-4 w-4" />
						</button>
					{/if}
				</div>
			{:else}
				<p class="mt-3 text-fine leading-relaxed text-txt4">
					Nothing yet. A key, a certificate or any other file lives beside the entry it belongs to.
				</p>
			{/each}
		</div>

		{#if pinned}
			<div class="mt-3 animate-rise rounded-sm border border-hairline bg-surface2 p-3">
				<p class="text-fine leading-relaxed text-txt2">
					{pinned.message} of this entry. The file can go once they have.
				</p>
				<div class="mt-3 flex flex-wrap gap-2">
					<button
						type="button"
						onclick={() => (pinned = null)}
						class="h-9 rounded-full px-4 text-small text-txt3 transition-colors hover:text-txt2"
					>
						Keep the file
					</button>
					<button
						type="button"
						onclick={detachWithVersions}
						class="h-9 rounded-full px-4 text-small text-danger transition-colors hover:bg-dangerwash"
					>
						Clear the versions and remove it
					</button>
				</div>
			</div>
		{/if}

		<Versions
			entry={entry.id}
			{versions}
			{now}
			{readOnly}
			{onVersions}
			onChanged={(changed) => onChanged(changed)}
			{onFailure}
		/>
	</div>

	<footer class="shrink-0 border-t border-hairline px-6 py-3 font-mono text-label text-txt4">
		Created {fully(entry.created, now)} · changed {fully(entry.modified, now)}
	</footer>
</section>
