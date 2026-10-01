<script lang="ts">
	/**
	 * A question asked where the destructive press landed, before anything goes.
	 *
	 * Asked in place rather than in a dialog: the question stands beside the
	 * thing it is about, where a sheet over the window would cover it. Every
	 * confirmation in the window is this one, so they all read the same way:
	 *
	 * - the question in words, naming what goes and whether it comes back;
	 * - the way out first, named by what it leaves rather than by "Cancel";
	 * - a neutral answer in the middle when there is one: a step worth taking
	 *   first, which leaves the question where it is, or a way on that loses
	 *   nothing;
	 * - the destructive answer last, and the only one in the danger colour.
	 *
	 * A question whose destructive answer was given and refused can be asked
	 * again without it, when something that loses nothing is still open to the
	 * reader: the same box and the same keys, one button fewer.
	 *
	 * The focus lands on an answer that loses nothing - the way out, or the
	 * neutral answer when that is the one a reader means most often - and Escape
	 * is the way out, so that a second press of Return or Space, or a key meant
	 * for the pane, is never the one that destroys. The Escape stops here: the
	 * window reads the same key as "close the entry", and a question answered by
	 * taking the pane away is a question nobody saw answered.
	 *
	 * A question asked because the reader moved the focus somewhere else leaves
	 * it there. Taking it back would put their next key - a letter meant for the
	 * field they just clicked into, a Space - on one of the answers.
	 */
	let {
		question,
		keep = 'Keep it',
		act,
		neutral,
		focus = 'keep',
		bare = false,
		class: classes = '',
		onKeep,
		onAct
	}: {
		question: string;
		/** The way out, named by its result. */
		keep?: string;
		/** The destructive answer, named by what it does. Left out when it has
		 * been refused and the question is asked again without it, and when no
		 * answer to the question takes anything out of the vault. */
		act?: string;
		neutral?: { label: string; run: () => void };
		/** Which answer the focus lands on, and so which one Return gives. Never
		 * the destructive one, and none for a question that the focus leaving
		 * something raised. */
		focus?: 'keep' | 'neutral' | 'none';
		/** Without a box of its own, for a question asked inside a card that is
		 * one already: a file's row, a version's. */
		bare?: boolean;
		/** Where it sits, and for a boxed one the plane it is drawn on, which is
		 * the other one from the plane around it. */
		class?: string;
		onKeep: () => void;
		/** Given with `act`, and only then. */
		onAct?: () => void;
	} = $props();

	const id = $props.id();
	let safe = $state<HTMLButtonElement>();
	let middle = $state<HTMLButtonElement>();

	$effect(() => {
		if (focus !== 'none') (focus === 'neutral' && middle ? middle : safe)?.focus();
	});

	function keys(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		event.stopPropagation();
		onKeep();
	}
</script>

<div
	role="group"
	aria-labelledby="{id}-question"
	data-confirm
	class="animate-rise {bare ? '' : 'rounded-sm border border-hairline p-3'} {classes}"
>
	<p id="{id}-question" class="text-fine leading-relaxed break-words text-txt2">{question}</p>
	<div class="mt-3 flex flex-wrap gap-2">
		<button
			bind:this={safe}
			type="button"
			onclick={onKeep}
			onkeydown={keys}
			class="h-9 rounded-full px-3 text-small text-txt3 transition-colors hover:text-txt2"
		>
			{keep}
		</button>
		{#if neutral}
			<button
				bind:this={middle}
				type="button"
				onclick={neutral.run}
				onkeydown={keys}
				class="h-9 rounded-full border border-hairline px-3 text-small text-txt2 transition-colors hover:border-txt4 hover:text-txt"
			>
				{neutral.label}
			</button>
		{/if}
		{#if act !== undefined}
			<button
				type="button"
				onclick={onAct}
				onkeydown={keys}
				class="h-9 rounded-full px-3 text-small text-danger transition-colors hover:bg-dangerwash"
			>
				{act}
			</button>
		{/if}
	</div>
</div>
