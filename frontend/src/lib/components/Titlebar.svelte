<script lang="ts">
	import Icon from './Icon.svelte';

	let {
		name,
		unlocked = false,
		showing,
		onLock,
		onSettings
	}: {
		name: string;
		unlocked?: boolean;
		/** Which screen the window is on, so the way in to the settings can be
		 * the way back out of them. */
		showing?: 'vault' | 'settings' | 'create';
		onLock?: () => void;
		onSettings?: () => void;
	} = $props();
</script>

<!--
	The window's own title bar, drawn here because the frame is hidden. The left
	gap is where macOS puts its three buttons over the content; `deep` lets a
	drag start anywhere in the bar except on the button, which stays a button.
-->
<header
	data-tauri-drag-region="deep"
	class="flex h-11 shrink-0 items-center border-b border-hairline bg-surface2 px-4"
>
	<div class="flex w-[78px] shrink-0 items-center justify-end">
		{#if onSettings}
			<button
				type="button"
				onclick={onSettings}
				aria-label={showing === 'settings' ? 'Back to the vault' : 'Settings'}
				class="transition-colors {showing === 'settings'
					? 'text-txt'
					: 'text-txt4 hover:text-txt2'}"
			>
				<Icon name="sliders" class="h-4 w-4" />
			</button>
		{/if}
	</div>

	<div
		class="mx-auto flex items-center gap-2 font-mono text-label tracking-label text-txt3 uppercase"
	>
		<Icon
			name={showing === 'settings' ? 'sliders' : unlocked ? 'unlock' : 'lock'}
			class="h-3.5 w-3.5"
		/>
		{name}
	</div>

	{#if unlocked && onLock}
		<button
			type="button"
			onclick={onLock}
			class="flex items-center gap-2 rounded-full border border-hairline px-3 py-1.5 font-mono text-label tracking-label text-txt3 uppercase transition-colors hover:border-txt4 hover:text-txt2"
		>
			<Icon name="lock" class="h-3.5 w-3.5" /> Lock
		</button>
	{:else}
		<div class="w-[78px] shrink-0" aria-hidden="true"></div>
	{/if}
</header>
