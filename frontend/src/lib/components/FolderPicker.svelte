<script lang="ts">
	import { quoted } from '$lib/format';
	import { cancels, composing } from '$lib/lines';
	import type { Group } from '$lib/model';
	import { destinations } from '$lib/places';
	import { leaves } from '$lib/popup';
	import { fold } from '$lib/search';
	import Icon from './Icon.svelte';
	import Path from './Path.svelte';

	/**
	 * The folders a thing can go to, with a filter on top: the settings chip's
	 * list, for a vault that may hold a thousand folders.
	 *
	 * The focus stays in the filter and the keys move a line through the list,
	 * the way a combo box on a Mac does, so Return takes the line the list opened
	 * on without a single key before it. Nothing in the recycle bin is listed:
	 * deleting is the way in.
	 */
	let {
		root,
		heading,
		label,
		current,
		chosen,
		class: classes,
		onPick,
		onClose
	}: {
		root: Group;
		/** The heading over the list: "Move to" or "Put it in". */
		heading: string;
		/** What the filter is for, read out by a screen reader. */
		label: string;
		/** Where the thing is now, marked in the list; `null` for an entry not
		 * made yet. */
		current: string | null;
		/** The line the keys start on, which Return gives straight away. */
		chosen: string;
		/** Where it sits: placement and width only. */
		class: string;
		onPick: (into: string) => void;
		/**
		 * Nothing was chosen. `escaped` for Escape and for the current place
		 * chosen again, after which the opener takes the focus back; not for the
		 * focus leaving the list, which stays where the reader put it.
		 */
		onClose: (escaped: boolean) => void;
	} = $props();

	const id = $props.id();
	let wrapper = $state<HTMLElement>();
	let field = $state<HTMLInputElement>();
	let query = $state('');
	/** The line the keys are on, until the filter leaves it out. */
	let active = $state<string | null>(null);

	/** Every place, with the text the filter runs against folded once per
	 * drawing rather than on every key: a folder's name can be a megabyte. */
	const lines = $derived(
		destinations(root).map((place) => ({
			...place,
			haystack: fold([place.name, ...place.parents].join('\n'))
		}))
	);
	const shown = $derived.by(() => {
		if (query === '') return lines;
		const needle = fold(query);
		return lines.filter((line) => line.haystack.includes(needle));
	});
	/** The line Return gives: the one the keys are on while it still matches,
	 * else the first that does. */
	const at = $derived(shown.find((line) => line.id === (active ?? chosen)) ?? shown.at(0) ?? null);
	const list = `${id}-list`;
	const option = (place: string) => `${id}-${place}`;

	$effect(() => {
		field?.focus();
	});

	/**
	 * Keeps the line the keys are on in sight: the one the list opens on, which
	 * Return takes without a key before it, and every one the arrows or the
	 * filter move to. The list shows five lines, the line it opens on can be the
	 * fortieth, and WebKit does not scroll to an active descendant by itself - a
	 * Return would take a line the reader was never shown.
	 */
	$effect(() => {
		if (at !== null) document.getElementById(option(at.id))?.scrollIntoView({ block: 'nearest' });
	});

	function pick(into: string) {
		if (into === current) onClose(true);
		else onPick(into);
	}

	/**
	 * The keys of the filter. Escape is the list's and goes no further: the
	 * window's own Escape would clear the search or put the pane away behind a
	 * list the reader only meant to close.
	 */
	function key(event: KeyboardEvent) {
		if (cancels(event)) {
			event.preventDefault();
			event.stopPropagation();
			onClose(true);
			return;
		}
		if (event.key === 'Enter') {
			event.preventDefault();
			if (!composing(event) && at !== null) pick(at.id);
			return;
		}
		if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
		event.preventDefault();
		if (shown.length === 0) return;
		const here = at === null ? -1 : shown.indexOf(at);
		const step = event.key === 'ArrowDown' ? 1 : -1;
		const next = shown[(here + step + shown.length) % shown.length];
		active = next.id;
	}

	/**
	 * The focus left. The list hangs from the control that opened it, in one
	 * box with it, so the focus going back to that control is not the focus
	 * leaving: that control's own press is what closes the list then.
	 */
	function left(event: FocusEvent) {
		if (leaves(event, wrapper?.parentElement ?? wrapper)) onClose(false);
	}
</script>

<div
	bind:this={wrapper}
	onfocusout={left}
	class="absolute z-10 mt-1 origin-top animate-pop overflow-hidden rounded-sm border border-hairline bg-raised {classes}"
>
	<p class="px-3 pt-3 font-mono text-label tracking-label text-txt3 uppercase">{heading}</p>
	<div class="px-2 pt-2">
		<input
			bind:this={field}
			bind:value={query}
			type="text"
			role="combobox"
			aria-expanded="true"
			aria-controls={shown.length > 0 ? list : undefined}
			aria-activedescendant={at === null ? undefined : option(at.id)}
			aria-label={label}
			autocomplete="off"
			spellcheck="false"
			placeholder="Find a folder"
			onkeydown={key}
			class="block w-full rounded-sm border border-accent bg-surface px-2 py-1.5 text-body text-txt ring-4 ring-accent/15 outline-none placeholder:text-txt4"
		/>
	</div>

	{#if shown.length > 0}
		<div id={list} role="listbox" aria-label={heading} class="max-h-40 overflow-y-auto p-1">
			{#each shown as line (line.id)}
				{@const here = line.id === at?.id}
				<button
					id={option(line.id)}
					type="button"
					role="option"
					tabindex="-1"
					aria-selected={here}
					aria-current={line.id === current ? 'location' : undefined}
					onmousedown={(event) => event.preventDefault()}
					onclick={() => pick(line.id)}
					class="flex w-full items-center gap-2 rounded-sm px-2 py-[7px] text-left text-small transition-colors {here
						? 'bg-surface2 text-txt'
						: 'text-txt2 hover:bg-surface2 hover:text-txt'}"
				>
					<Icon
						name={line.id === root.id ? 'disk' : 'folder'}
						class="h-4 w-4 shrink-0 {line.id === current ? 'text-accent' : 'text-txt4'}"
					/>
					<span class="min-w-0 truncate"><bdi>{line.name}</bdi></span>
					<span class="min-w-0 flex-1 truncate font-mono text-meta text-txt4">
						<Path names={line.parents} />
					</span>
				</button>
			{/each}
		</div>
	{:else}
		<!-- What was typed can be as long as anything pasted, so the line is
		     bounded like every other that repeats a value. -->
		<p class="line-clamp-2 px-3 py-2 text-fine break-words text-txt4">
			No folder matches {quoted(query)}
		</p>
	{/if}
</div>
