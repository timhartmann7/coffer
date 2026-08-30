<script lang="ts" generics="T extends string">
	/**
	 * One value out of a short list, with every answer on the screen at once,
	 * drawn as the pill the mockup draws for the theme.
	 *
	 * Not {@link Choice}, which opens a list: three looks are few enough to show,
	 * and the one that is lit is the answer to what the window is wearing.
	 *
	 * Every segment is a button of its own, so Tab reaches each of them and there
	 * is no roving focus to keep. The popup is what made the keyboard Choice's
	 * job, and there is no popup here.
	 */
	let {
		value,
		choices,
		label,
		render,
		onChoose
	}: {
		value: T;
		choices: T[];
		label: string;
		render: (value: T) => string;
		onChoose: (value: T) => void;
	} = $props();
</script>

<div
	role="group"
	aria-label={label}
	class="flex shrink-0 gap-1 rounded-full border border-hairline bg-surface2 p-1 font-mono text-meta tracking-label uppercase"
>
	{#each choices as choice (choice)}
		<button
			type="button"
			aria-pressed={choice === value}
			onclick={() => choice !== value && onChoose(choice)}
			class="rounded-full px-3 py-1.5 transition-colors {choice === value
				? 'bg-raised text-txt'
				: 'text-txt4 hover:text-txt2'}"
		>
			{render(choice)}
		</button>
	{/each}
</div>
