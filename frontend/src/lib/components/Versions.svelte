<script lang="ts">
	import { fully } from '$lib/format';
	import { clearHistory, deleteVersion, restoreVersion, revealVersion, version } from '$lib/ipc';
	import type { Entry, History, Position, Span } from '$lib/model';
	import Confirm from './Confirm.svelte';
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
	 * second are the same date. The position goes to Rust with the revision of
	 * the vault the list was read at, and Rust refuses it once the vault has
	 * changed since: see `Position`.
	 */
	let {
		entry,
		history,
		now,
		readOnly,
		onCopy,
		onVersions,
		onChanged,
		onFailure
	}: {
		entry: string;
		/**
		 * The versions Rust listed, with the entry it listed them for, or `null`
		 * while they are being read - which includes the save after every change
		 * to the entry. A list of any other entry's is not drawn, so nothing
		 * here can act on it: a position is a position in one entry's history,
		 * and in another's it names a version nobody saw.
		 */
		history: History | null;
		now: Date;
		/** A database Coffer will not write back: a version can be read and not
		 * restored, dropped or cleared. */
		readOnly: boolean;
		/** Copies a field of a version in Rust, whole or the part of it the
		 * reader selected. A value read here is as much a secret as the one in
		 * the entry, and goes to the clipboard the same way. */
		onCopy: (at: Position, field: string, range: Span | null) => void;
		/** The list after a drop or a clear, which is a change to the file like
		 * any other. Settled once the save after it has been read back. */
		onVersions: (history: History) => Promise<void>;
		onChanged: (entry: Entry) => Promise<void>;
		onFailure: (thrown: unknown) => void;
	} = $props();

	/** This entry's list, or `null` while there is no list of this entry's to
	 * draw. */
	const own = $derived(history !== null && history.entry === entry ? history : null);

	let open = $state(false);
	let showing = $state<{ at: Position; entry: Entry } | null>(null);
	let confirming = $state(false);
	/** The version whose deletion is being asked about. Nothing brings one back
	 * once the vault is written, and the vault is written straight away. */
	let dropping = $state<number | null>(null);

	/**
	 * A version is addressed by its position in one entry's history, and every
	 * change moves positions: a drop renumbers the rest, a save prunes. Anything
	 * open when the entry or its list changes is about a number that may name a
	 * different version now - a version being read, a question about dropping
	 * one or all of them - so it closes rather than acting on whichever version
	 * had moved into the place.
	 */
	$effect(() => {
		void entry;
		void own;
		showing = null;
		confirming = false;
		dropping = null;
	});

	/** Newest first: what a reader looks for is what changed last. */
	const listed = $derived(own === null ? [] : [...own.versions].reverse());

	/**
	 * Whether a restore, a drop or a clear is on its way.
	 *
	 * Each is a change followed by a save, and the list on the screen is the
	 * one from before it until the list read after that save arrives. A second
	 * press in that time names a position in a list that is no longer true, so
	 * every press is let go until then. Nothing is drawn from it, so it is not
	 * state.
	 */
	let acting = false;

	/** Runs one change to the versions at a time, and drops a press that
	 * arrives during one. */
	async function act(change: () => Promise<void>) {
		if (acting) return;
		acting = true;
		try {
			await change();
		} catch (thrown) {
			onFailure(thrown);
		} finally {
			acting = false;
		}
	}

	async function view(at: Position) {
		if (showing?.at.index === at.index) {
			showing = null;
			return;
		}
		if (acting) return;
		const from = own;
		try {
			const read = await version(entry, at);
			// The list changed while the version was being read - a version
			// dropped, a save that pruned, another entry - and the position it
			// was read at may name another version now, or none of this entry's.
			if (own === from) showing = { at, entry: read };
		} catch (thrown) {
			onFailure(thrown);
		}
	}

	function restore(at: Position) {
		return act(async () => {
			showing = null;
			await onChanged(await restoreVersion(entry, at));
		});
	}

	function drop(at: Position) {
		return act(async () => {
			dropping = null;
			showing = null;
			await onVersions(await deleteVersion(entry, at));
		});
	}

	function clear() {
		return act(async () => {
			confirming = false;
			showing = null;
			await onVersions(await clearHistory(entry));
		});
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
		{#if own !== null}
			<span class="text-txt4">{own.versions.length}</span>
		{/if}
	</button>

	{#if open && own !== null}
		{#if own.versions.length === 0}
			<p class="mt-3 animate-rise text-fine leading-relaxed text-txt4">
				Nothing yet. Every change to this entry keeps what was there before, so this fills up as the
				entry is worked on.
			</p>
		{:else}
			{#each listed as version (version.index)}
				{@const at = { index: version.index, revision: own.revision }}
				{@const here = showing?.at.index === version.index}
				<div class="mt-2 animate-rise rounded-sm border border-hairline bg-surface2">
					<div class="flex items-center gap-3 px-3 py-2.5">
						<Icon name="clip" class="h-3.5 w-3.5 shrink-0 text-txt4" />
						<span class="min-w-0 flex-1 truncate font-mono text-fine text-txt2">
							{fully(version.modified, now)}
						</span>
						<button
							type="button"
							onclick={() => view(at)}
							class="text-fine text-txt3 transition-colors hover:text-txt"
						>
							{here ? 'Close' : 'View'}
						</button>
						{#if !readOnly}
							<button
								type="button"
								onclick={() => restore(at)}
								class="text-fine text-txt3 transition-colors hover:text-txt"
							>
								Restore
							</button>
							<!-- A fingertip wide and a step further off than View and
							     Restore, and drawn into the row's own padding so the row
							     is no taller for it. -->
							<button
								type="button"
								onclick={() => (dropping = version.index)}
								class="-my-1.5 ml-2 flex h-7 w-7 shrink-0 items-center justify-center rounded-sm text-txt4 transition-colors hover:bg-dangerwash hover:text-danger"
								aria-label="Delete this version"
							>
								<Icon name="trash" class="h-4 w-4" />
							</button>
						{/if}
					</div>

					{#if dropping === version.index}
						<Confirm
							bare
							class="border-t border-hairline px-3 py-3"
							question="Drop this version? What the entry holds now stays."
							act="Drop it"
							onKeep={() => (dropping = null)}
							onAct={() => drop(at)}
						/>
					{/if}

					{#if here && showing}
						{@const viewed = showing.at}
						<div class="animate-rise border-t border-hairline px-3 py-3">
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
											read={(of, name) => revealVersion(of, viewed, name)}
											bare
											onCopy={(range) => onCopy(viewed, field.name, range)}
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
					<Confirm
						class="bg-surface2"
						question="Drop all {own.versions.length} versions? What the entry holds now stays."
						keep="Keep them"
						act="Clear the history"
						onKeep={() => (confirming = false)}
						onAct={clear}
					/>
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
