<script lang="ts">
	import { fully, size } from '$lib/format';
	import {
		addAttachment,
		exportAttachment,
		openUrl,
		removeAttachment,
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
		onCopy,
		onChanged,
		onVersions,
		onDelete,
		onFailure
	}: {
		entry: Entry;
		/** The folders from the vault down to this entry, for the line above the
		 * title. */
		path: Group[];
		versions: Version[];
		now: Date;
		onCopy: (entry: string, field: string) => void;
		onChanged: (entry: Entry) => void;
		onVersions: (versions: Version[]) => void;
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

	/** Runs a change and hands back what Rust now holds. */
	async function change(run: () => Promise<Entry>) {
		try {
			onChanged(await run());
		} catch (thrown) {
			onFailure(thrown);
		}
	}

	function write(field: string, value: string, protect: boolean) {
		void change(() => setField(entry.id, field, value, protect));
	}

	/** The name a standard field carries in the file. An entry that arrived
	 * without one still has to be editable, so the format's own name is used. */
	function nameOf(field: Field | undefined, standard: string): string {
		return field?.name ?? standard;
	}

	function addField() {
		naming = true;
		queueMicrotask(() => named?.focus());
	}

	function makeField() {
		if (!named) return;
		const name = named.value.trim();
		named.value = '';
		naming = false;
		if (name === '') return;
		// A new field of the reader's own is protected: a field somebody adds to
		// a password entry is far more often a secret than not, and the value is
		// shown behind an eye until they say otherwise.
		write(name, '', true);
	}
</script>

<section class="flex flex-col overflow-hidden bg-surface">
	<header class="shrink-0 border-b border-hairline px-6 py-5">
		<div class="flex items-start gap-3">
			<div class="min-w-0 flex-1">
				<div class="font-mono text-label tracking-label text-txt4 uppercase">
					{path.map((group) => group.name).join(' · ')}
				</div>
				{#if title && title.value === null}
					<h1 class="mt-2 truncate text-title font-medium tracking-tight text-txt">
						<Mask />
					</h1>
				{:else}
					<h1 class="mt-1.5 -ml-2 flex">
						<Editable
							value={title?.value ?? ''}
							label="Title"
							placeholder="Untitled"
							classes="text-title font-medium tracking-tight text-txt"
							onCommit={(value) => write(nameOf(title, 'Title'), value, title?.protected ?? false)}
						/>
					</h1>
				{/if}
			</div>
			<button
				type="button"
				onclick={onDelete}
				class="text-txt4 transition-colors hover:text-danger"
				aria-label="Delete this entry"
			>
				<Icon name="trash" class="h-4 w-4" />
			</button>
		</div>
	</header>

	<div class="flex-1 overflow-y-auto px-6 py-5">
		<div class="block">
			<span class="font-mono text-label tracking-label text-txt3 uppercase">Login</span>
			<span class="mt-1.5 -ml-2 flex items-center gap-2">
				{#if username && username.value === null && !username.empty}
					<!-- A database may protect any field, the login included. It comes
					     back the same way a protected custom field does: one reveal at
					     a time. -->
					<span class="ml-2 flex min-w-0 flex-1 items-center gap-2">
						<ProtectedValue entry={entry.id} field={username.name} {onFailure} />
					</span>
				{:else}
					<Editable
						value={username?.value ?? ''}
						label="Login"
						placeholder="No login"
						mono
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
				<span class="mt-1.5 -ml-2 flex items-center gap-2">
					<Editable
						value={url?.value ?? ''}
						label="Address"
						placeholder="No address"
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
			<Tags tags={entry.tags} onSet={(tags) => void change(() => setTags(entry.id, tags))} />
		</div>

		<div class="mt-6 border-t border-line pt-5">
			<div class="font-mono text-label tracking-label text-txt3 uppercase">Notes</div>
			{#if notes && notes.value === null && !notes.empty}
				<div class="mt-3 flex items-center gap-3">
					<ProtectedValue entry={entry.id} field={notes.name} {onFailure} />
				</div>
			{:else}
				<div class="-ml-2">
					<Editable
						value={notes?.value ?? ''}
						label="Notes"
						placeholder="Nothing written down"
						multiline
						onCommit={(value) => write(nameOf(notes, 'Notes'), value, notes?.protected ?? false)}
					/>
				</div>
			{/if}
		</div>

		<div class="mt-6 border-t border-line pt-5">
			<div class="flex items-center justify-between">
				<span class="font-mono text-label tracking-label text-txt3 uppercase">Own fields</span>
				<button
					type="button"
					onclick={addField}
					class="text-txt4 transition-colors hover:text-txt2"
					aria-label="Add a field"
				>
					<Icon name="plus" class="h-4 w-4" />
				</button>
			</div>

			{#each custom as field (field.name)}
				<div class="mt-3 flex items-center gap-3">
					<span class="w-24 shrink-0 truncate text-small text-txt2">{field.name}</span>
					{#if field.value === null && !field.empty}
						<ProtectedValue entry={entry.id} field={field.name} {onFailure} />
					{:else if field.value === null}
						<span class="-ml-2 min-w-0 flex-1">
							<Editable
								value=""
								label={field.name}
								placeholder="Empty"
								mono
								onCommit={(value) => write(field.name, value, true)}
							/>
						</span>
					{:else}
						<span class="-ml-2 min-w-0 flex-1">
							<Editable
								value={field.value}
								label={field.name}
								placeholder="Empty"
								mono
								onCommit={(value) => write(field.name, value, field.protected)}
							/>
						</span>
					{/if}
					<button
						type="button"
						onclick={() => void change(() => removeField(entry.id, field.name))}
						class="shrink-0 text-txt4 transition-colors hover:text-danger"
						aria-label="Remove the field {field.name}"
					>
						<Icon name="trash" class="h-4 w-4" />
					</button>
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
					class="mt-3 w-full rounded-sm border border-accent bg-surface2 px-3 py-2 text-small text-txt ring-4 ring-accent/15 outline-none placeholder:text-txt4"
				/>
			{/if}
		</div>

		<div class="mt-6 border-t border-line pt-5">
			<div class="flex items-center justify-between">
				<span class="font-mono text-label tracking-label text-txt3 uppercase">Attachments</span>
				<button
					type="button"
					onclick={() => void change(() => addAttachment(entry.id))}
					class="text-txt4 transition-colors hover:text-txt2"
					aria-label="Add a file"
				>
					<Icon name="plus" class="h-4 w-4" />
				</button>
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
					<button
						type="button"
						onclick={() => void change(() => removeAttachment(entry.id, attachment.name))}
						class="shrink-0 text-txt4 transition-colors hover:text-danger"
						aria-label="Remove {attachment.name}"
					>
						<Icon name="trash" class="h-4 w-4" />
					</button>
				</div>
			{:else}
				<p class="mt-3 text-fine leading-relaxed text-txt4">
					Nothing yet. A key, a certificate or any other file lives beside the entry it belongs to.
				</p>
			{/each}
		</div>

		<Versions
			entry={entry.id}
			{versions}
			{now}
			{onVersions}
			onChanged={(changed) => onChanged(changed)}
			{onFailure}
		/>
	</div>

	<footer class="shrink-0 border-t border-hairline px-6 py-3 font-mono text-label text-txt4">
		Created {fully(entry.created, now)} · changed {fully(entry.modified, now)}
	</footer>
</section>
