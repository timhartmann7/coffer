<script lang="ts">
	import { fully } from '$lib/format';
	import type { Rival } from '$lib/model';
	import Icon from './Icon.svelte';

	/**
	 * What to do when the file underneath the window stopped being the one it was
	 * opened from.
	 *
	 * Two shapes of that, and the difference is whether there is still a file.
	 *
	 * Somebody rewrote it: three outcomes, named by what they do rather than by
	 * "OK" and "Cancel", and each one keeps something. Taking the version on disk
	 * drops the changes in this window; keeping this version writes over the
	 * file, and what was there goes into the first snapshot on the way, so it can
	 * still be opened. Keeping both writes this version beside the database as a
	 * file of its own.
	 *
	 * It is gone: renamed, moved, deleted, or on a disk that was unplugged. There
	 * is nothing to reload and nothing to compare against, so the two outcomes
	 * are writing it back where it belongs and putting it somewhere else. Without
	 * them a save that failed here failed for every save afterwards, and a whole
	 * session's work sat in memory with no way out of the application at all.
	 */
	let {
		rival,
		missing = false,
		entries,
		changedAt,
		now,
		busy,
		onReload,
		onCopy,
		onOverwrite
	}: {
		/** What the file on disk holds, or as much of it as could be read. */
		rival: Rival;
		/** Whether the file is not there at all, rather than changed. */
		missing?: boolean;
		/** How many entries this window holds. */
		entries: number;
		/** When this window last changed something. */
		changedAt: Date | null;
		now: Date;
		busy: boolean;
		onReload: () => void;
		onCopy: () => void;
		onOverwrite: () => void;
	} = $props();

	const theirs = $derived(
		rival.entries === null
			? 'it will not open with this password'
			: `${rival.entries} ${rival.entries === 1 ? 'entry' : 'entries'}`
	);
</script>

<div class="absolute inset-0 z-30 flex animate-fade items-center justify-center bg-canvas/75 px-6">
	<div class="w-full max-w-[520px] animate-rise rounded-md border border-hairline bg-raised p-8">
		<!-- One column, where the mockup draws the icon in a column of its own.
		     Its buttons sit at the edge of the dialog rather than under the text
		     beside the icon, which is a step nobody notices until the labels are
		     long enough to see it. Ours are: the row wrapped, and the last
		     button landed alone under a text block it was not aligned with. -->
		<h2 class="flex items-center gap-3 text-title font-medium tracking-tight text-txt">
			<Icon name="warn" class="h-5 w-5 shrink-0 text-warn" />
			{missing ? 'The vault file is not there any more' : 'The file changed while you were working'}
		</h2>
		<div>
			<div>
				<p class="mt-3 max-w-[52ch] text-base leading-relaxed text-txt2">
					{#if missing}
						It was renamed, moved, deleted, or it is on a disk that is no longer mounted. Everything
						you have done since is still in this window and has not been saved anywhere.
					{:else}
						The vault was written by another program, or by you on another computer. Nothing has
						been saved from this window yet.
					{/if}
				</p>
				<dl class="mt-5 grid grid-cols-2 gap-3 border-y border-hairline py-4">
					<div>
						<dt class="font-mono text-label tracking-label text-txt4 uppercase">This window</dt>
						<dd class="mt-1.5 text-small text-txt">
							{changedAt ? fully(changedAt.toISOString(), now) : 'unchanged'} · {entries}
							{entries === 1 ? 'entry' : 'entries'}
						</dd>
					</div>
					<div>
						<dt class="font-mono text-label tracking-label text-txt4 uppercase">
							The file on disk
						</dt>
						<dd class="mt-1.5 text-small text-txt">
							{#if missing}
								not there
							{:else}
								{fully(rival.modified, now)} · {theirs}
							{/if}
						</dd>
					</div>
				</dl>
				<p class="mt-4 max-w-[52ch] text-fine leading-relaxed text-txt3">
					{#if missing}
						“Put it back” writes the vault where it was, making the file again. “Keep it elsewhere”
						asks where to put it instead and leaves this window as it is, so it can still be put
						back afterwards.
					{:else}
						“Keep both” puts this window's version beside the vault as a file of its own: nothing is
						lost and it can be sorted out later. “Keep mine” writes over the file, and the version
						that is there now goes into the first snapshot beside it.
					{/if}
				</p>
			</div>
		</div>

		<!-- The two that lose nothing on the left, and the one that writes over
		     somebody else's work pushed away from them. If the row ever wraps it
		     wraps there, which is where a reader would put the break themselves.
		     Nothing is pushed away when the file is gone: writing it back where it
		     belongs takes nobody's work, so it is the plain first answer. -->
		<div class="mt-7 flex flex-wrap items-center gap-2">
			{#if missing}
				<button
					type="button"
					onclick={onOverwrite}
					disabled={busy}
					class="h-[46px] rounded-full bg-accent px-6 text-base font-medium text-canvas transition-colors hover:bg-accenthi active:bg-accenthi disabled:cursor-not-allowed"
				>
					Put it back
				</button>
				<button
					type="button"
					onclick={onCopy}
					disabled={busy}
					class="h-[46px] rounded-full border border-hairline px-5 text-base text-txt transition-colors hover:border-txt3 active:bg-raised disabled:cursor-not-allowed"
				>
					Keep it elsewhere
				</button>
			{:else}
				<button
					type="button"
					onclick={onReload}
					disabled={busy}
					class="h-[46px] rounded-full bg-accent px-6 text-base font-medium text-canvas transition-colors hover:bg-accenthi active:bg-accenthi disabled:cursor-not-allowed"
				>
					Take the file on disk
				</button>
				<button
					type="button"
					onclick={onCopy}
					disabled={busy}
					class="h-[46px] rounded-full border border-hairline px-5 text-base text-txt transition-colors hover:border-txt3 active:bg-raised disabled:cursor-not-allowed"
				>
					Keep both
				</button>
				<button
					type="button"
					onclick={onOverwrite}
					disabled={busy}
					class="ml-auto h-[46px] rounded-full px-5 text-base text-danger transition-colors hover:bg-dangerwash active:bg-dangerwash disabled:cursor-not-allowed"
				>
					Keep mine
				</button>
			{/if}
		</div>
	</div>
</div>
