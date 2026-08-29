<script lang="ts">
	/**
	 * A setting that is on or off. The switch is the action: there is no button
	 * that applies it afterwards, which is why the accent lives in the on state.
	 *
	 * The mockup draws only the on state, so the off state is derived from the
	 * same tokens: the track loses the accent for the line colour and the knob
	 * moves to the other end and dims.
	 */
	let { on, label, onChange }: { on: boolean; label: string; onChange: (on: boolean) => void } =
		$props();
</script>

<button
	type="button"
	role="switch"
	aria-checked={on}
	aria-label={label}
	onclick={() => onChange(!on)}
	class="relative h-5 w-9 shrink-0 rounded-full transition-colors {on
		? 'bg-accent'
		: 'bg-line hover:bg-hairline'}"
>
	<span
		class="absolute top-[3px] h-3.5 w-3.5 rounded-full transition-all {on
			? 'right-[3px] bg-canvas'
			: 'left-[3px] bg-txt4'}"
		aria-hidden="true"
	></span>
</button>
