<script lang="ts">
	import { fully } from '$lib/format';
	import { clearHistory, deleteVersion, restoreVersion, revealVersion, version } from '$lib/ipc';
	import type { Entry, Version } from '$lib/model';
	import Icon from './Icon.svelte';
	import ProtectedValue from './ProtectedValue.svelte';

	/**
	 * What the entry held before, closed until it is asked for.
	 *
	 * The format keeps previous versions inside the entry, so this is the whole
	 * of what "history" means here: no other screen shows one, and a version
	 * never appears in the tree, the list or a search.
	 *
	 * A version is addressed by its position rather than by its date, because
	 * dates have one-second resolution and two versions written in the same
	 * second are the same date.
	 */
	let {
		entry,
		versions,
		now,
		readOnly,
		onVersions,
		onChanged,
		onFailure
	}: {
		entry: string;
		versions: Version[];
		now: Date;
		/** A database Coffer will not write back: a version can be read and not
		 * restored, dropped or cleared. */
		readOnly: boolean;
		onVersions: (versions: Version[]) => void;
		onChanged: (entry: Entry) => Promise<void>;
		onFailure: (thrown: unknown) => void;
	} = $props();

	let open = $state(false);
	let showing = $state<{ index: number; entry: Entry } | null>(null);
	let confirming = $state(false);

	// Another entry is another history. Whatever is on the screen goes with it.
	$effect(() => {
		void entry;
		showing = null;
		confirming = false;
	});

	/**
	 * A version is addressed by its position, and dropping one moves every
	 * position after it. Anything open when the list changes is showing a
	 * version by a number that may now name a different one, so it closes.
	 */
	$effect(() => {
		void versions;
		showing = null;
	});

	/** Newest first: what a reader looks for is what changed last. */
	const listed = $derived([...versions].reverse());

	async function view(index: number) {
		if (showing?.index === index) {
			showing = null;
			return;
		}
		try {
			showing = { index, entry: await version(entry, index) };
		} catch (thrown) {
			onFailure(thrown);
		}
	}

	async function restore(index: number) {
		try {
			showing = null;
			await onChanged(await restoreVersion(entry, index));
		} catch (thrown) {
			onFailure(thrown);
		}
	}

	async function drop(index: number) {
		try {
			showing = null;
			onVersions(await deleteVersion(entry, index));
		} catch (thrown) {
			onFailure(thrown);
		}
	}

	async function clear() {
		try {
			confirming = false;
			showing = null;
			onVersions(await clearHistory(entry));
		} catch (thrown) {
			onFailure(thrown);
		}
	}
</script>

<div class="mt-6 border-t border-line pt-5">
	<button
		type="button"
		onclick={() => (open = !open)}
		class="flex w-full items-center gap-2 font-mono text-label tracking-label text-txt3 uppercase transition-colors hover:text-txt2"
	>
		<Icon name={open ? 'chev-d' : 'chev-r'} class="h-4 w-4 text-txt4" />
		<span>Versions</span>
		<span class="text-txt4">{versions.length}</span>
	</button>

	{#if open}
		{#if versions.length === 0}
			<p class="mt-3 text-fine leading-relaxed text-txt4">
				Nothing yet. Every change to this entry keeps what was there before, so this fills up as the
				entry is worked on.
			</p>
		{:else}
			{#each listed as version (version.index)}
				{@const here = showing?.index === version.index}
				<div class="mt-2 rounded-sm border border-hairline bg-surface2">
					<div class="flex items-center gap-3 px-3 py-2.5">
						<Icon name="clip" class="h-3.5 w-3.5 shrink-0 text-txt4" />
						<span class="min-w-0 flex-1 truncate font-mono text-fine text-txt2">
							{fully(version.modified, now)}
						</span>
						<button
							type="button"
							onclick={() => view(version.index)}
							class="text-fine text-txt3 transition-colors hover:text-txt"
						>
							{here ? 'Close' : 'View'}
						</button>
						{#if !readOnly}
							<button
								type="button"
								onclick={() => restore(version.index)}
								class="text-fine text-txt3 transition-colors hover:text-txt"
							>
								Restore
							</button>
							<button
								type="button"
								onclick={() => drop(version.index)}
								class="text-txt4 transition-colors hover:text-danger"
								aria-label="Delete this version"
							>
								<Icon name="trash" class="h-4 w-4" />
							</button>
						{/if}
					</div>

					{#if here && showing}
						{@const at = showing.index}
						<div class="border-t border-hairline px-3 py-3">
							{#each showing.entry.fields as field (field.name)}
								<div class="mt-1.5 flex items-start gap-3 first:mt-0">
									<span class="w-24 shrink-0 truncate text-fine text-txt3">{field.name}</span>
									{#if field.value === null && !field.empty}
										<!-- One of these per value, each holding its own node, so the
										     value that was asked for cannot land in another row. -->
										<ProtectedValue
											{entry}
											field={field.name}
											label="{field.name} as it was"
											read={(of, name) => revealVersion(of, at, name)}
											bare
											{onFailure}
										/>
									{:else}
										<span
											class="min-w-0 flex-1 font-mono text-fine break-all whitespace-pre-wrap text-txt2"
										>
											{field.value ?? ''}
										</span>
									{/if}
								</div>
							{/each}
							<p class="mt-3 font-mono text-label text-txt4">
								A version is read only. Restoring it keeps what is here now as a version of its own.
							</p>
						</div>
					{/if}
				</div>
			{/each}

			<div class="mt-3">
				{#if readOnly}
					<!-- Nothing to offer: this database is not written back. -->
				{:else if confirming}
					<div class="flex flex-wrap items-center gap-2">
						<span class="text-fine text-txt2">
							Drop all {versions.length} versions? What the entry holds now stays.
						</span>
						<button
							type="button"
							onclick={() => (confirming = false)}
							class="h-9 rounded-full px-4 text-small text-txt3 transition-colors hover:text-txt2"
						>
							Keep them
						</button>
						<button
							type="button"
							onclick={clear}
							class="h-9 rounded-full px-4 text-small text-danger transition-colors hover:bg-dangerwash"
						>
							Clear the history
						</button>
					</div>
				{:else}
					<button
						type="button"
						onclick={() => (confirming = true)}
						class="text-fine text-txt3 transition-colors hover:text-txt2"
					>
						Clear the history
					</button>
				{/if}
			</div>
		{/if}
	{/if}
</div>
