<script lang="ts">
	import { untrack } from 'svelte';
	import { drop, typed as said, unfinished, type Place } from '$lib/drafts';
	import { finishes, lines } from '$lib/lines';
	import Confirm from './Confirm.svelte';
	import Field from './Field.svelte';
	import Unsaved from './Unsaved.svelte';

	/**
	 * A new value for something the database protects, written on purpose.
	 *
	 * Showing a value is not changing it. The field a password used to be shown
	 * in was the field it was edited in, with the focus already there, so a
	 * stray key while it was on the screen for somebody to read out changed the
	 * stored password and the next click anywhere saved it. A change is now a
	 * field of its own, opened by a press that says so, empty, and written only
	 * by Save or Return. Cancel and Escape write nothing. The value it replaces
	 * stays where it was, shown if it was shown, until the new one is saved.
	 *
	 * Leaving it with something typed asks rather than deciding. The click that
	 * took the focus away could have been meant for anything, and neither
	 * writing a half-typed password over the real one nor throwing away one the
	 * reader has just set on a website is a guess to make for them. The question
	 * leaves the focus wherever the reader put it, and going back into the field
	 * takes the question away again.
	 *
	 * What is typed here is the reader's own. It is in this field, and - so that
	 * a lock that comes before Save or Discard, a lid closed or the reader
	 * walking away, writes it the way Save would have rather than wiping it with
	 * the window - in Rust as a draft of the field (`drafts.ts`), told as it is
	 * typed. Both go when the field closes, whichever way it closes. A field that
	 * goes while holding something nobody answered for - its entry closed, or
	 * another one opened - says so rather than going quietly.
	 */
	let {
		label,
		placeholder,
		what,
		class: classes = '',
		draft,
		onSave,
		onClose,
		onFailure
	}: {
		/** What the field is called to anything that reads the screen aloud. */
		label: string;
		placeholder: string;
		/** What is being changed, after "the new": "password", or "value of
		 * “API token”". The question and the notice both say it. */
		what: string;
		class?: string;
		/** The entry and field the new value is for, and whether the database
		 * protects it: what is typed is told to Rust as a draft of that field. */
		draft: Place;
		/** Writes the value, and answers whether the vault took it. A value that
		 * was refused stays in the field to be put right. */
		onSave: (value: string) => Promise<boolean>;
		/** The field is done with, saved or not. */
		onClose: () => void;
		onFailure: (thrown: unknown) => void;
	} = $props();

	let node = $state<HTMLTextAreaElement>();
	let root = $state<HTMLElement>();
	/** Whether the reader has typed anything since the field opened. A field
	 * opened and left as it was is not a new value. Nothing is drawn from it. */
	let edited = false;
	/** Whether the field was left holding something, and the row is asking what
	 * to do with it. */
	let asking = $state(false);
	/** How many lines are in the field, which is how tall it is. */
	let written = $state(1);
	/** Whether a save is on its way, so that Return and Save pressed together
	 * write once. */
	let saving = false;

	/**
	 * Focuses the field when it opens, and wipes it when it closes.
	 *
	 * Tied to the element rather than to the component, so that the wipe is of
	 * the element the reader typed into, whatever the component still knows
	 * about it by then.
	 */
	function held(element: HTMLTextAreaElement) {
		element.focus();
		// Where the typing goes is settled when the field opens, and a field
		// going is the one moment its owner may already be gone. Read without
		// being followed: an attachment runs again for whatever it reads, and
		// running again is wiping the field.
		const place = untrack(() => draft);
		return () => {
			if (edited && element.value !== '') {
				onFailure({ code: 'refused', message: `The new ${what} was not saved.` });
			}
			drop(place);
			element.value = '';
		};
	}

	function typed() {
		if (!node) return;
		edited = true;
		written = lines(node.value);
		const element = node;
		said(draft, () => element.value);
	}

	/**
	 * The focus left the field and its answers.
	 *
	 * Moving between the field and its own Cancel and Save is not leaving, and
	 * neither is the window going behind another one: the focus comes back to
	 * the field with the window, and a question waiting there would be about
	 * nothing the reader did. A field left with nothing in it closes, because
	 * there is nothing in it to ask about.
	 */
	function left(event: FocusEvent) {
		if (event.relatedTarget instanceof Node && root?.contains(event.relatedTarget)) return;
		if (!document.hasFocus()) return;
		if (!edited || !node || node.value === '') {
			close();
			return;
		}
		asking = true;
	}

	function keys(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			// The window reads Escape as "close the entry", and a change put
			// away is not an entry put away.
			event.stopPropagation();
			close();
			return;
		}
		if (finishes(event, written > 1)) {
			event.preventDefault();
			void save();
		}
	}

	async function save() {
		if (!node || saving) return;
		if (!edited) {
			close();
			return;
		}
		saving = true;
		try {
			if (await onSave(node.value)) close();
		} finally {
			saving = false;
		}
	}

	/** Puts the field away and wipes what was in it, writing nothing. */
	export function close() {
		edited = false;
		asking = false;
		drop(draft);
		if (node) node.value = '';
		onClose();
	}
</script>

<div bind:this={root} class={classes} onfocusout={left}>
	<Field>
		<textarea
			bind:this={node}
			{@attach held}
			rows={written}
			wrap={written > 1 ? 'soft' : 'off'}
			aria-label={label}
			{placeholder}
			autocomplete="off"
			spellcheck="false"
			oninput={typed}
			onfocus={() => (asking = false)}
			onkeydown={keys}
			class="min-w-0 flex-1 resize-none bg-transparent font-mono text-small text-txt outline-none placeholder:text-txt4 {written >
			1
				? 'overflow-x-hidden overflow-y-auto'
				: 'overflow-hidden'}"></textarea>
		{#if unfinished(draft)}
			<Unsaved class="mt-2 self-start" />
		{/if}
	</Field>

	{#if asking}
		<Confirm
			class="mt-2 bg-surface2"
			question="Save the new {what}?"
			keep="Discard"
			neutral={{ label: 'Save', run: () => void save() }}
			focus="none"
			onKeep={close}
		/>
	{:else}
		<!-- Neither press moves the focus, so the field is not left - and asked
		     about - by the press that answers it. -->
		<div class="mt-2 flex flex-wrap items-center gap-2">
			<button
				type="button"
				onmousedown={(event) => event.preventDefault()}
				onclick={close}
				class="flex h-7 shrink-0 items-center rounded-full px-3 text-fine text-txt3 transition-colors hover:text-txt2"
			>
				Cancel
			</button>
			<button
				type="button"
				onmousedown={(event) => event.preventDefault()}
				onclick={() => void save()}
				class="flex h-7 shrink-0 items-center rounded-full border border-hairline px-3 text-fine text-txt2 transition hover:border-txt3 hover:text-txt active:bg-raised"
			>
				Save
			</button>
		</div>
	{/if}
</div>
