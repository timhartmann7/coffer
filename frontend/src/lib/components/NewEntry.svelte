<script lang="ts">
	import { tick, type Snippet } from 'svelte';
	import { UNTITLED } from '$lib/format';
	import { cancels } from '$lib/lines';
	import type { EntryRow, Offer } from '$lib/model';
	import { leaves, step } from '$lib/popup';
	import Icon from './Icon.svelte';
	import Mask from './Mask.svelte';

	/**
	 * "+ Entry", split in two: the mockup's pill makes a login, and the chevron
	 * beside it lists every kind Coffer makes and then the templates the vault
	 * keeps.
	 *
	 * The list is the settings chip's list, and its keyboard is this
	 * component's: opening it puts the focus on its first line - which is also
	 * what lets a press outside close it, since a WebKit button that was
	 * pressed takes no focus and so could never see it leave - the arrows step
	 * through the lines and round, Escape closes it with the focus back on the
	 * chevron and goes no further, from a line or from the chevron, and the
	 * focus leaving the control closes it. A second press on the chevron closes
	 * it too, with the focus on the chevron. A line chosen closes it without
	 * taking the focus back, so that the entry it makes can take it.
	 */
	let {
		offered,
		templates,
		asking,
		choosing,
		main = $bindable(),
		question,
		onMake,
		onTemplate,
		onDismiss
	}: {
		/** The kinds, in the order to list them. */
		offered: Offer[];
		/** The entries of the vault's templates group, in the order to list. */
		templates: EntryRow[];
		/** Whether making one asks where first: the pill then opens a list of
		 * folders rather than making a login there and then. */
		asking: boolean;
		/** Whether that list of folders is open. */
		choosing: boolean;
		/** The pill, for the list of folders to give the focus back to. */
		main?: HTMLButtonElement;
		/** The list of folders, drawn in this control's box so that the focus
		 * moving between it and the pill is not the focus leaving. */
		question?: Snippet;
		/** A kind was chosen from the list - or, with none, the pill was
		 * pressed, which makes what the screen makes when nobody chose. */
		onMake: (offer?: Offer) => void;
		onTemplate: (row: EntryRow) => void;
		/** The pill was pressed while the list of folders was open. */
		onDismiss: () => void;
	} = $props();

	const id = $props.id();
	let wrapper = $state<HTMLElement>();
	let chevron = $state<HTMLButtonElement>();
	let list = $state<HTMLElement>();
	let open = $state(false);
	const listed = `${id}-kinds`;
	const heading = `${id}-templates`;

	/** Opens the list with the focus on its first line, or its last. */
	async function show(from: 'first' | 'last') {
		open = true;
		await tick();
		step(list, 'menuitem', from === 'first' ? 'ArrowDown' : 'ArrowUp');
	}

	function toggle() {
		if (open) shut(true);
		else void show('first');
	}

	function shut(back: boolean) {
		open = false;
		if (back) chevron?.focus();
	}

	/** The chevron's keys: an arrow opens the list, and Escape closes it the
	 * way it does from a line - Shift+Tab from the first line lands here with
	 * the list still open. Return and Space are the button's own press. */
	function opening(event: KeyboardEvent) {
		if (open && cancels(event)) {
			event.preventDefault();
			event.stopPropagation();
			shut(true);
			return;
		}
		if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
		event.preventDefault();
		void show(event.key === 'ArrowDown' ? 'first' : 'last');
	}

	/** The keys of the list. Escape is the list's and goes no further: the
	 * window's own would clear the search or put the pane away behind a list
	 * the reader only meant to close. */
	function keys(event: KeyboardEvent) {
		if (cancels(event)) {
			event.preventDefault();
			event.stopPropagation();
			shut(true);
			return;
		}
		if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
		event.preventDefault();
		step(list, 'menuitem', event.key);
	}

	function make(offer: Offer) {
		shut(false);
		onMake(offer);
	}

	function fromTemplate(row: EntryRow) {
		shut(false);
		onTemplate(row);
	}

	/** The focus left the chevron or a line. Whether it left the control is
	 * the question. */
	function left(event: FocusEvent) {
		if (open && leaves(event, wrapper)) open = false;
	}
</script>

<!-- One box, so the list hangs from the pill's edge and the list of folders
     the pill asks with sits in it too. -->
<span bind:this={wrapper} class="relative flex shrink-0" onfocusout={left}>
	<span
		class="flex h-9 items-stretch rounded-full border border-hairline transition-colors hover:border-txt4"
	>
		<!-- The press that opens the list of folders is an ordinary one, so a
		     value being typed into the pane is left, and written, before the
		     new entry takes it. With that list open the press keeps the focus in
		     its filter, as the line above an entry's title does: WebKit gives a
		     pressed button no focus, so the filter would lose it to nothing, the
		     list would close on the press, and the click would open it again. -->
		<button
			bind:this={main}
			type="button"
			onmousedown={(event) => {
				if (choosing) event.preventDefault();
			}}
			onclick={() => (choosing ? onDismiss() : onMake())}
			aria-haspopup={asking ? 'listbox' : undefined}
			aria-expanded={asking ? choosing : undefined}
			class="flex items-center gap-2 rounded-l-full pr-3 pl-4 text-small text-txt2 transition-colors hover:text-txt active:bg-raised"
		>
			<Icon name="plus" class="h-4 w-4" /> Entry
		</button>
		<!-- A press with the list open keeps the focus where it is, for the
		     reason the pill keeps it in the list of folders: the line holding it
		     would lose it to nothing, the list would close on the press, and the
		     click would open it again. -->
		<button
			bind:this={chevron}
			type="button"
			onmousedown={(event) => {
				if (open) event.preventDefault();
			}}
			onclick={toggle}
			onkeydown={opening}
			aria-label="Other kinds of entry"
			aria-haspopup="menu"
			aria-expanded={open}
			aria-controls={open ? listed : undefined}
			class="flex items-center rounded-r-full border-l border-hairline pr-3 pl-2 text-txt4 transition-colors hover:text-txt2 active:bg-raised"
		>
			<Icon name="chev-d" class="h-3.5 w-3.5" />
		</button>
	</span>

	{#if open}
		<div
			bind:this={list}
			id={listed}
			role="menu"
			aria-label="New entry"
			tabindex="-1"
			onkeydown={keys}
			class="absolute top-full right-0 z-10 mt-1 max-h-[420px] max-w-[34ch] min-w-full origin-top animate-pop overflow-y-auto rounded-sm border border-hairline bg-raised"
		>
			{#each offered as offer (offer.kind)}
				<button
					type="button"
					role="menuitem"
					onmousedown={(event) => event.preventDefault()}
					onclick={() => make(offer)}
					class="block w-full px-3 py-2 text-left text-fine whitespace-nowrap text-txt2 transition-colors outline-none hover:bg-surface2 hover:text-txt focus:bg-surface2 focus:text-txt"
				>
					{offer.name}
				</button>
			{/each}
			{#if templates.length > 0}
				<div role="group" aria-labelledby={heading} class="border-t border-hairline">
					<p
						id={heading}
						class="px-3 pt-2 pb-1 font-mono text-label tracking-label text-txt4 uppercase"
					>
						Templates in this vault
					</p>
					{#each templates as template (template.id)}
						<button
							type="button"
							role="menuitem"
							aria-label={template.title === null ? 'A template whose name is hidden' : undefined}
							onmousedown={(event) => event.preventDefault()}
							onclick={() => fromTemplate(template)}
							class="block w-full truncate px-3 py-2 text-left text-fine whitespace-nowrap text-txt2 transition-colors outline-none hover:bg-surface2 hover:text-txt focus:bg-surface2 focus:text-txt"
						>
							{#if template.title === null}
								<Mask />
							{:else if template.title === ''}
								<span class="text-txt4">{UNTITLED}</span>
							{:else}
								<bdi>{template.title}</bdi>
							{/if}
						</button>
					{/each}
				</div>
			{/if}
		</div>
	{/if}

	{@render question?.()}
</span>
