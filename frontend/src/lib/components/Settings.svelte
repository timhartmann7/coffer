<script lang="ts">
	import { asFailure, setSettings } from '$lib/ipc';
	import { named } from '$lib/duration';
	import { LOOKS } from '$lib/theme';
	import type { Database, Settings } from '$lib/model';
	import Choice from './Choice.svelte';
	import Icon from './Icon.svelte';
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
		onChoose
	}: {
		settings: Settings;
		database: Database | null;
		onSettings: (settings: Settings) => void;
		/**
		 * Asks for another vault. It rejects while one is open, and the refusal
		 * says what to do about it, so it is shown rather than swallowed: a
		 * button that answers a press with nothing at all is a button the reader
		 * presses again.
		 */
		onChoose: () => Promise<void>;
	} = $props();

	let failure = $state<string | null>(null);

	async function pick() {
		failure = null;
		try {
			await onChoose();
		} catch (thrown) {
			failure = asFailure(thrown).message;
		}
	}

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
			<button
				type="button"
				onclick={pick}
				class="h-9 shrink-0 rounded-full border border-hairline px-4 text-small text-txt2 transition-colors hover:border-txt4 hover:text-txt active:bg-raised"
			>
				Open another
			</button>
		</div>

		<div class="flex items-center justify-between gap-6 px-8 py-5">
			<div>
				<div class="text-row text-txt">Snapshots before a write</div>
				<div class="mt-1 text-fine text-txt3">
					Ten copies beside the vault; the oldest is pushed out
				</div>
			</div>
			<span class="shrink-0 font-mono text-fine text-txt3">
				{database ? `${database.name}.1–10.bak` : '1–10.bak'}
			</span>
		</div>
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
