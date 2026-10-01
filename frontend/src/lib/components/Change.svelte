<script lang="ts">
	import { untrack } from 'svelte';
	import { drop, replacing, unfinished, type Place } from '$lib/drafts';
	import { hold } from '$lib/holding';
	import { asFailure } from '$lib/ipc';
	import { cancels, finishes, lines } from '$lib/lines';
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
	 * by Save or Return. Cancel and Escape write nothing, and neither does a
	 * field emptied again: nothing typed is no new value, so Save is not offered
	 * for it and Return passes it by. The value it replaces stays where it was
	 * until the new one is saved.
	 *
	 * Leaving it with something typed asks rather than deciding. The click that
	 * took the focus away could have been meant for anything, and neither
	 * writing a half-typed password over the real one nor throwing away one the
	 * reader has just set on a website is a guess to make for them. The question
	 * leaves the focus wherever the reader put it, and going back into the field
	 * takes the question away again. The press that opened the field, pressed
	 * again, asks the same question, because it is as likely to mean "done" as
	 * "never mind".
	 *
	 * Its way out, and so its Escape, is back into the field, which loses
	 * nothing. Escape is the key a reader presses again when the pane did not
	 * close the first time, and the window's Escape is what raised the question
	 * with the focus on it: a way out that discarded was the only copy of a new
	 * password gone on the second press, with nothing said. Discard is an
	 * answer of its own, last and in the danger colour, because it is the one
	 * that throws away what nothing else holds.
	 *
	 * The pane does not go while this holds something or is saving it
	 * (`holding.ts`). A row pressed, Escape, Close, a folder chosen: each leaves
	 * the pane where it is, and this puts its question with the focus on it, so
	 * nothing typed is lost until the reader answers.
	 *
	 * What is typed here is the reader's own. It is in this field, and - so that
	 * a lock that comes before Save or Discard, a lid closed or the reader
	 * walking away, keeps it rather than wiping it with the window - in Rust as
	 * a new value for the field (`drafts.ts`), told as it is typed. The reader
	 * never saved it, so a lock keeps it beside the value it was for and never
	 * writes it over one. Both go when the field closes, whichever way it
	 * closes. A field that goes all the same while holding something nobody
	 * answered for - its entry gone from the vault, the file read again - says
	 * so rather than going quietly. One that goes while its save is on its way
	 * has handed the value over, and says only if the save then fails.
	 */
	let {
		label,
		placeholder,
		what,
		multiline,
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
		 * “API token”". The question and the notices all say it. */
		what: string;
		/** Written in lines from the start, four lines tall, where Return starts
		 * a line and Cmd+Return saves: the value it replaces is in lines, or the
		 * reader asked for lines when they named the field. Ten recovery codes
		 * changed one line at a time were saved over all ten at the first
		 * Return. */
		multiline: boolean;
		class?: string;
		/** The entry and field the new value is for, and whether the database
		 * protects it: what is typed is told to Rust as a new value for that
		 * field. */
		draft: Place;
		/** Writes the value. Rejects with what the vault said when it would not
		 * take it, and a value that was refused stays in the field to be put
		 * right. */
		onSave: (value: string) => Promise<void>;
		/** The field is done with, saved or not. `back` is whether the focus was
		 * in it, which is when it goes back to the press that opened it rather
		 * than falling to nowhere. */
		onClose: (back: boolean) => void;
		onFailure: (thrown: unknown) => void;
	} = $props();

	let node = $state<HTMLTextAreaElement>();
	let root = $state<HTMLElement>();
	/** Whether the field has nothing in it, and so no new value to save. */
	let blank = $state(true);
	/** Whether the row is asking what to do with what is in the field. */
	let asking = $state(false);
	/**
	 * How many times the window has put the question since the reader was last
	 * in the field, because the pane was asked to go.
	 *
	 * A question the window put takes the focus, so that the answer is one key
	 * away; one the focus leaving put does not. Counted rather than a flag,
	 * because a question already standing that is put again - a second row
	 * pressed - is drawn again, and takes the focus again.
	 */
	let summoned = $state(0);
	/** How many lines are in the field, which is how tall it is. */
	let written = $state(1);
	/** Whether Return starts a line rather than saving. */
	const lined = $derived(multiline || written > 1);
	/** Whether a save is on its way, so that Return and Save pressed together
	 * write once, and so that the pane stays until it is back. */
	let saving = false;
	/** Whether the field has gone from the screen. A save that comes back after
	 * that has no field to close or to leave a refused value in. */
	let gone = false;

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
		const letGo = hold({ holds: () => saving || element.value !== '', ask: summon });
		return () => {
			letGo();
			gone = true;
			// A save on its way has the value already, and says how it went.
			if (!saving && element.value !== '') {
				onFailure({ code: 'refused', message: `The new ${what} was not saved.` });
			}
			drop(place);
			element.value = '';
		};
	}

	function typed() {
		if (!node) return;
		const element = node;
		written = lines(element.value);
		blank = element.value === '';
		// Emptied is taken back. A lock that kept an empty new value would have
		// nothing to keep, and one that wrote it would empty the value it was
		// typed for.
		if (blank) drop(draft);
		else replacing(draft, () => element.value);
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
		if (!node || node.value === '') {
			close(false);
			return;
		}
		asking = true;
	}

	/**
	 * Puts the question because the window was asked to take the pane away,
	 * with the focus on Save when `focus` says so: an answer that loses nothing,
	 * since a saved value leaves the old one in Versions. Answers whether there
	 * was a question to put. A save on its way has none, and the pane only
	 * waits for it.
	 */
	function summon(focus: boolean): boolean {
		if (saving) return false;
		asking = true;
		if (focus) summoned += 1;
		return true;
	}

	function keys(event: KeyboardEvent) {
		if (cancels(event)) {
			// The window reads Escape as "close the entry", and a change put
			// away is not an entry put away.
			event.stopPropagation();
			close();
			return;
		}
		if (finishes(event, lined)) {
			event.preventDefault();
			void save();
		}
	}

	async function save() {
		if (!node || saving || node.value === '') return;
		saving = true;
		try {
			await onSave(node.value);
			if (!gone) close();
		} catch (thrown) {
			// A field still on the screen keeps the value to be put right, and
			// the reason is enough. One that has gone keeps nothing, and every
			// reason Rust refuses a value - the entry gone from the vault, text
			// the file format cannot hold - would refuse it again: the reader is
			// told plainly that it did not go in, rather than left to find out.
			onFailure(
				gone
					? {
							code: 'refused',
							message: `The new ${what} was not saved: ${asFailure(thrown).message}.`
						}
					: thrown
			);
		} finally {
			saving = false;
		}
	}

	/**
	 * The press that opened the field, pressed again. With nothing typed the
	 * field is put away; with something typed the question is put, the way
	 * leaving asks it, and nothing is thrown away until it is answered. A save
	 * on its way is let finish.
	 */
	export function leave() {
		if (saving) return;
		if (!node || node.value === '') close();
		else summon(true);
	}

	/**
	 * Puts the field away and wipes what was in it, writing nothing. The focus
	 * goes back to whatever opened the field when it was in the field or its
	 * answers, and stays wherever the reader put it otherwise.
	 */
	export function close(back = root?.contains(document.activeElement) ?? false) {
		asking = false;
		summoned = 0;
		blank = true;
		drop(draft);
		if (node) node.value = '';
		onClose(back);
	}
</script>

<div bind:this={root} class={classes} onfocusout={left}>
	<Field>
		<textarea
			bind:this={node}
			{@attach held}
			rows={multiline ? Math.max(4, written) : written}
			wrap={lined ? 'soft' : 'off'}
			aria-label={label}
			{placeholder}
			autocomplete="off"
			spellcheck="false"
			oninput={typed}
			onfocus={() => {
				asking = false;
				summoned = 0;
			}}
			onkeydown={keys}
			class="min-w-0 flex-1 resize-none bg-transparent font-mono text-small text-txt outline-none placeholder:text-txt4 {lined
				? 'overflow-x-hidden overflow-y-auto'
				: 'overflow-hidden'}"></textarea>
		{#if unfinished(draft)}
			<Unsaved class="mt-2 self-start" />
		{/if}
	</Field>

	{#if asking}
		<!-- Drawn again each time the window puts it, so that the question rises
		     where the reader is looking and takes the focus again. -->
		{#key summoned}
			<Confirm
				class="mt-2 bg-surface2"
				question="Save the new {what}?"
				keep="Keep typing"
				neutral={{ label: 'Save', run: () => void save() }}
				act="Discard"
				focus={summoned > 0 ? 'neutral' : 'none'}
				onKeep={() => node?.focus()}
				onAct={() => close()}
			/>
		{/key}
	{:else}
		<!-- Neither press moves the focus, so the field is not left - and asked
		     about - by the press that answers it. -->
		<div class="mt-2 flex flex-wrap items-center gap-2">
			<button
				type="button"
				onmousedown={(event) => event.preventDefault()}
				onclick={() => close()}
				class="flex h-7 shrink-0 items-center rounded-full px-3 text-fine text-txt3 transition-colors hover:text-txt2"
			>
				Cancel
			</button>
			<button
				type="button"
				onmousedown={(event) => event.preventDefault()}
				onclick={() => void save()}
				disabled={blank}
				class="flex h-7 shrink-0 items-center rounded-full border border-hairline px-3 text-fine text-txt2 transition hover:border-txt3 hover:text-txt active:bg-raised disabled:cursor-not-allowed disabled:text-txt4 disabled:hover:border-hairline"
			>
				Save
			</button>
		</div>
	{/if}
</div>
