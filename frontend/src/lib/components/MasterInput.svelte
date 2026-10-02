<script lang="ts">
	import { composing } from '$lib/lines';
	import Icon from './Icon.svelte';

	/**
	 * A master password typed into the mockup's field, with its eye.
	 *
	 * Never bound to a value: what is typed stays in the input until the form
	 * around it reads it once, as bytes. The eye shows it only while the focus
	 * is in the field or on the eye; the focus going anywhere else masks it
	 * again, so a password is never left on the screen by a reader who went to
	 * type somewhere else. A field drawn without the eye is always masked.
	 */
	let {
		label,
		input = $bindable(),
		autocomplete,
		wrong = false,
		describedby,
		matched = false,
		eye = true,
		locked = false,
		oninput,
		onkeydown,
		onleave
	}: {
		label: string;
		input?: HTMLInputElement;
		/** `off` for a password the vault already has, `new-password` for the
		 * one it is getting. */
		autocomplete: 'off' | 'new-password';
		/** Whether a refusal was about what is in this field: the mockup's
		 * wrong-password border. */
		wrong?: boolean;
		/** The sentence a refusal about this field is said in, while it is
		 * said, so that the field is read out with it. */
		describedby?: string;
		/** Whether this is the second of two that agree, which draws the
		 * mockup's check. */
		matched?: boolean;
		/** Whether the field has the eye that shows what is typed. Without it
		 * the field is always masked. */
		eye?: boolean;
		/** Read only while what was typed is on its way to Rust. */
		locked?: boolean;
		oninput?: () => void;
		/** A key in the field. An input method's Return never reaches it. */
		onkeydown?: (event: KeyboardEvent) => void;
		/** The focus left the field and its eye. */
		onleave?: () => void;
	} = $props();

	const id = $props.id();
	let box = $state<HTMLElement>();
	let shown = $state(false);

	function left(event: FocusEvent) {
		const next = event.relatedTarget;
		if (next instanceof Node && box?.contains(next)) return;
		shown = false;
		onleave?.();
	}

	/** The Return that confirms a conversion is the input method's, and would
	 * otherwise move on, or send, with half a word typed. */
	function keys(event: KeyboardEvent) {
		if (event.key === 'Enter' && composing(event)) {
			event.preventDefault();
			return;
		}
		onkeydown?.(event);
	}
</script>

<div>
	<label for={id} class="mb-2 block font-mono text-label tracking-label text-txt3 uppercase">
		{label}
	</label>
	<span
		bind:this={box}
		onfocusout={left}
		class="flex items-center gap-2 rounded-sm border bg-surface2 px-3 py-2.5 {wrong
			? 'border-danger/60'
			: 'border-hairline focus-within:border-accent focus-within:ring-4 focus-within:ring-accent/15'} transition"
	>
		<input
			bind:this={input}
			{id}
			type={shown ? 'text' : 'password'}
			{autocomplete}
			readonly={locked}
			aria-invalid={wrong}
			aria-describedby={describedby}
			spellcheck="false"
			autocapitalize="off"
			oninput={() => oninput?.()}
			onkeydown={keys}
			class="min-w-0 flex-1 bg-transparent text-txt outline-none {shown
				? 'font-mono'
				: 'tracking-mask'}"
		/>
		{#if matched}
			<Icon name="check" class="h-4 w-4 shrink-0 text-txt3" />
			<span class="sr-only">The two are the same</span>
		{/if}
		<!-- Pressing it leaves the focus in the field, so the reader goes on
		     typing where they were. Its name is what it does next, the way the
		     eye on a revealed value is named, and it is not also a pressed
		     toggle: "Hide the password, selected" would say two things. -->
		{#if eye}
			<button
				type="button"
				onmousedown={(event) => event.preventDefault()}
				onclick={() => (shown = !shown)}
				aria-label={shown ? 'Hide the password' : 'Show the password'}
				class="shrink-0 text-txt3 transition-colors hover:text-txt2"
			>
				<Icon name={shown ? 'eye-off' : 'eye'} class="h-4 w-4" />
			</button>
		{/if}
	</span>
</div>
