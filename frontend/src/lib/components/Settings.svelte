<script module lang="ts">
	import type { CopyElsewhere, Rekey } from '$lib/saving.svelte';

	/**
	 * What the screen the settings are drawn over hands them: the rows that act
	 * on an open vault, each present only where it can. One object, so that a
	 * row added later is one more field here and not one more argument every
	 * screen in between passes along.
	 */
	export interface Handed {
		/** Gives the vault a new master password. Handed only while a vault
		 * Coffer can write is open: with nothing open, or a snapshot or a file
		 * Coffer reads and does not write, there is no password to change from
		 * here. Nor in the copy a lock left, until it is made the vault: a
		 * password given to the copy alone would leave the vault and its
		 * snapshots opening with the old one. */
		rekey?: Rekey;
		/** Saves a copy of the vault on another disk. Handed only while a vault
		 * Coffer keeps copies of is open: not a backup or the copy a lock left,
		 * whose copies would not be the vault's, and not a format Coffer will
		 * not write anywhere. It says in a notice where the copy went, and
		 * throws a refusal back to be said here. */
		onCopy?: CopyElsewhere;
		/** Whether a copy of the vault is on its way, however it was asked for:
		 * from here, the status bar or the menu bar. The screen this is drawn
		 * over holds the one answer, so a copy started there before the
		 * settings opened greys the button here too. */
		copying?: boolean;
	}
</script>

<script lang="ts">
	import { lastMade, nearby, refused } from '$lib/copies';
	import { asFailure, setSettings, showInFinder, snapshots } from '$lib/ipc';
	import { named } from '$lib/duration';
	import { answer } from '$lib/menu.svelte';
	import { LOOKS } from '$lib/theme';
	import type { Database, Elsewhere, Settings, Snapshot } from '$lib/model';
	import Backups from './Backups.svelte';
	import Choice from './Choice.svelte';
	import Icon from './Icon.svelte';
	import MasterPassword from './MasterPassword.svelte';
	import Segmented from './Segmented.svelte';
	import Toggle from './Toggle.svelte';

	/**
	 * One screen, no tabs, and no button that applies anything: the switch and
	 * the chip are the action.
	 *
	 * Every value shown is the one Rust stored, not the one that was clicked.
	 * They are usually the same, and when they are not it is because Rust
	 * settled the choice onto something it offers, and the screen has to say so
	 * rather than draw a value the file does not hold.
	 */
	let {
		settings,
		database,
		onSettings,
		onChoose,
		onLook,
		rekey,
		onCopy,
		copying = false,
		elsewhere = null,
		copyBeside = false
	}: Handed & {
		settings: Settings;
		database: Database | null;
		/** What Rust knows of copies of the open vault kept on another disk.
		 * Null while nothing Coffer keeps copies of is open, and then no row
		 * is drawn for them. */
		elsewhere?: Elsewhere | null;
		onSettings: (settings: Settings) => void;
		/**
		 * Whether the copy a lock left sits beside the open vault. It opens with
		 * the password the vault has now, so a new one waits until the copy is
		 * made the vault or removed, and the row says so rather than offering a
		 * change Rust would refuse.
		 */
		copyBeside?: boolean;
		/**
		 * Asks for another vault. It rejects while one is open, and the refusal
		 * says what to do about it, so it is shown rather than swallowed: a
		 * button that answers a press with nothing at all is a button the reader
		 * presses again.
		 */
		onChoose: () => Promise<void>;
		/**
		 * Opens the backup the list showed at this slot, to look at. With a
		 * vault open that is a lock first, which takes this window with it. A
		 * refusal is thrown back to the list.
		 */
		onLook: (index: number) => Promise<void>;
	} = $props();

	let failure = $state<string | null>(null);

	/** The backups beside the chosen vault, as Rust last listed them. */
	let backups = $state<Snapshot[]>([]);
	/** Whether they are drawn under their row. */
	let showing = $state(false);

	/** How many there are, in the words the row says it in. */
	const counted = $derived(
		backups.length === 0 ? 'None yet' : backups.length === 1 ? '1 copy' : `${backups.length} copies`
	);

	/** The file name of the one open now, when it is one of them. */
	const openNow = $derived(database?.path.split('/').at(-1) ?? null);

	/** Reads the backups beside the chosen vault again. A list that cannot be
	 * read is said where every other refusal here is. */
	async function listBackups() {
		try {
			backups = await snapshots();
		} catch (thrown) {
			backups = [];
			failure = asFailure(thrown).message;
		}
	}

	// Read whenever the settings open, and again for another vault: a save
	// since the last reading is another backup.
	$effect(() => {
		backups = [];
		if (database) void listBackups();
	});

	async function pick() {
		failure = null;
		try {
			await onChoose();
		} catch (thrown) {
			failure = asFailure(thrown).message;
		}
	}

	// Open Vault… in the menu bar is "Open another", refusal and all. Rust greys
	// the item out while a vault is open, which is when it would be refused.
	$effect(() => answer({ openVault: { run: () => void pick() } }));

	/** What the row about copies says, written against the moment Rust last
	 * answered about them, so that a copy just saved is "today". */
	const made = $derived(elsewhere ? lastMade(elsewhere, new Date()) : '');
	const near = $derived(elsewhere ? nearby(elsewhere, new Date()) : null);

	/** Shows the chosen vault's file in the Finder. A file that is not there
	 * any more is said where every other refusal here is. */
	async function show() {
		failure = null;
		try {
			await showInFinder();
		} catch (thrown) {
			failure = asFailure(thrown).message;
		}
	}

	/** Saves a copy on another disk, once however quickly the button is
	 * pressed again. What Rust then knows reaches the row through the screen
	 * this is drawn over, which also says in a notice where the copy went; a
	 * refusal is said here. */
	async function copy() {
		if (!onCopy || copying) return;
		failure = null;
		try {
			await onCopy();
		} catch (thrown) {
			failure = refused(thrown);
		}
	}

	// File ▸ Show in Finder and Save a Copy… are the two buttons here while the
	// settings are open, on the condition each is drawn on.
	$effect(() =>
		answer({
			showInFinder: { run: () => void show(), when: () => database !== null },
			saveCopy: {
				run: () => void copy(),
				when: () => elsewhere !== null && onCopy !== undefined && !copying
			}
		})
	);

	async function change(wanted: Settings) {
		failure = null;
		try {
			onSettings(await setSettings(wanted));
		} catch (thrown) {
			failure = asFailure(thrown).message;
		}
	}
</script>

<div class="flex flex-1 flex-col overflow-y-auto">
	<div class="divide-y divide-line">
		<div class="flex items-center justify-between gap-6 px-8 py-5">
			<div>
				<div class="text-row text-txt">Lock after doing nothing</div>
				<div class="mt-1 text-fine text-txt3">
					The window is built again and the tree is wiped out of memory
				</div>
			</div>
			<Choice
				value={settings.idleSeconds}
				choices={settings.idleChoices}
				label="How long an untouched vault stays open"
				render={named}
				onChoose={(idleSeconds) => change({ ...settings, idleSeconds })}
			/>
		</div>

		<div class="flex items-center justify-between gap-6 px-8 py-5">
			<div class="text-row text-txt">Lock when this Mac goes to sleep</div>
			<Toggle
				on={settings.lockOnSleep}
				label="Lock when this Mac goes to sleep"
				onChange={(lockOnSleep) => change({ ...settings, lockOnSleep })}
			/>
		</div>

		<div class="flex items-center justify-between gap-6 px-8 py-5">
			<div class="text-row text-txt">Lock when the screen locks</div>
			<Toggle
				on={settings.lockOnScreenLock}
				label="Lock when the screen locks"
				onChange={(lockOnScreenLock) => change({ ...settings, lockOnScreenLock })}
			/>
		</div>

		<div class="flex items-center justify-between gap-6 px-8 py-5">
			<div>
				<div class="text-row text-txt">Clear the clipboard</div>
				<div class="mt-1 text-fine text-txt3">
					A copied value is marked concealed, so clipboard history tools skip it
				</div>
			</div>
			<Choice
				value={settings.clipboardSeconds}
				choices={settings.clipboardChoices}
				label="How long a copied password stays on the clipboard"
				render={named}
				onChoose={(clipboardSeconds) => change({ ...settings, clipboardSeconds })}
			/>
		</div>

		<div class="flex items-center justify-between gap-6 px-8 py-5">
			<div class="text-row text-txt">Theme</div>
			<Segmented
				value={settings.theme}
				choices={settings.themeChoices}
				label="How the window is drawn"
				render={(theme) => LOOKS[theme]}
				onChoose={(theme) => change({ ...settings, theme })}
			/>
		</div>

		<div class="flex items-center justify-between gap-6 px-8 py-5">
			<div class="min-w-0">
				<div class="text-row text-txt">Vault</div>
				<div class="mt-1 truncate font-mono text-fine text-txt3">
					{database ? database.path : 'None chosen yet'}
				</div>
			</div>
			<div class="flex shrink-0 gap-3">
				{#if database}
					<button
						type="button"
						onclick={show}
						class="h-9 shrink-0 rounded-full border border-hairline px-4 text-small text-txt2 transition-colors hover:border-txt4 hover:text-txt active:bg-raised"
					>
						Show in Finder
					</button>
				{/if}
				<button
					type="button"
					onclick={pick}
					class="h-9 shrink-0 rounded-full border border-hairline px-4 text-small text-txt2 transition-colors hover:border-txt4 hover:text-txt active:bg-raised"
				>
					Open another
				</button>
			</div>
		</div>

		<!-- The password row writes the vault, which moves the backups: a new
		     password is a save, and old backups removed are gone. The list is
		     read again after either, so the count and the rows are the disk's. -->
		{#if rekey}
			<MasterPassword {rekey} {copyBeside} onBackups={listBackups} />
		{/if}

		<!-- Above the automatic backups, so that the two kinds of copy are read
		     together: those sit beside the vault, and this one is on another
		     disk. -->
		{#if elsewhere && onCopy}
			<div class="flex items-center justify-between gap-6 px-8 py-5">
				<div class="min-w-0">
					<div class="text-row text-txt">Copy on another disk</div>
					<div class="mt-1 text-fine text-txt3">{made}</div>
					{#if near}
						<div class="mt-1 text-fine text-txt3">{near}</div>
					{/if}
				</div>
				<button
					type="button"
					onclick={copy}
					disabled={copying}
					class="h-9 shrink-0 rounded-full border border-hairline px-4 text-small text-txt2 transition-colors hover:border-txt4 hover:text-txt active:bg-raised disabled:cursor-not-allowed disabled:text-txt4 disabled:hover:border-hairline"
				>
					{copying ? 'Saving a copy…' : 'Save a copy…'}
				</button>
			</div>
		{/if}

		<div class="flex items-center justify-between gap-6 px-8 py-5">
			<div>
				<div class="text-row text-txt">Automatic backups</div>
				<div class="mt-1 text-fine text-txt3">
					Before every save, the vault as it was is kept beside it. The ten newest stay.
				</div>
			</div>
			<div class="flex shrink-0 items-center gap-3">
				<span class="font-mono text-fine text-txt3">{counted}</span>
				{#if backups.length > 0 || showing}
					<button
						type="button"
						aria-expanded={showing}
						aria-controls="backups"
						onclick={() => (showing = !showing)}
						class="h-9 shrink-0 rounded-full border border-hairline px-4 text-small text-txt2 transition-colors hover:border-txt4 hover:text-txt active:bg-raised"
					>
						{showing ? 'Hide' : 'Show'}
					</button>
				{/if}
			</div>
		</div>

		{#if showing}
			<div id="backups" class="px-8 pb-4">
				<Backups
					{backups}
					act="Open to look"
					open={openNow}
					onOpen={onLook}
					onAgain={listBackups}
				/>
			</div>
		{/if}
	</div>

	{#if failure}
		<div class="flex items-center gap-3 border-t border-line px-8 py-4">
			<Icon name="warn" class="h-4 w-4 shrink-0 text-warn" />
			<span class="text-small text-txt2">{failure}</span>
		</div>
	{/if}

	<div class="mt-auto border-t border-hairline bg-surface2 px-8 py-4">
		<p class="text-fine leading-relaxed text-txt3">
			Argon2id parameters are settled when a vault is made and are not changed here. Somebody else's
			vault always opens with its own.
		</p>
		<p class="mt-2 font-mono text-label text-txt4">
			coffer {__COFFER_VERSION__} · kdbx 4.1 · network requests: 0
		</p>
	</div>
</div>
