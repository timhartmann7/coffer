<script lang="ts">
	import Create from '$lib/components/Create.svelte';
	import InCopy from '$lib/components/InCopy.svelte';
	import Settings from '$lib/components/Settings.svelte';
	import Titlebar from '$lib/components/Titlebar.svelte';
	import Unlock from '$lib/components/Unlock.svelte';
	import Vault from '$lib/components/Vault.svelte';
	import { flush } from '$lib/drafts';
	import { lockByHand } from '$lib/locking';
	import {
		chooseDatabase,
		leaveRescue,
		promoteRescue,
		settings as loadSettings,
		status,
		tree
	} from '$lib/ipc';
	import { Presence } from '$lib/presence';
	import { wear } from '$lib/theme';
	import type {
		CopyOf,
		Database,
		Found,
		Group,
		OnDisk,
		Rescued,
		Settings as Chosen,
		Status
	} from '$lib/model';

	let database = $state<Database | null>(null);
	/** A vault Rust found in Coffer's own folder, when nothing was remembered. */
	let found = $state<Found | null>(null);
	/** The key file the next unlock will use, when Rust is holding one. */
	let keyFile = $state<Database | null>(null);
	let root = $state<Group | null>(null);
	let readOnly = $state(false);
	let ready = $state(false);
	let chosen = $state<Chosen | null>(null);
	let showing = $state<'vault' | 'settings' | 'create'>('vault');
	let reason = $state<string | null>(null);
	/** The copy a lock left beside the vault, when there is one. Rust's answer
	 * rather than this window's: a lock destroys the window that would have
	 * known, and a copy an earlier run left is still worth offering. */
	let rescue = $state<Rescued | null>(null);
	/** Whether the last lock had work to write and nowhere at all to put it. */
	let lost = $state(false);
	/** Whether the last lock found what the reader was typing and saved it. */
	let typed = $state(false);
	/** The chosen file as the disk has it, which is what the sentences about
	 * a lock's copy and about lost work are measured against. */
	let file = $state<OnDisk | null>(null);
	/** The vault the chosen file was copied from, when it is a lock's copy. */
	let copy = $state<CopyOf | null>(null);

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
				found = opening.found;
				keyFile = opening.keyFile;
				reason = opening.lockedBy;
				heard(opening);
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
		heard(now);
	}

	/** What Rust says about the files beside the vault, which every reading of
	 * the status brings. */
	function heard(now: Status | null) {
		rescue = now?.rescue ?? null;
		lost = now?.lost ?? false;
		typed = now?.typed ?? false;
		file = now?.file ?? null;
		copy = now?.copy ?? null;
	}

	/**
	 * Makes the open copy the vault it was taken from. The session stays open,
	 * now on the vault, so the window reads again what is true of it: its name,
	 * whether it can be written, and that it is no copy.
	 *
	 * Every value already on its way to Rust is waited for first, so that the
	 * file that goes over the vault holds every field the reader has left.
	 * What is still being typed stays with the session, which is now the
	 * vault's, and the next lock writes it there.
	 *
	 * Read again however it went. Rust holds the press to how the banner last
	 * said the vault's file stood, and refuses when it stands otherwise; the
	 * banner then has to say how it stands now before the reader presses
	 * again.
	 */
	async function promote() {
		await flush();
		try {
			database = await promoteRescue();
		} finally {
			const now = await status();
			readOnly = now.readOnly;
			heard(now);
		}
	}

	/**
	 * Goes back from the copy to its vault. With the copy open that is a lock,
	 * which takes this window with it, so the screen is only cleared once Rust
	 * has answered - the same order `lockByHand` keeps, for the same reason.
	 */
	async function back() {
		await flush();
		database = await leaveRescue();
		root = null;
		showing = 'vault';
		await chosen_elsewhere();
	}

	function lock() {
		return lockByHand(() => {
			root = null;
			showing = 'vault';
		});
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
		if (picked) await pointAt(picked);
	}

	/**
	 * Takes the reader to the unlock screen for a file chosen away from it: in
	 * the settings, or on the creation screen, which offers to open what already
	 * sits where the new vault would have gone.
	 */
	async function pointAt(picked: Database) {
		database = picked;
		// Rust forgot the key file when the session was pointed elsewhere, and
		// the screen has to say the same thing.
		keyFile = null;
		showing = 'vault';
		await chosen_elsewhere();
	}

	/**
	 * Reads back what is true of the file that was just chosen.
	 *
	 * The copy a lock left is a fact about a database and not about this run, so
	 * pointing the session at another file makes what is on the screen about the
	 * wrong one. Only Rust knows whether the new one has a copy beside it.
	 */
	async function chosen_elsewhere() {
		heard(await status().catch(() => null));
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
			onOpen={(picked) => void pointAt(picked)}
		/>
	{:else if root && database}
		{#if copy}
			<InCopy {copy} onPromote={promote} onBack={back} />
		{/if}
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
			{found}
			{keyFile}
			{reason}
			{rescue}
			{lost}
			{file}
			{copy}
			{typed}
			onChoose={(picked) => {
				database = picked;
				void chosen_elsewhere();
			}}
			onKeyFile={(chosen) => (keyFile = chosen)}
			onCreate={() => (showing = 'create')}
			onUnlocked={opened}
		/>
	{/if}
</div>
