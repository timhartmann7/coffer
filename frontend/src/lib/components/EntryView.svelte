<script lang="ts">
	import { fully, size } from '$lib/format';
	import { openUrl } from '$lib/ipc';
	import type { Entry, Field, Group } from '$lib/model';
	import Icon from './Icon.svelte';
	import Mask from './Mask.svelte';
	import PasswordField from './PasswordField.svelte';
	import ProtectedValue from './ProtectedValue.svelte';

	let {
		entry,
		path,
		now,
		onCopy,
		onFailure
	}: {
		entry: Entry;
		/** The groups from the vault down to this entry, for the line above the
		 * title. */
		path: Group[];
		now: Date;
		onCopy: (entry: string, field: string) => void;
		onFailure: (thrown: unknown) => void;
	} = $props();

	const of = (kind: Field['kind']) => entry.fields.find((field) => field.kind === kind);

	const title = $derived(of('title'));
	const username = $derived(of('username'));
	const password = $derived(of('password'));
	const url = $derived(of('url'));
	const notes = $derived(of('notes'));
	const custom = $derived(entry.fields.filter((field) => field.kind === 'custom'));
</script>

<section class="flex flex-col overflow-hidden bg-surface">
	<header class="shrink-0 border-b border-hairline px-6 py-5">
		<div class="font-mono text-label tracking-label text-txt4 uppercase">
			{path.map((group) => group.name).join(' · ')}
		</div>
		<h1 class="mt-2 truncate text-title font-medium tracking-tight text-txt">
			{#if title && title.value === null}
				<Mask />
			{:else}
				{title?.value ?? ''}
			{/if}
		</h1>
	</header>

	<div class="flex-1 overflow-y-auto px-6 py-5">
		<div class="block">
			<span class="font-mono text-label tracking-label text-txt3 uppercase">Login</span>
			<span class="mt-1.5 flex items-center gap-2">
				<span class="min-w-0 flex-1 truncate font-mono text-body text-txt select-text">
					{#if username && username.value === null}
						<Mask />
					{:else}
						{username?.value ?? ''}
					{/if}
				</span>
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

		{#if password}
			<PasswordField
				entry={entry.id}
				field={password.name}
				empty={password.empty}
				onCopy={(field) => onCopy(entry.id, field)}
				{onFailure}
			/>
		{/if}

		{#if url && !url.empty}
			<div class="mt-5">
				<span class="font-mono text-label tracking-label text-txt3 uppercase">Address</span>
				{#if url.openable}
					<button
						type="button"
						onclick={() => openUrl(entry.id).catch(onFailure)}
						class="mt-1.5 block w-full truncate text-left text-body text-txt2 underline decoration-hairline underline-offset-4 transition-colors hover:text-txt"
					>
						{url.value}
					</button>
				{:else if url.value === null}
					<span class="mt-1.5 block"><Mask /></span>
				{:else}
					<!-- An address Coffer will not open is a value on a screen and
					     nothing more. Nothing here can be clicked into a browser. -->
					<span class="mt-1.5 block truncate text-body text-txt2 select-text">
						{url.value}
					</span>
				{/if}
			</div>
		{/if}

		{#if entry.tags.length > 0}
			<div class="mt-5">
				<span class="font-mono text-label tracking-label text-txt3 uppercase">Tags</span>
				<div class="mt-2 flex flex-wrap gap-1.5">
					{#each entry.tags as tag (tag)}
						<span
							class="rounded-full bg-surface2 px-2.5 py-1 font-mono text-label tracking-label text-txt3 uppercase"
						>
							{tag}
						</span>
					{/each}
				</div>
			</div>
		{/if}

		{#if notes && !notes.empty}
			<div class="mt-6 border-t border-line pt-5">
				<div class="font-mono text-label tracking-label text-txt3 uppercase">Notes</div>
				<p class="mt-3 text-small leading-relaxed whitespace-pre-wrap text-txt2 select-text">
					{notes.value}
				</p>
			</div>
		{/if}

		{#if custom.length > 0}
			<div class="mt-6 border-t border-line pt-5">
				<div class="font-mono text-label tracking-label text-txt3 uppercase">Own fields</div>
				{#each custom as field (field.name)}
					<div class="mt-3 flex items-center gap-3">
						<span class="w-24 shrink-0 truncate text-small text-txt2">{field.name}</span>
						{#if field.value === null && !field.empty}
							<ProtectedValue entry={entry.id} field={field.name} {onFailure} />
						{:else}
							<span class="min-w-0 flex-1 font-mono text-small break-all text-txt select-text">
								{field.value ?? ''}
							</span>
						{/if}
					</div>
				{/each}
			</div>
		{/if}

		{#if entry.attachments.length > 0}
			<div class="mt-6 border-t border-line pt-5">
				<div class="font-mono text-label tracking-label text-txt3 uppercase">Attachments</div>
				{#each entry.attachments as attachment (attachment.name)}
					<div
						class="mt-3 flex items-center gap-3 rounded-sm border border-hairline bg-surface2 px-3 py-2.5"
					>
						<Icon name="file" class="h-4 w-4 shrink-0 text-txt4" />
						<span class="min-w-0 flex-1">
							<span class="block truncate font-mono text-fine text-txt">{attachment.name}</span>
							<span class="block font-mono text-label text-txt4">{size(attachment.size)}</span>
						</span>
					</div>
				{/each}
			</div>
		{/if}
	</div>

	<footer class="shrink-0 border-t border-hairline px-6 py-3 font-mono text-label text-txt4">
		Created {fully(entry.created, now)} · changed {fully(entry.modified, now)}
	</footer>
</section>
