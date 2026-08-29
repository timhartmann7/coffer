<script lang="ts">
	import Titlebar from '$lib/components/Titlebar.svelte';
	import Unlock from '$lib/components/Unlock.svelte';
	import Vault from '$lib/components/Vault.svelte';
	import { lock as lockVault, status, tree } from '$lib/ipc';
	import type { Database, Group } from '$lib/model';

	let database = $state<Database | null>(null);
	let root = $state<Group | null>(null);
	let ready = $state(false);

	// The window opens locked. Nothing is asked of the vault until a password
	// has opened it.
	$effect(() => {
		void (async () => {
			try {
				const opening = await status();
				database = opening.database;
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
		database = (await status()).database;
	}

	async function lock() {
		await lockVault();
		root = null;
	}
</script>

<div class="flex h-full flex-col">
	<Titlebar
		name={root && database ? database.name : 'Coffer'}
		unlocked={root !== null}
		onLock={root !== null ? lock : undefined}
	/>

	{#if root && database}
		<Vault {database} {root} />
	{:else if ready}
		<Unlock {database} onChoose={(picked) => (database = picked)} onUnlocked={opened} />
	{/if}
</div>
