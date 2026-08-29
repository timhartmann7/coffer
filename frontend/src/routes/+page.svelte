<script lang="ts">
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
	import type { Database, Group, Settings as Chosen } from '$lib/model';

	let database = $state<Database | null>(null);
	let root = $state<Group | null>(null);
	let readOnly = $state(false);
	let ready = $state(false);
	let chosen = $state<Chosen | null>(null);
	let showing = $state<'vault' | 'settings'>('vault');

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

	/** Runs inside the unlock screen's own attempt, so that a vault which opens
	 * and then will not answer is reported where the reader is looking. */
	async function opened() {
		root = await tree();
		const now = await status();
		database = now.database;
		readOnly = now.readOnly;
	}

	async function lock() {
		await lockVault();
		root = null;
		showing = 'vault';
	}

	/** The settings screen offers another vault, which is the unlock screen's
	 * picker under another name. */
	async function choose() {
		const picked = await chooseDatabase().catch(() => null);
		if (picked) {
			database = picked;
			showing = 'vault';
		}
	}
</script>

<div class="flex h-full flex-col">
	<Titlebar
		name={showing === 'settings' ? 'Settings' : root && database ? database.name : 'Coffer'}
		unlocked={root !== null}
		{showing}
		onLock={root !== null ? lock : undefined}
		onSettings={chosen
			? () => (showing = showing === 'settings' ? 'vault' : 'settings')
			: undefined}
	/>

	{#if showing === 'settings' && chosen}
		<Settings
			settings={chosen}
			{database}
			onSettings={(settled) => (chosen = settled)}
			onChoose={choose}
		/>
	{:else if root && database}
		<Vault {database} {root} {readOnly} onTree={(tree) => (root = tree)} />
	{:else if ready}
		<Unlock {database} onChoose={(picked) => (database = picked)} onUnlocked={opened} />
	{/if}
</div>
