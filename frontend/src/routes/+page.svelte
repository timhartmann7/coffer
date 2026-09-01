<script lang="ts">
	import Create from '$lib/components/Create.svelte';
	import Settings from '$lib/components/Settings.svelte';
	import Titlebar from '$lib/components/Titlebar.svelte';
	import Unlock from '$lib/components/Unlock.svelte';
	import Vault from '$lib/components/Vault.svelte';
	import {
		chooseDatabase,
		lock as lockVault,
		settings as loadSettings,
		status,
		tree
	} from '$lib/ipc';
	import { Presence } from '$lib/presence';
	import { wear } from '$lib/theme';
	import type { Database, Group, Settings as Chosen } from '$lib/model';

	let database = $state<Database | null>(null);
	/** The key file the next unlock will use, when Rust is holding one. */
	let keyFile = $state<Database | null>(null);
	let root = $state<Group | null>(null);
	let readOnly = $state(false);
	let ready = $state(false);
	let chosen = $state<Chosen | null>(null);
	let showing = $state<'vault' | 'settings' | 'create'>('vault');
	let reason = $state<string | null>(null);

	/** The throttle on telling Rust that somebody is at the machine. */
	const presence = new Presence();

	// The window opens locked. Nothing is asked of the vault until a password
	// has opened it.
	//
	// Everything the reader chose is asked for here as well, on every boot,
	// because locking destroys this window: the whole JavaScript realm goes with
	// it, so nothing may be cached across a lock and there is no module-level
	// value to go stale.
	$effect(() => {
		void (async () => {
			try {
				const opening = await status();
				database = opening.database;
				keyFile = opening.keyFile;
				reason = opening.lockedBy;
				chosen = await loadSettings();
				if (opening.unlocked) await opened();
			} finally {
				// Whatever went wrong, the unlock screen is where it can be said
				// and where the reader can choose another file. A window with
				// nothing in it says nothing.
				ready = true;
			}
		})();
	});

	// Worn before the settings arrive as well as after. Rust built this window in
	// the look the reader chose, so what the Mac reports is already their answer
	// until the message lands, and there is no dark frame in front of a light
	// window.
	$effect(() => wear(chosen?.theme ?? 'system'));

	/** Runs inside the unlock screen's own attempt, so that a vault which opens
	 * and then will not answer is reported where the reader is looking. */
	async function opened() {
		root = await tree();
		const now = await status();
		database = now.database;
		readOnly = now.readOnly;
		reason = null;
	}

	/**
	 * Locking destroys this window, so nothing after the call is guaranteed to
	 * run. The state is cleared first for the case where it does: a window that
	 * outlived its own lock would go on drawing a tree that is no longer in
	 * memory.
	 */
	async function lock() {
		root = null;
		showing = 'vault';
		await lockVault();
	}

	/** The way in to the settings is the way back out of them, wherever it is
	 * drawn. */
	function toggleSettings() {
		showing = showing === 'settings' ? 'vault' : 'settings';
	}

	/**
	 * The settings screen offers another vault, which is the unlock screen's
	 * picker under another name.
	 *
	 * The refusal is left to reach the screen. Rust will not point the session
	 * at another file while one is open - the tree of the first would still be
	 * in memory - and it says so in a sentence that names the way through. A
	 * caught refusal made the offer a button that did nothing whatever.
	 */
	async function choose() {
		const picked = await chooseDatabase();
		if (picked) {
			database = picked;
			// Rust forgot the key file when the session was pointed elsewhere,
			// and the screen has to say the same thing.
			keyFile = null;
			showing = 'vault';
		}
	}
</script>

<!--
	What counts as the reader being there. Deliberately not every event a browser
	has: a scroll or a mouse move can happen without anybody in the room, and a
	deadline they reset would be a vault that never locks itself.
-->
<svelte:window
	onkeydown={() => presence.stir()}
	onpointerdown={() => presence.stir()}
	onwheel={() => presence.stir()}
/>

{#snippet settingsScreen()}
	{#if chosen}
		<Settings
			settings={chosen}
			{database}
			onSettings={(settled) => (chosen = settled)}
			onChoose={choose}
		/>
	{/if}
{/snippet}

<div class="flex h-full flex-col">
	<Titlebar
		name={showing === 'settings'
			? 'Settings'
			: showing === 'create'
				? 'A new vault'
				: root && database
					? database.name
					: 'Coffer'}
		unlocked={root !== null}
		{showing}
		onLock={root !== null ? lock : undefined}
		onSettings={chosen && showing !== 'create' && !(root && database) ? toggleSettings : undefined}
	/>

	{#if showing === 'create'}
		<Create
			onMade={async () => {
				showing = 'vault';
				await opened();
			}}
			onCancel={() => (showing = 'vault')}
		/>
	{:else if root && database}
		<!-- An open vault keeps the screen, and the settings go over it. There is
		     one way in and out of them while a vault is open, it is in the status
		     bar, and it does not move when it is pressed. -->
		<Vault
			{database}
			{root}
			{readOnly}
			settings={showing === 'settings' ? settingsScreen : undefined}
			onSettings={toggleSettings}
			onTree={(tree) => (root = tree)}
		/>
	{:else if showing === 'settings' && chosen}
		<!-- Nothing is open, so there is nothing to lay them over: the way in was
		     the title bar, and that is where it stays. -->
		{@render settingsScreen()}
	{:else if ready}
		<Unlock
			{database}
			{keyFile}
			{reason}
			onChoose={(picked) => (database = picked)}
			onKeyFile={(chosen) => (keyFile = chosen)}
			onCreate={() => (showing = 'create')}
			onUnlocked={opened}
		/>
	{/if}
</div>
