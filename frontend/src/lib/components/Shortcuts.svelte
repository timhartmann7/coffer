<script lang="ts">
	import { KEYS, OWN } from '$lib/shortcuts';
	import { COMMANDS, type Command } from '$lib/model';
	import Sheet from './Sheet.svelte';

	/**
	 * Every key Coffer answers, in two lists: the menu bar's items, which is
	 * where a Mac application's keys are looked up, and the keys the window
	 * answers itself and the presses that choose rows in the list, with the
	 * system's Close Window, which locks the vault here. Reached from Help in
	 * the menu bar: nothing on the screens links to it, because the unlock
	 * screen is held to what the spec draws on it.
	 */
	let { onClose }: { onClose: () => void } = $props();

	/** What each item with a key does, in the window's words rather than the
	 * menu's title. */
	const MENU: Record<keyof typeof KEYS, string> = {
		settings: 'Open the settings',
		lock: 'Lock the vault',
		newEntry: 'Make a new entry',
		newFolder: 'Make a new folder',
		duplicate: 'Make a copy of the open entry beside it',
		openVault: 'Open another vault, while this one is locked',
		saveCopy: 'Save a copy of the vault on another disk',
		showInFinder: 'Show the vault’s file in the Finder',
		find: 'Search the list',
		copyLogin: 'Copy the open entry’s login',
		copyPassword: 'Copy the open entry’s password',
		moveToBin: 'Move the selected entries, or the open one, to the Recycle Bin'
	};

	const WINDOW: Record<keyof typeof OWN, string> = {
		copy: 'Copy the open entry’s password, when nothing is selected',
		undo: 'Undo what the notice names, while it offers Undo',
		choose: 'Add an entry in the list to the selection, or take it out',
		reach: 'Select every entry from the last one pressed',
		all: 'Select every entry in the list',
		away: 'Close the settings, clear the search, deselect, then put the open entry away',
		close: 'Close the window, which locks the vault'
	};

	type Row = { key: string; does: string };

	const keyed = (command: Command): command is keyof typeof KEYS => command in KEYS;

	const menu: Row[] = COMMANDS.filter(keyed).map((command) => ({
		key: KEYS[command],
		does: MENU[command]
	}));
	const own: Row[] = (Object.keys(OWN) as (keyof typeof OWN)[]).map((name) => ({
		key: OWN[name],
		does: WINDOW[name]
	}));
</script>

{#snippet list(heading: string, rows: Row[])}
	<section>
		<h3 class="font-mono text-label tracking-label text-txt4 uppercase">{heading}</h3>
		<dl class="mt-2 divide-y divide-line">
			{#each rows as row (row.key)}
				<div class="flex items-center justify-between gap-4 py-2">
					<dt class="text-small text-txt2">{row.does}</dt>
					<dd>
						<kbd
							class="shrink-0 rounded-xs border border-hairline px-1.5 py-0.5 font-mono text-label text-txt4"
						>
							{row.key}
						</kbd>
					</dd>
				</div>
			{/each}
		</dl>
	</section>
{/snippet}

<Sheet title="Keyboard shortcuts" {onClose}>
	<div class="mt-5 grid grid-cols-2 gap-8">
		{@render list('In the menus', menu)}
		{@render list('In the window', own)}
	</div>
	<p class="mt-5 text-fine leading-relaxed text-txt3">
		The first list is the menu bar’s, where anything that does not apply right now is greyed out.
	</p>
	<div class="mt-7 flex">
		<button
			type="button"
			onclick={onClose}
			class="h-9 rounded-full border border-hairline px-5 text-small text-txt transition-colors hover:border-txt3"
		>
			Close
		</button>
	</div>
</Sheet>
