<script lang="ts">
	import { untrack } from 'svelte';
	import { sealed } from '$lib/guard';
	import { generatePassword, generator } from '$lib/ipc';
	import type { Alphabet, Generator, Purpose } from '$lib/model';
	import { place } from '$lib/reveal.svelte';
	import Field from './Field.svelte';
	import Icon from './Icon.svelte';

	/**
	 * Length and character sets, and nothing else.
	 *
	 * The password itself is never held here. It arrives from Rust, goes
	 * straight into the node that shows it, and goes from there into the field
	 * it was made for or nowhere at all - the same rule a revealed value lives
	 * by, for the same reason.
	 *
	 * What the generator was last asked for is Rust's to remember, because a
	 * lock destroys this window and a reader who set it up for their bank's
	 * rules should not set it up again after every one. So are the lengths the
	 * slider runs between and the characters two of its switches stand for: a
	 * copy of those written down here was a second answer to what the engine
	 * makes, and a PIN of four digits is the engine's to allow, not the slider's.
	 */
	let {
		purpose,
		label,
		onInsert,
		onClose,
		onFailure
	}: {
		/** Which generator this is: each remembers its own recipe. */
		purpose: Purpose;
		/** What the panel is called to anything that reads the screen aloud.
		 * Two can be open at once, and a "Put it in the field" that does not
		 * say which field is a guess. */
		label: string;
		onInsert: (value: string) => void;
		onClose: () => void;
		onFailure: (thrown: unknown) => void;
	} = $props();

	/** The password's generator and a field's can be open at once, and a label
	 * pointing at an id the other one also has names the wrong slider. */
	const id = $props.id();

	const SETS: { key: Alphabet; label: string }[] = [
		{ key: 'lower', label: 'a–z' },
		{ key: 'upper', label: 'A–Z' },
		{ key: 'digits', label: '0–9' },
		{ key: 'symbols', label: 'Symbols' }
	];

	/** What a sentence calls one character of each kind. */
	const ONE: Record<Alphabet, string> = {
		lower: 'lowercase letter',
		upper: 'capital letter',
		digits: 'digit',
		symbols: 'symbol'
	};

	/** The generator as Rust last drew it, or nothing until it has answered. */
	let drawn = $state<Generator | null>(null);
	let length = $state(0);
	let chosen = $state<Alphabet[]>([]);
	let similar = $state(false);
	let avoid = $state('');
	/** The kinds asked for that the password on the screen happens to lack. */
	let missing = $state<Alphabet[]>([]);
	/** How many passwords have been put on the screen. The sentence about a
	 * kind missing is drawn afresh for each, so a second password that lacks
	 * a digit too is said again rather than met with silence. */
	let told = $state(0);
	let node = $state<HTMLElement>();
	let made = $state(false);
	/**
	 * Which password is the one asked for last.
	 *
	 * A switch pressed while the slider's password is on its way asks again,
	 * and the two can come back in either order. Only the last asked for is
	 * put on the screen, so the password showing is always the one the
	 * switches describe. Nothing is drawn from it, so it is not state.
	 */
	let asked = 0;

	/** Starts from the recipe Rust remembers, then makes the first password. */
	async function open() {
		try {
			const found = await generator(purpose);
			drawn = found;
			length = found.recipe.length;
			chosen = found.recipe.alphabets;
			similar = found.recipe.similar;
			avoid = found.recipe.avoid;
		} catch (thrown) {
			onFailure(thrown);
			return;
		}
		await draw();
	}

	function toggle(key: Alphabet) {
		chosen = chosen.includes(key) ? chosen.filter((set) => set !== key) : [...chosen, key];
		void draw();
	}

	/**
	 * Makes a password from what the switches say.
	 *
	 * The length comes back as Rust settled it: digits and letters cannot be
	 * as short as a PIN, so leaving the PIN's four digits for a password moves
	 * the slider to the shortest password there is, rather than leaving it
	 * showing a length the password does not have.
	 */
	async function draw() {
		// Nothing to make it of until Rust has said what was asked for last: a
		// press before then would ask for a password of nothing, and the
		// refusal would be about a recipe the reader never chose.
		if (!node || !drawn) return;
		const ask = ++asked;
		try {
			const answer = await generatePassword({ length, alphabets: chosen, similar, avoid }, purpose);
			if (ask !== asked || !node) return;
			place(node, answer.value);
			made = true;
			missing = answer.missing;
			told += 1;
			drawn = answer.generator;
			length = answer.generator.recipe.length;
		} catch (thrown) {
			if (ask !== asked) return;
			if (node) place(node, '');
			made = false;
			missing = [];
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
	 * screen the same way the reveal timer does, and an answer still on its way
	 * finds nowhere to go. */
	function wipe() {
		asked += 1;
		if (node) place(node, '');
		made = false;
		missing = [];
	}

	$effect(() => {
		if (!node) return;
		// Opening reads and writes the panel's own state. Untracked, so that
		// nothing it touches - a failure said, say - is taken for something
		// this effect depends on and opens the panel again.
		untrack(() => void open());
		return wipe;
	});

	/** "digit", "digit or symbol", "capital letter, digit or symbol". */
	function listed(kinds: Alphabet[]): string {
		const words = kinds.map((kind) => ONE[kind]);
		const last = words.pop();
		return words.length === 0 ? (last ?? '') : `${words.join(', ')} or ${last}`;
	}

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

<div
	role="group"
	aria-label={label}
	class="animate-fade overflow-hidden rounded-md border border-hairline bg-surface"
>
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
				{@attach sealed(refused, onFailure)}
				class="min-w-0 flex-1 font-mono text-base break-all text-txt"
			></span>
			<button
				type="button"
				onclick={draw}
				disabled={!drawn}
				class="shrink-0 text-txt3 transition-colors hover:text-txt disabled:cursor-not-allowed disabled:hover:text-txt3"
				aria-label="Make another one"
			>
				<Icon name="refresh" class="h-4 w-4" />
			</button>
		</div>

		<div class="mt-6">
			<div class="flex items-center justify-between">
				<label for="{id}-length" class="font-mono text-label tracking-label text-txt3 uppercase"
					>{drawn?.pin ? 'PIN length' : 'Length'}</label
				>
				<span class="font-mono text-fine text-txt">{length}</span>
			</div>
			<input
				id="{id}-length"
				type="range"
				min={drawn?.shortest}
				max={drawn?.longest}
				disabled={!drawn}
				bind:value={length}
				onchange={draw}
				class="mt-3 w-full"
			/>
			<div class="mt-2 flex justify-between font-mono text-label text-txt4">
				<span>{drawn?.shortest}</span><span>{drawn?.longest}</span>
			</div>
		</div>

		<div class="mt-6 flex flex-wrap gap-2">
			{#each SETS as set (set.key)}
				{@const on = chosen.includes(set.key)}
				<button
					type="button"
					onclick={() => toggle(set.key)}
					aria-pressed={on}
					disabled={!drawn}
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
			<!-- On when look-alikes stay out, which is how the generator starts:
			     the switch reads as what it does, and pressed means it is doing
			     it. It used to read "Look-alike characters" and let them in when
			     it was on. Not drawn for a PIN, which the rule leaves alone. -->
			{#if drawn?.lookAlikes}
				<button
					type="button"
					onclick={() => {
						similar = !similar;
						void draw();
					}}
					aria-pressed={!similar}
					class="flex items-center gap-1.5 rounded-full border border-hairline px-3 py-1.5 font-mono text-meta transition-colors {similar
						? 'text-txt4 hover:text-txt3'
						: 'bg-surface2 text-txt'}"
				>
					{#if !similar}
						<Icon name="check" class="h-3.5 w-3.5 text-txt2" />
					{/if}
					Avoid look-alikes ({[...(drawn?.lookAlikes ?? '')].join(' ')})
				</button>
			{/if}
		</div>
		<!-- What "Symbols" draws from, character for character, as the engine
		     writes it. "!@#$%" on the switch promised five and the engine drew
		     thirty-two, quotes and the backslash among them. -->
		<p class="mt-3 font-mono text-label leading-relaxed break-all text-txt4">
			Symbols: {drawn?.symbols}
		</p>

		<div class="mt-6">
			<label for="{id}-avoid" class="font-mono text-label tracking-label text-txt3 uppercase">
				Avoid these characters
			</label>
			<div class="mt-2 flex">
				<Field>
					<input
						id="{id}-avoid"
						type="text"
						value={avoid}
						placeholder="None"
						autocomplete="off"
						spellcheck="false"
						disabled={!drawn}
						oninput={(event) => {
							avoid = event.currentTarget.value;
							void draw();
						}}
						class="min-w-0 flex-1 bg-transparent font-mono text-small text-txt outline-none placeholder:text-txt4"
					/>
				</Field>
			</div>
		</div>

		<div class="mt-8 flex items-center justify-between gap-4">
			<!-- Where the reader decides, so it is where a password that lacks a
			     kind they asked for says so. The generator draws evenly and does
			     not draw again on its own; the reader knows what the site wants. -->
			<span
				class="font-mono text-label leading-relaxed {missing.length > 0
					? 'text-warn'
					: 'text-txt4'}"
			>
				<!-- Only the warning is said aloud, and it is said for every
				     password that has it: the footnote is not news, and a second
				     password that lacks a digit too is. -->
				<span role="status">
					{#key told}
						{#if missing.length > 0}
							No {listed(missing)} in this one.<br />Make another if the site asks for {missing.length >
							1
								? 'them'
								: 'one'}.
						{/if}
					{/key}
				</span>
				{#if missing.length === 0}
					Randomness comes from OsRng<br />There are no “memorable” passwords
				{/if}
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
