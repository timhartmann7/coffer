<script lang="ts">
	import { leaves, step } from '$lib/popup';
	import Icon from './Icon.svelte';

	/**
	 * One value out of a short list, drawn as the chip the mockup draws.
	 *
	 * Not a native `<select>`: macOS draws that popup in its own colours, which
	 * are not the ones in design.html, and nothing in the window can change
	 * them.
	 *
	 * Which means the keyboard is this component's job rather than the system's.
	 * The list closes when focus leaves the whole control and not when it leaves
	 * the button, because those are the same moment for a reader pressing Tab -
	 * and a list that closed then could only ever be used with a pointer.
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
	let wrapper = $state<HTMLElement>();
	let trigger = $state<HTMLButtonElement>();

	function choose(chosen: number) {
		open = false;
		trigger?.focus();
		if (chosen !== value) onChoose(chosen);
	}

	/** Focus left the button. Whether it left the control is the question. */
	function left(event: FocusEvent) {
		if (leaves(event, wrapper)) open = false;
	}

	function key(event: KeyboardEvent) {
		if (event.key === 'Escape' && open) {
			event.preventDefault();
			open = false;
			trigger?.focus();
			return;
		}
		if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;

		event.preventDefault();
		if (!open) {
			open = true;
			return;
		}
		step(wrapper, 'option', event.key);
	}
</script>

<div bind:this={wrapper} class="relative shrink-0" onfocusout={left}>
	<button
		bind:this={trigger}
		type="button"
		aria-label={label}
		aria-haspopup="listbox"
		aria-expanded={open}
		onclick={() => (open = !open)}
		onkeydown={key}
		class="flex items-center gap-2 rounded-sm border border-hairline bg-surface2 px-3 py-2 font-mono text-fine text-txt transition-colors hover:border-txt4"
	>
		{render(value)}
		<Icon name="chev-d" class="h-3.5 w-3.5 text-txt4" />
	</button>

	{#if open}
		<div
			role="listbox"
			aria-label={label}
			class="absolute top-full right-0 z-10 mt-1 min-w-full origin-top animate-pop overflow-hidden rounded-sm border border-hairline bg-raised"
		>
			{#each choices as choice (choice)}
				<button
					type="button"
					role="option"
					aria-selected={choice === value}
					onkeydown={key}
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
