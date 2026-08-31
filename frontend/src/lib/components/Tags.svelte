<script lang="ts">
	import Icon from './Icon.svelte';

	/**
	 * The tags on an entry, and the chip that adds one.
	 *
	 * A tag is refused by the engine when it holds a semicolon, a comma or a
	 * tab, because the format keeps them all in one string and reads any of the
	 * three as the separator; the message that comes back says so.
	 */
	let {
		tags,
		readOnly,
		onSet
	}: { tags: string[]; readOnly: boolean; onSet: (tags: string[]) => void } = $props();

	let adding = $state(false);
	let field = $state<HTMLInputElement>();

	function add() {
		if (!field) return;
		const named = field.value.trim();
		field.value = '';
		adding = false;
		if (named === '') return;
		if (tags.includes(named)) return;
		onSet([...tags, named]);
	}

	function keys(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			adding = false;
		}
		if (event.key === 'Enter') {
			event.preventDefault();
			add();
		}
	}

	function remove(at: number) {
		onSet(tags.filter((_, index) => index !== at));
	}

	function open() {
		adding = true;
		// The field is drawn by the same change that sets this, so it is focused
		// once it exists rather than now.
		queueMicrotask(() => field?.focus());
	}
</script>

<div class="mt-2 flex flex-wrap items-center gap-1.5">
	<!-- Keyed by position: a file may hold the same tag twice. -->
	{#each tags as tag, at (at)}
		<span
			class="flex items-center gap-1.5 rounded-full bg-surface2 px-2.5 py-1 font-mono text-label tracking-label text-txt3 uppercase"
		>
			{tag}
			{#if !readOnly}
				<button
					type="button"
					onclick={() => remove(at)}
					class="text-txt4 transition-colors hover:text-danger"
					aria-label="Remove the tag {tag}"
				>
					<Icon name="x" class="h-3.5 w-3.5" />
				</button>
			{/if}
		</span>
	{/each}

	{#if adding}
		<input
			bind:this={field}
			type="text"
			autocomplete="off"
			spellcheck="false"
			aria-label="A new tag"
			onblur={add}
			onkeydown={keys}
			class="w-24 animate-rise rounded-full border border-accent bg-surface2 px-2.5 py-1 font-mono text-label tracking-label text-txt uppercase ring-4 ring-accent/15 outline-none"
		/>
	{:else if !readOnly}
		<button
			type="button"
			onclick={open}
			class="rounded-full border border-dashed border-hairline px-2.5 py-1 font-mono text-label tracking-label text-txt4 uppercase transition-colors hover:border-txt4 hover:text-txt3"
		>
			+ tag
		</button>
	{/if}
</div>
