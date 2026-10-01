<script lang="ts">
	import { sealed } from '$lib/guard';
	import { generatePassword } from '$lib/ipc';
	import type { Alphabet } from '$lib/model';
	import { place } from '$lib/reveal.svelte';
	import Icon from './Icon.svelte';

	/**
	 * Length and character sets, and nothing else.
	 *
	 * The password itself is never held here. It arrives from Rust, goes
	 * straight into the node that shows it, and goes from there into the field
	 * it was made for or nowhere at all - the same rule a revealed value lives
	 * by, for the same reason.
	 */
	let {
		onInsert,
		onClose,
		onFailure
	}: {
		onInsert: (value: string) => void;
		onClose: () => void;
		onFailure: (thrown: unknown) => void;
	} = $props();

	/** The range the engine accepts. It clamps anything else, and `docs/ipc.md`
	 * records that the two are written down twice. */
	const SHORTEST = 8;
	const LONGEST = 64;

	const SETS: { key: Alphabet; label: string }[] = [
		{ key: 'lower', label: 'a–z' },
		{ key: 'upper', label: 'A–Z' },
		{ key: 'digits', label: '0–9' },
		{ key: 'symbols', label: '!@#$%' }
	];

	let length = $state(24);
	let chosen = $state<Alphabet[]>(['lower', 'upper', 'digits']);
	let similar = $state(false);
	let node = $state<HTMLElement>();
	let made = $state(false);

	function toggle(key: Alphabet) {
		chosen = chosen.includes(key) ? chosen.filter((set) => set !== key) : [...chosen, key];
		void draw();
	}

	async function draw() {
		if (!node) return;
		try {
			place(node, await generatePassword(length, chosen, similar));
			made = true;
		} catch (thrown) {
			place(node, '');
			made = false;
			onFailure(thrown);
		}
	}

	function insert() {
		if (!node?.textContent) return;
		onInsert(node.textContent);
		wipe();
		onClose();
	}

	/** Nothing leaves this panel behind: closing it takes the value off the
	 * screen the same way the reveal timer does. */
	function wipe() {
		if (node) place(node, '');
		made = false;
	}

	$effect(() => {
		void draw();
		return wipe;
	});

	/**
	 * The system's copy, which stops here.
	 *
	 * A made password is not in the vault yet, so Rust has nothing to copy by
	 * name, and the system's own copy is the ordinary pasteboard write the rest
	 * of the window is kept away from. Once it is in the field, Copy takes it
	 * from there the way every other copy goes.
	 */
	function refused() {
		onFailure({
			code: 'refused',
			message: 'Put it in the field first, and copy it from there.'
		});
	}
</script>

<div class="animate-fade overflow-hidden rounded-md border border-hairline bg-surface">
	<div class="flex h-11 shrink-0 items-center border-b border-hairline bg-surface2 px-4">
		<span class="font-mono text-label tracking-label text-txt3 uppercase">Password generator</span>
		<button
			type="button"
			onclick={onClose}
			class="ml-auto text-txt4 transition-colors hover:text-txt2"
			aria-label="Close the generator"
		>
			<Icon name="x" class="h-4 w-4" />
		</button>
	</div>

	<div class="p-6">
		<div class="flex items-center gap-3 rounded-sm border border-hairline bg-surface2 px-4 py-3.5">
			<!-- The value lives here and nowhere else. -->
			<span
				bind:this={node}
				data-value
				{@attach sealed(refused)}
				class="min-w-0 flex-1 font-mono text-base break-all text-txt"
			></span>
			<button
				type="button"
				onclick={draw}
				class="shrink-0 text-txt3 transition-colors hover:text-txt"
				aria-label="Make another one"
			>
				<Icon name="refresh" class="h-4 w-4" />
			</button>
		</div>

		<div class="mt-6">
			<div class="flex items-center justify-between">
				<label
					for="generator-length"
					class="font-mono text-label tracking-label text-txt3 uppercase">Length</label
				>
				<span class="font-mono text-fine text-txt">{length}</span>
			</div>
			<input
				id="generator-length"
				type="range"
				min={SHORTEST}
				max={LONGEST}
				bind:value={length}
				onchange={draw}
				class="mt-3 w-full"
			/>
			<div class="mt-2 flex justify-between font-mono text-label text-txt4">
				<span>{SHORTEST}</span><span>{LONGEST}</span>
			</div>
		</div>

		<div class="mt-6 flex flex-wrap gap-2">
			{#each SETS as set (set.key)}
				{@const on = chosen.includes(set.key)}
				<button
					type="button"
					onclick={() => toggle(set.key)}
					aria-pressed={on}
					class="flex items-center gap-1.5 rounded-full border border-hairline px-3 py-1.5 font-mono text-meta transition-colors {on
						? 'bg-surface2 text-txt'
						: 'text-txt4 hover:text-txt3'}"
				>
					{#if on}
						<Icon name="check" class="h-3.5 w-3.5 text-txt2" />
					{/if}
					{set.label}
				</button>
			{/each}
			<button
				type="button"
				onclick={() => {
					similar = !similar;
					void draw();
				}}
				aria-pressed={similar}
				class="flex items-center gap-1.5 rounded-full border border-hairline px-3 py-1.5 font-mono text-meta transition-colors {similar
					? 'bg-surface2 text-txt'
					: 'text-txt4 hover:text-txt3'}"
			>
				{#if similar}
					<Icon name="check" class="h-3.5 w-3.5 text-txt2" />
				{/if}
				Look-alike characters
			</button>
		</div>

		<div class="mt-8 flex items-center justify-between gap-4">
			<span class="font-mono text-label leading-relaxed text-txt4">
				Randomness comes from OsRng<br />There are no “memorable” passwords
			</span>
			<button
				type="button"
				onclick={insert}
				disabled={!made}
				class="h-[46px] shrink-0 rounded-full px-6 text-base font-medium transition-colors {made
					? 'bg-accent text-canvas hover:bg-accenthi active:bg-accenthi'
					: 'cursor-not-allowed bg-surface2 text-txt4'}"
			>
				Put it in the field
			</button>
		</div>
	</div>
</div>
