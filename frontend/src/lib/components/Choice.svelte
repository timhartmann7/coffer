<script lang="ts">
	import Icon from './Icon.svelte';

	/**
	 * One value out of a short list, drawn as the chip the mockup draws.
	 *
	 * Not a native `<select>`: macOS draws that popup in its own colours, which
	 * are not the ones in design.html, and nothing in the window can change
	 * them.
	 */
	let {
		value,
		choices,
		label,
		render,
		onChoose
	}: {
		value: number;
		choices: number[];
		label: string;
		render: (value: number) => string;
		onChoose: (value: number) => void;
	} = $props();

	let open = $state(false);

	function choose(chosen: number) {
		open = false;
		if (chosen !== value) onChoose(chosen);
	}
</script>

<div class="relative shrink-0">
	<button
		type="button"
		aria-label={label}
		aria-expanded={open}
		onclick={() => (open = !open)}
		onblur={() => (open = false)}
		class="flex items-center gap-2 rounded-sm border border-hairline bg-surface2 px-3 py-2 font-mono text-fine text-txt transition-colors hover:border-txt4"
	>
		{render(value)}
		<Icon name="chev-d" class="h-3.5 w-3.5 text-txt4" />
	</button>

	{#if open}
		<div
			class="absolute top-full right-0 z-10 mt-1 min-w-full overflow-hidden rounded-sm border border-hairline bg-raised"
		>
			{#each choices as choice (choice)}
				<button
					type="button"
					onmousedown={(event) => event.preventDefault()}
					onclick={() => choose(choice)}
					class="block w-full px-3 py-2 text-right font-mono text-fine whitespace-nowrap transition-colors {choice ===
					value
						? 'bg-surface2 text-txt'
						: 'text-txt2 hover:bg-surface2 hover:text-txt'}"
				>
					{render(choice)}
				</button>
			{/each}
		</div>
	{/if}
</div>
