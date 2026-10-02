<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { flush, type Place } from '$lib/drafts';
	import { quoted } from '$lib/format';
	import { cancels, composing } from '$lib/lines';
	import { masked, type Field, type Span } from '$lib/model';
	import Changer from './Changer.svelte';
	import Confirm from './Confirm.svelte';
	import Editable from './Editable.svelte';
	import Generator from './Generator.svelte';
	import Icon from './Icon.svelte';
	import ProtectedValue from './ProtectedValue.svelte';

	/**
	 * One field of the reader's own: its name, its value, and what can be done
	 * to each.
	 *
	 * Every value here can be copied through Rust, so nobody selects one in a
	 * field they could change by accident and has it land on the ordinary
	 * pasteboard. Every field can be hidden or shown with the lock beside its
	 * name, which moves the value from one kind of storage to the other inside
	 * Rust. A hidden one can be given a made value as well as a typed one, the
	 * way the password can. And the name is pressed to be changed, which takes
	 * the value with it rather than asking for it again.
	 */
	let {
		entry,
		field,
		draft,
		multiline,
		fresh,
		renamed,
		readOnly,
		removing,
		onCopy,
		onWrite,
		onPut,
		onProtect,
		onRename,
		onRemove,
		onKeep,
		onFailure
	}: {
		entry: string;
		field: Field;
		/** Where a value typed into the field goes, and how it is protected. */
		draft: Place;
		/** The reader asked for lines when they named the field. */
		multiline: boolean;
		/** The reader has just named the field with Return, and is about to give
		 * it a value: the value's field takes the focus as it is drawn. */
		fresh: boolean;
		/** The reader has just given the field this name with Return, and the
		 * focus goes back to the name, as it does after Escape. */
		renamed: boolean;
		/** Nothing here may be changed: a database Coffer will not write back,
		 * or an entry in the recycle bin. */
		readOnly: boolean;
		/** Whether the row is asking if a removal Rust said would be for good is
		 * what the reader wants. */
		removing: boolean;
		onCopy: (range: Span | null) => void;
		/** Writes a value typed where it stands, and says whether it was taken. */
		onWrite: (value: string) => Promise<boolean>;
		/** Writes a new value: a typed one from Change, or a made one. Rejects
		 * with what the vault said when it would not take it. */
		onPut: (value: string) => Promise<void>;
		/** Hides the field on `entry`, or stops hiding it. The entry is the one
		 * the press was made on, read at the press. */
		onProtect: (entry: string, protect: boolean) => Promise<void>;
		/** Gives the field on `entry` another name, and says whether it took.
		 * `typed` is whether it was finished with Return rather than by leaving
		 * the name. */
		onRename: (entry: string, to: string, typed: boolean) => Promise<boolean>;
		/** Takes the field off. `forever` is the answer to the question Rust
		 * asks when nothing would bring it back. */
		onRemove: (forever: boolean) => void;
		/** The reader kept the field after all. */
		onKeep: () => void;
		onFailure: (thrown: unknown) => void;
	} = $props();

	const hidden = $derived(masked(field));

	/** Whether a new value is being written for the field in a Change of its
	 * own. Neither the name nor the lock may move under it: the new value is
	 * on its way to this field, under this name and this protection. */
	let changing = $state(false);
	let generating = $state(false);
	let renaming = $state(false);
	/**
	 * Whether the lock's answer or a new name is on its way.
	 *
	 * The row drawn when it lands may have no Change in it, or be another row
	 * altogether, so nothing may be opened or typed meanwhile: a Change opened
	 * then went with everything typed in it. A second press of the lock would
	 * undo the first before either had landed.
	 */
	let pending = $state(false);
	/** Whether what is on its way is a new name. What is typed into the value
	 * meanwhile would be written under the old one, which by the time it
	 * lands may be no field at all - and writing it would make it one. */
	let retitling = $state(false);
	let maker = $state<HTMLButtonElement>();
	let namer = $state<HTMLButtonElement>();

	// The generator is Make one's, which is offered for a hidden field only. A
	// field shown in the open while it was up would take a made password as
	// plain text. And a Change and the generator are two ways to give the field
	// a new value: a made value put in while a typed one waits beside it would
	// be overwritten by the typed one a moment later.
	$effect(() => {
		if (changing || !field.protected) generating = false;
	});

	/** The made value goes straight in: putting it there is the press that
	 * asked for it. The focus goes back to Make one, which may have been drawn
	 * again by then, unless the reader has put it somewhere since. */
	function insert(made: string) {
		onPut(made)
			.then(() => tick())
			.then(() => {
				if (document.activeElement === document.body) maker?.focus();
			}, onFailure);
	}

	function closeGenerator() {
		generating = false;
		maker?.focus();
	}

	/**
	 * The lock, pressed. What it is to become, and on which entry, is read at
	 * the press, before anything typed in the field is finished: leaving the
	 * field to press it wrote that value, and the value has to land before the
	 * protection changes, or the entry drawn last would be the one from before
	 * the press. The reader may have moved to another entry by then, and the
	 * press was about this one.
	 */
	async function protect() {
		if (pending) return;
		const on = entry;
		const wanted = !field.protected;
		// A made value put in while the field is on its way into the open
		// would land in the open, whichever of the two arrived first.
		if (!wanted) generating = false;
		pending = true;
		try {
			await flush();
			await onProtect(on, wanted);
		} catch (thrown) {
			onFailure(thrown);
		} finally {
			pending = false;
		}
	}

	function writing(element: HTMLInputElement) {
		element.focus();
		element.select();
	}

	/** The name takes the focus as it is drawn after a rename finished with
	 * Return, unless the reader has put it somewhere since. */
	function retitled(button: HTMLButtonElement) {
		if (!untrack(() => renamed)) return;
		if (document.activeElement && document.activeElement !== document.body) return;
		button.focus();
	}

	/** Gives the focus back to the name, rather than to nowhere - where the
	 * next Escape closes the entry. */
	function back() {
		void tick().then(() => {
			if (document.activeElement === document.body) namer?.focus();
		});
	}

	/**
	 * Finishes the name, the way the name of a new field is finished: Return or
	 * leaving it. A name left as it was changes nothing, even one with spaces
	 * at either end, which only a typed edit takes off; so does nothing at all.
	 */
	async function finish(element: HTMLInputElement, typed: boolean) {
		if (!renaming) return;
		renaming = false;
		const to = element.value.trim();
		if (element.value === field.name || to === '' || to === field.name) {
			if (typed) back();
			return;
		}
		const on = entry;
		pending = true;
		retitling = true;
		let taken = false;
		try {
			await flush();
			taken = await onRename(on, to, typed);
		} catch (thrown) {
			onFailure(thrown);
		} finally {
			pending = false;
			retitling = false;
		}
		if (!taken && typed) back();
	}

	function keys(event: KeyboardEvent & { currentTarget: HTMLInputElement }) {
		if (cancels(event)) {
			// The window reads Escape as "close the entry", and a name put back
			// is not an entry put away.
			event.stopPropagation();
			renaming = false;
			back();
			return;
		}
		if (event.key === 'Enter' && !composing(event)) {
			event.preventDefault();
			void finish(event.currentTarget, true);
		}
	}
</script>

{#snippet make()}
	<button
		bind:this={maker}
		type="button"
		onclick={() => (generating = !generating)}
		disabled={pending}
		aria-expanded={generating}
		class="text-fine transition-colors hover:text-txt disabled:cursor-not-allowed disabled:hover:text-txt3 {generating
			? 'text-txt'
			: 'text-txt3'}"
		aria-label="Make one for {field.name}"
	>
		Make one
	</button>
{/snippet}

<div class="mt-3">
	<div class="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-3">
		{#if renaming}
			<!-- The whole row while it is being renamed: ninety-six pixels is a
			     column to read a name in, not to write one. -->
			<input
				{@attach writing}
				type="text"
				value={field.name}
				autocomplete="off"
				spellcheck="false"
				aria-label="New name for {field.name}"
				onkeydown={keys}
				onblur={(event) => void finish(event.currentTarget, false)}
				class="col-span-2 min-w-0 rounded-sm border border-accent bg-surface2 px-3 py-2 text-small text-txt ring-4 ring-accent/15 outline-none"
			/>
		{:else}
			<!--
				The name's column holds the lock before the name. It says whether
				the value is hidden, which an empty field shows no other way, and
				it is there on every row rather than in the value's column, where
				it took the width a revealed value is read in.
			-->
			<span class="flex w-24 min-w-0 items-center gap-1.5">
				{#if !readOnly}
					<button
						type="button"
						onclick={() => void protect()}
						disabled={changing || pending}
						aria-pressed={field.protected}
						class="shrink-0 text-txt4 transition-colors hover:text-txt2 disabled:cursor-not-allowed disabled:hover:text-txt4"
						title={changing
							? 'Save or cancel the new value first.'
							: field.protected
								? 'Hidden until shown. Press to keep it in the open.'
								: 'Kept in the open. Press to hide it.'}
						aria-label="Keep {field.name} hidden"
					>
						<Icon name={field.protected ? 'lock' : 'unlock'} class="h-3.5 w-3.5" />
					</button>
				{/if}
				{#if readOnly || changing || generating || pending}
					<span class="min-w-0 truncate text-small text-txt2" title={field.name}>{field.name}</span>
				{:else}
					<button
						bind:this={namer}
						{@attach retitled}
						type="button"
						onclick={() => (renaming = true)}
						class="min-w-0 truncate text-left text-small text-txt2 transition-colors hover:text-txt"
						title={field.name}
						aria-label="Rename {field.name}"
					>
						{field.name}
					</button>
				{/if}
			</span>

			<!--
				The trash is the last thing in the row and is there only while the
				row is under the pointer or holds the focus, in a box the size of a
				fingertip and a step further off than the gap between the others.
			-->
			<div class="group flex min-w-0 items-center gap-3">
				{#if hidden}
					<ProtectedValue {entry} field={field.name} {onCopy} {onFailure} />
				{:else}
					<!-- A protected field that is empty comes back with no value to
					     reveal, and is written to protected. A field of the reader's
					     own may be given lines, so a paste keeps its breaks. -->
					<Editable
						value={field.value ?? ''}
						label={field.name}
						placeholder="Empty"
						mono
						breaks
						{multiline}
						readonly={readOnly || retitling}
						focused={fresh}
						{draft}
						onCommit={onWrite}
					/>
					{#if !field.empty}
						<button
							type="button"
							onclick={() => onCopy(null)}
							class="shrink-0 text-txt4 transition-colors hover:text-txt2"
							aria-label="Copy {field.name}"
						>
							<Icon name="copy" class="h-4 w-4" />
						</button>
					{/if}
				{/if}
				{#if !readOnly && !removing}
					<button
						type="button"
						onclick={() => onRemove(false)}
						class="ml-2 flex h-7 w-7 shrink-0 items-center justify-center rounded-sm text-txt4 opacity-0 transition group-focus-within:opacity-100 group-hover:opacity-100 hover:bg-dangerwash hover:text-danger"
						aria-label="Remove the field {field.name}"
					>
						<Icon name="trash" class="h-4 w-4" />
					</button>
				{/if}
			</div>

			<!-- Under the value, in the value's column: a protected value's Change,
			     and Make one for any hidden field, the way the password has both. -->
			{#if !readOnly && field.protected}
				<div class="col-start-2 min-w-0">
					{#if hidden}
						<Changer
							{entry}
							name={field.name}
							multiline={field.lines || multiline}
							{draft}
							disabled={pending}
							bind:open={changing}
							onSave={onPut}
							{onFailure}
						>
							{@render make()}
						</Changer>
					{:else}
						<div class="mt-1 flex items-center gap-x-3">{@render make()}</div>
					{/if}
				</div>
			{/if}
		{/if}
	</div>

	{#if generating && !readOnly && field.protected}
		<div class="mt-3 animate-rise">
			<Generator
				purpose="field"
				label="Generator for {field.name}"
				onInsert={insert}
				onClose={closeGenerator}
				{onFailure}
			/>
		</div>
	{/if}

	{#if removing}
		<Confirm
			class="mt-3 bg-surface2"
			question="Remove {quoted(
				field.name
			)}? This vault keeps no version to bring it back from, so this can’t be undone."
			act="Remove"
			{onKeep}
			onAct={() => onRemove(true)}
		/>
	{/if}
</div>
