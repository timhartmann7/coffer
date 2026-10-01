<script lang="ts">
	import {
		asFailure,
		chooseDatabase,
		chooseFound,
		chooseKeyFile,
		chooseRescue,
		chooseSnapshot,
		discardRescue,
		forgetKeyFile,
		leaveRescue,
		putBackRescue,
		snapshots,
		unlock,
		unlockTakingOver
	} from '$lib/ipc';
	import { at, fully } from '$lib/format';
	import type { CopyOf, Database, Failure, Found, OnDisk, Rescued, Snapshot } from '$lib/model';
	import Confirm from './Confirm.svelte';
	import Icon from './Icon.svelte';
	import Mark from './Mark.svelte';

	let {
		database,
		found = null,
		keyFile = null,
		reason = null,
		rescue = null,
		lost = false,
		file = null,
		copy = null,
		typed = false,
		typedBeside = false,
		onChoose,
		onKeyFile,
		onCreate,
		onGone,
		onUnlocked
	}: {
		database: Database | null;
		/**
		 * A vault Rust found in Coffer's own folder, offered on the first-run
		 * screen when nothing is remembered. A vault made by Coffer 0.1.0 was
		 * forgotten by the next launch, and this is how its owner gets back to it
		 * rather than making a second one.
		 */
		found?: Found | null;
		/**
		 * The key file this vault needs beside the password, when one has been
		 * chosen. Rust's answer rather than this window's: a lock destroys the
		 * window and the vault it is about is still the same one.
		 */
		keyFile?: Database | null;
		/**
		 * Why the vault that was open is not open any more. Rust's word, not the
		 * window's: the window that knew was destroyed, which is what locking
		 * means here.
		 */
		reason?: string | null;
		/**
		 * The copy a lock left beside this vault, when it could not save what
		 * was in the window. Rust's answer rather than this window's, for the
		 * same reason `reason` is: the window that knew was destroyed, and a
		 * copy an earlier run left is still worth offering.
		 */
		rescue?: Rescued | null;
		/** Whether the last lock had work to write and nowhere to put it. */
		lost?: boolean;
		/**
		 * The chosen file as the disk has it: what survived a lock that could
		 * write nothing, and whether the copy a lock left beside it has a vault
		 * to go over or only an empty name to go back into.
		 */
		file?: OnDisk | null;
		/** The vault the chosen file was copied from, when the chosen file is
		 * the copy a lock left. */
		copy?: CopyOf | null;
		/**
		 * Whether the last lock found text the reader was still typing and saved
		 * it into the vault. Only whether: after a lock nothing of the vault is
		 * left in memory to name the entry it went into, and this screen must
		 * not be what keeps one.
		 */
		typed?: boolean;
		/**
		 * Whether some of it was a new value typed in a Change field, kept in a
		 * field of its own beside the old one rather than over it. "Saved" alone
		 * reads as the new value being the field's now, and the reader who
		 * copies the old one somewhere it no longer works is the one misled.
		 * Named no more than `typed` is.
		 */
		typedBeside?: boolean;
		onChoose: (database: Database) => void;
		onKeyFile: (chosen: Database | null) => void;
		/** Offered on the first run, where there is nothing to open yet. */
		onCreate: () => void;
		/** The vault Rust found was not a vault any more when it was pressed.
		 * The status is read again, so that the card says what is there now. */
		onGone: () => void;
		onUnlocked: () => Promise<void>;
	} = $props();

	/** What happened, in a sentence. Every one of them ends the same way,
	 * because what matters is that the data is out of memory rather than which
	 * of three things put it there. */
	const said: Record<string, string> = {
		idle: 'Nothing happened here for a while.',
		sleeping: 'This Mac went to sleep.',
		screenLocked: 'The screen locked.',
		sessionSwitched: 'Somebody else signed in on this Mac.'
	};
	const why = $derived(reason ? (said[reason] ?? 'The vault was locked.') : null);

	let field = $state<HTMLInputElement>();
	let busy = $state(false);
	let failure = $state<Failure | null>(null);
	let snapshot = $state<Snapshot | null>(null);

	/**
	 * The one failure the field itself is about. Everything else - a file that
	 * will not open, a database somebody else has open, a file that is gone -
	 * is about the vault, and painting the password field red for those sends
	 * the reader back to a password that was right all along.
	 */
	const wrongPassword = $derived(failure?.code === 'wrongCredentials');

	/** A file that will not open at all. The offer to open a snapshot belongs to
	 * this and to nothing else. */
	const unreadable = $derived(
		failure !== null && (failure.code === 'damaged' || failure.code === 'notADatabase')
	);

	/** Somebody's lock file sits beside the vault. Coffer respects it, and the
	 * only thing that can tell a live one from one a power cut left behind is a
	 * person reading the machine and the hour out of the message. */
	const held = $derived(failure?.code === 'heldByAnother');

	/**
	 * Whether the next attempt takes that lock over.
	 *
	 * Set by a press, never by Coffer, and it costs the reader the password
	 * again: the first one is already derived and gone, and keeping it here to
	 * retry with would be the one thing this screen exists not to do.
	 */
	let takingOver = $state(false);

	/** Whether the reader has just taken the copy away. Rust is asked for the
	 * status once per window, so the offer has to be taken off the screen here
	 * rather than by reading it again. */
	let dropped = $state(false);

	/** Whether the reader pressed Remove and is being asked whether they mean
	 * it: the copy holds the only copy of those changes, or of the whole vault
	 * when the vault's file has gone. */
	let removing = $state(false);

	/** When the copy beside the vault was written, when the filesystem kept
	 * that. */
	const kept = $derived(rescue ? at(rescue.written, new Date()) : '');

	/** Why the copy could not be put back, when it could not. */
	let unmoved = $state<string | null>(null);

	/** Whether this disk cannot take the copy back without it being opened.
	 * Opening it is then the way it becomes the vault, so that is what the
	 * card offers instead. */
	let mustOpen = $state(false);

	/**
	 * The copy stands in for a vault whose file has gone.
	 *
	 * There is nothing to unlock, so the password is not asked for, and the
	 * copy goes back into the vault's name without one: nothing is opened. A
	 * vault that is there is never gone over from here - the copy is opened
	 * and looked at first, and only from inside it does it become the vault.
	 */
	const standIn = $derived(rescue !== null && !dropped && file?.there === false);

	/**
	 * What the copy stands in for, as the card names it. The chosen file is
	 * itself a copy when the reader was in one a lock had left and that lock
	 * kept their work beside it: then it is that copy's file that went, and the
	 * vault it came from is another file the card knows nothing new about.
	 * Saying "your vault" there sent a reader to put a copy back over a vault
	 * they believed was gone.
	 */
	const missing = $derived(copy ? 'This copy' : 'Your vault file');

	/** What survived a lock that could write nothing: the chosen file, as it
	 * was before the changes that were lost. That is the copy, when the lock
	 * that lost them was of a copy opened to look. */
	const survived = $derived.by(() => {
		if (!file) return null;
		const which = copy ? 'This copy' : 'Your vault file';
		if (!file.there) return `${which} is not where it was, either.`;
		const when = at(file.written, new Date());
		return when ? `${which} is as it was ${when}.` : `${which} is as it was before those changes.`;
	});

	/** The start of the sentence that says the chosen file is a copy, with
	 * when it was written when the filesystem kept that. */
	const copied = $derived.by(() => {
		const when = copy ? at(copy.saved, new Date()) : '';
		return when
			? `This is the copy a lock saved ${when} beside`
			: 'This is the copy a lock saved beside';
	});

	/**
	 * What a move that did not go says, in words about the move. The codes are
	 * the ones every command shares, and "there is already a file with that
	 * name" is a sentence about making a vault.
	 */
	const unmovedBecause: Record<string, string> = $derived({
		taken: `A file is back where ${copy ? 'this copy' : 'your vault'} was, so the copy was left where it is.`,
		gone: 'The copy is not there any more.',
		needsOpening: `This disk cannot take the copy back without it being opened. Open it, and ${copy ? 'put it back in this one’s place' : 'make it your vault'} from inside.`
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (busy || !database || !field) return;

		// The password is read out of the field and turned into bytes here. It
		// is never bound to a variable the framework keeps, and the field is
		// emptied before the call that could take a second comes back.
		const bytes = new TextEncoder().encode(field.value);
		field.value = '';

		const over = takingOver;
		busy = true;
		failure = null;
		snapshot = null;

		try {
			await (over ? unlockTakingOver(bytes) : unlock(bytes));
			takingOver = false;
			await onUnlocked();
		} catch (thrown) {
			failure = asFailure(thrown);
			if (unreadable) {
				snapshot = (await snapshots().catch(() => []))[0] ?? null;
			}
			field.focus();
		} finally {
			busy = false;
		}
	}

	/**
	 * Opens the copy a lock left, which is a vault of its own at another path.
	 *
	 * `sameVault` because it is the same database under the same credentials: a
	 * copy taken from a vault that wants a key file needs the same one, and
	 * forgetting it would leave it impossible to open.
	 */
	async function openRescue() {
		if (busy) return;
		try {
			chose(await chooseRescue(), true);
		} catch (thrown) {
			failure = asFailure(thrown);
		}
	}

	/** Opens the vault Rust found. Nothing is sent: Rust opens the file it
	 * named, so this window never names one. A file that has gone since takes
	 * the card with it. */
	async function openFound() {
		if (busy) return;
		busy = true;
		try {
			chose(await chooseFound());
		} catch (thrown) {
			failure = asFailure(thrown);
			if (failure.code === 'gone') onGone();
		} finally {
			busy = false;
		}
	}

	/** Takes the copy away, once the reader has answered the question about
	 * it: it holds the only copy of those changes. */
	async function dropRescue() {
		removing = false;
		if (busy) return;
		try {
			await discardRescue();
			dropped = true;
		} catch (thrown) {
			failure = asFailure(thrown);
		}
	}

	/**
	 * Moves the copy into the name of a vault whose file has gone. Nothing is
	 * sent: Rust moves it, and only into a name that holds nothing.
	 *
	 * Whatever stood in the way - a vault that came back, a copy that went -
	 * is a fact about the disk, so the offer is read again from there, and the
	 * screen says what it found rather than repeating what it believed.
	 */
	async function putBack() {
		if (busy || !database) return;
		const chosen = database;
		busy = true;
		unmoved = null;
		try {
			chose(await putBackRescue(), true);
		} catch (thrown) {
			const refused = asFailure(thrown);
			unmoved = unmovedBecause[refused.code] ?? refused.message;
			mustOpen = refused.code === 'needsOpening';
			onChoose(chosen);
		} finally {
			busy = false;
		}
	}

	/** Points the screen back at the vault the chosen copy was taken from.
	 * Nothing is sent: Rust reads the vault off the copy's name. */
	async function backToVault() {
		if (busy) return;
		try {
			chose(await leaveRescue(), true);
		} catch (thrown) {
			failure = asFailure(thrown);
		}
	}

	/** Asks for the key file, and says so upwards so that the screen and Rust
	 * hold one answer between them. */
	async function pickKeyFile() {
		if (busy) return;
		try {
			const picked = await chooseKeyFile();
			if (picked) {
				onKeyFile(picked);
				failure = null;
				field?.focus();
			}
		} catch (thrown) {
			failure = asFailure(thrown);
		}
	}

	async function dropKeyFile() {
		if (busy) return;
		await forgetKeyFile().catch(() => {});
		onKeyFile(null);
		field?.focus();
	}

	async function choose() {
		// Not while a key is deriving: the unlock in flight would finish over
		// whatever is picked here, and Rust would then refuse it as stale.
		if (busy) return;

		const picked = await chooseDatabase().catch((thrown) => {
			failure = asFailure(thrown);
			return null;
		});
		if (picked) chose(picked);
	}

	async function openSnapshot(index: number) {
		try {
			chose(await chooseSnapshot(index), true);
		} catch (thrown) {
			failure = asFailure(thrown);
		}
	}

	/**
	 * Points the screen at another file.
	 *
	 * `sameVault` is true for a snapshot or the copy a lock left, each a copy of
	 * the vault that was already chosen, and for the vault a copy was taken
	 * from: all of them open with the same credentials. Rust keeps the key file
	 * for those and forgets it for anything else, and this has to say the same
	 * thing: a screen that showed no key file while Rust was still using one
	 * would be a screen nobody could reason about.
	 *
	 * A lock somebody said to take over is this file's lock and nothing else's,
	 * so that goes either way.
	 */
	function chose(picked: Database, sameVault = false) {
		onChoose(picked);
		failure = null;
		snapshot = null;
		unmoved = null;
		mustOpen = false;
		removing = false;
		takingOver = false;
		if (!sameVault) onKeyFile(null);
		field?.focus();
	}

	$effect(() => {
		field?.focus();
	});
</script>

<div class="flex flex-1 items-center justify-center overflow-y-auto px-10 py-10">
	<div class="w-full max-w-[320px] animate-rise">
		<Mark class="mx-auto h-11 w-11 text-txt" />
		{#if why}
			<h1 class="mt-6 text-center text-title font-medium tracking-tight text-txt">Locked</h1>
			<p class="mx-auto mt-3 max-w-[38ch] text-center text-body leading-relaxed text-txt2">
				{why} What was in the vault has been wiped out of memory - the password opens it again.
			</p>
		{:else}
			<div class="mt-5 text-center text-lead font-bold tracking-wordmark">COFFER</div>
		{/if}

		<!-- The one thing the lock has to add about the reader's own work, and
		     in the same place the two lines about work it could not save stand.
		     It names no entry: none is left in memory to name. -->
		{#if typed}
			<p class="mx-auto mt-6 max-w-[38ch] text-center text-small leading-relaxed text-txt2">
				What you were typing was saved before locking.
				{#if typedBeside}
					A new value you had not saved yet was kept in a field of its own, beside the old one,
					which is unchanged.
				{/if}
			</p>
		{/if}

		<!-- Standing news about a file beside the vault, so it sits above the
		     password rather than inside the panel that reports a failure to open
		     this one. It is as true on a first launch as it is straight after
		     the lock that wrote it. -->
		{#if lost}
			<p class="mx-auto mt-6 max-w-[38ch] text-center text-small leading-relaxed text-danger">
				There were changes the vault had not taken, and Coffer could not write them anywhere.
			</p>
			<!-- What is left is as much the news as what is not, and the reader's
			     next question: whether the vault they are about to open is the
			     one they last saw. -->
			{#if survived}
				<p class="mx-auto mt-2 max-w-[38ch] text-center text-small leading-relaxed text-txt2">
					{survived}
				</p>
			{/if}
		{/if}

		{#if rescue && !dropped}
			<div class="mt-6 rounded-sm border border-hairline bg-surface2 px-4 py-4">
				<div class="flex items-center gap-2">
					<Icon name="warn" class="h-4 w-4 shrink-0 text-warn" />
					<span class="text-body text-txt">Work that never reached the vault</span>
				</div>
				<p class="mt-2 text-fine leading-relaxed text-txt2">
					A lock could not save it, so Coffer put it in
					<bdi class="font-mono text-txt">{rescue.name}</bdi>{kept ? ` ${kept}` : ''}.
					{#if standIn && !mustOpen}
						{missing} is not there any more, so the copy can go back in its place with nothing to type.
					{:else if standIn && copy}
						This copy is not there any more. The copy kept of it opens with the same password, and
						inside it you can put it back in this one’s place.
					{:else if standIn}
						Your vault file is not there any more. The copy opens with the same password, and inside
						it you can make it your vault.
					{:else}
						It opens with the same password, and inside it you can make it your vault or come back
						to this one.
					{/if}
				</p>
				{#if removing}
					<Confirm
						bare
						class="mt-4 border-t border-hairline pt-3"
						question={!standIn
							? 'Remove the only copy of those changes?'
							: copy
								? 'Remove the only copy of what this copy held?'
								: 'Remove the only copy of your vault?'}
						act="Remove"
						onKeep={() => (removing = false)}
						onAct={dropRescue}
					/>
				{:else}
					<div class="mt-4 flex flex-wrap gap-2">
						<!-- The one call this card makes is the accent only when there is
						     no vault to unlock instead: then it is the screen's call. On a
						     disk that cannot take the copy back unopened, opening it is
						     that call. -->
						{#if standIn && !mustOpen}
							<button
								type="button"
								onclick={putBack}
								disabled={busy}
								class="h-9 rounded-full bg-accent px-4 text-small font-medium text-canvas transition-colors hover:bg-accenthi active:bg-accenthi disabled:cursor-not-allowed"
							>
								Put this copy back as my vault
							</button>
						{:else}
							<button
								type="button"
								onclick={openRescue}
								disabled={busy}
								class="h-9 rounded-full px-4 text-small transition-colors disabled:cursor-not-allowed {standIn
									? 'bg-accent font-medium text-canvas hover:bg-accenthi active:bg-accenthi'
									: 'border border-hairline text-txt hover:border-txt3 active:bg-surface2'}"
							>
								Open the copy to look
							</button>
						{/if}
						<button
							type="button"
							onclick={() => (removing = true)}
							disabled={busy}
							class="h-9 rounded-full px-3 text-small text-txt3 transition-colors hover:bg-dangerwash hover:text-danger disabled:cursor-not-allowed"
						>
							Remove it…
						</button>
					</div>
				{/if}
			</div>
		{/if}

		{#if unmoved}
			<p class="mx-auto mt-3 max-w-[38ch] text-center text-small leading-relaxed text-danger">
				{unmoved}
			</p>
		{/if}

		<!-- The chosen file is a copy, and the unlock screen is where a reader
		     who opened it to look finds their way back: before this, only a file
		     panel pointed at the vault again. -->
		{#if copy}
			<div class="mt-6 rounded-sm border border-hairline bg-surface2 px-4 py-4">
				<div class="flex items-center gap-2">
					<Icon name="warn" class="h-4 w-4 shrink-0 text-warn" />
					<span class="text-body text-txt">A copy, not your vault</span>
				</div>
				<p class="mt-2 text-fine leading-relaxed text-txt2">
					{copied}
					<bdi class="font-mono text-txt">{copy.vault}</bdi>. It opens with the same password.
				</p>
				<button
					type="button"
					onclick={backToVault}
					disabled={busy}
					class="mt-4 h-9 rounded-full border border-hairline px-4 text-small text-txt transition-colors hover:border-txt3 active:bg-surface2 disabled:cursor-not-allowed"
				>
					Back to my vault
				</button>
			</div>
		{/if}

		{#if database}
			<button
				type="button"
				onclick={choose}
				disabled={busy}
				class="mt-8 flex w-full items-center gap-3 rounded-sm border border-hairline bg-surface2 px-3 py-2.5 text-left transition-colors hover:border-txt4 disabled:cursor-not-allowed disabled:hover:border-hairline"
			>
				<Icon name="disk" class="h-4 w-4 shrink-0 text-txt4" />
				<span class="min-w-0 flex-1">
					<span class="block truncate text-body text-txt">{database.name}</span>
					<span class="block truncate font-mono text-meta text-txt4">{database.path}</span>
				</span>
				<Icon name="chev-d" class="h-4 w-4 shrink-0 text-txt4" />
			</button>

			<!-- Nothing to unlock while the copy stands in for a vault whose file
			     has gone: the card above is the whole of what can happen next. -->
			<form onsubmit={submit} hidden={standIn}>
				<label class="mt-4 block">
					<span class="mb-2 block font-mono text-label tracking-label text-txt3 uppercase">
						Vault password
					</span>
					<span
						class="flex items-center gap-2 rounded-sm border bg-surface2 px-3 py-3 {wrongPassword
							? 'border-danger/60'
							: 'border-hairline focus-within:border-accent focus-within:ring-4 focus-within:ring-accent/15'} transition"
					>
						<Icon name="lock" class="h-4 w-4 shrink-0 text-txt3" />
						<input
							bind:this={field}
							type="password"
							autocomplete="off"
							spellcheck="false"
							class="min-w-0 flex-1 bg-transparent tracking-mask text-txt outline-none"
						/>
					</span>
				</label>

				{#if wrongPassword && failure}
					<p class="mt-3 text-small text-danger">{failure.message}</p>
				{/if}

				<!-- Coffer never makes a vault that wants a key file, so this is
				     folded away until it is asked for. A vault whose owner chose
				     one in another client has no other way in. -->
				{#if keyFile}
					<div
						class="mt-3 flex items-center gap-2 rounded-sm border border-hairline bg-surface2 px-3 py-2"
					>
						<Icon name="key" class="h-4 w-4 shrink-0 text-txt4" />
						<span class="min-w-0 flex-1 truncate font-mono text-meta text-txt2">
							{keyFile.name}
						</span>
						<button
							type="button"
							onclick={dropKeyFile}
							disabled={busy}
							aria-label="Do not use a key file"
							class="shrink-0 text-txt4 transition-colors hover:text-txt2 disabled:cursor-not-allowed"
						>
							<Icon name="x" class="h-4 w-4" />
						</button>
					</div>
				{:else}
					<button
						type="button"
						onclick={pickKeyFile}
						disabled={busy}
						class="mt-3 text-fine text-txt3 transition-colors hover:text-txt2 disabled:cursor-not-allowed"
					>
						This vault also needs a key file
					</button>
				{/if}

				<button
					type="submit"
					disabled={busy}
					class="mt-5 h-[46px] w-full rounded-full px-6 text-base font-medium transition-colors {busy
						? 'cursor-not-allowed bg-surface2 text-txt4'
						: 'bg-accent text-canvas hover:bg-accenthi active:bg-accenthi'}"
				>
					{#if busy}
						Unlocking…
					{:else if takingOver}
						Open anyway
					{:else}
						Unlock
					{/if}
				</button>
			</form>

			{#if failure && !wrongPassword}
				<div class="mt-6 border-t border-line pt-5 text-center">
					<p class="flex items-center justify-center gap-2 text-lead text-txt">
						<Icon name="warn" class="h-5 w-5 shrink-0 text-warn" />
						{failure.message}
					</p>
					{#if snapshot}
						<p class="mx-auto mt-3 max-w-[38ch] text-small leading-relaxed text-txt2">
							{#if snapshot.taken}
								A snapshot from {fully(snapshot.taken, new Date())} sits beside it.
							{:else}
								A snapshot Coffer took before one of its own saves sits beside it.
							{/if}
							It opens with the same password.
						</p>
						{@const only = snapshot}
						<button
							type="button"
							onclick={() => openSnapshot(only.index)}
							class="mt-5 h-9 rounded-full border border-hairline px-5 text-small text-txt transition-colors hover:border-txt3 active:bg-surface2"
						>
							Open <bdi>{only.name}</bdi>
						</button>
					{/if}

					<!-- The lock file says who and when, and the reader is the only
					     one who can tell a Coffer running right now from a Mac that
					     lost power with the vault open. Without this the second is a
					     vault nothing here could ever open again. -->
					{#if held}
						{#if takingOver}
							<p class="mx-auto mt-3 max-w-[38ch] text-small leading-relaxed text-txt2">
								Type the password again and Coffer will open the vault and take the lock. If the
								other Coffer is really running, whichever saves last wins.
							</p>
						{:else}
							<p class="mx-auto mt-3 max-w-[38ch] text-small leading-relaxed text-txt2">
								If that Mac lost power or Coffer was force-quit, the lock is left over and nothing
								is holding the vault.
							</p>
							<button
								type="button"
								onclick={() => {
									takingOver = true;
									field?.focus();
								}}
								class="mt-5 h-9 rounded-full border border-hairline px-5 text-small text-txt transition-colors hover:border-txt3 active:bg-surface2"
							>
								Open it anyway
							</button>
						{/if}
					{/if}
				</div>
			{/if}

			<!-- The same two ways out, wherever the screen got to. A vault that
			     will not open is the moment somebody most wants to make another
			     one, and that is where the offer used to disappear. -->
			<div class="mt-6 flex justify-center gap-6 text-fine text-txt3">
				<button
					type="button"
					onclick={choose}
					class="transition-colors hover:text-txt2 active:text-txt3"
				>
					Open another database
				</button>
				<button
					type="button"
					onclick={onCreate}
					class="transition-colors hover:text-txt2 active:text-txt3"
				>
					Create new
				</button>
			</div>
		{:else}
			<h1
				class="mx-auto mt-7 max-w-[22ch] text-center text-display leading-snug font-medium tracking-tight text-txt"
			>
				Your passwords will live here
			</h1>
			<p class="mx-auto mt-4 max-w-[46ch] text-center text-base leading-relaxed text-txt2">
				All of it goes into one file on this Mac. One password opens it, and that is the one you
				choose next.
			</p>
			<!-- Above the offer to make one, because somebody with a vault already
			     on this Mac is about to make a second one and wonder where their
			     passwords went. The same card the copy a lock left is offered in. -->
			{#if found}
				<div class="mt-8 rounded-sm border border-hairline bg-surface2 px-4 py-4">
					<div class="flex items-start gap-2">
						<Icon name="disk" class="mt-0.5 h-4 w-4 shrink-0 text-txt4" />
						<!-- A vault whose file has gone, with the copy a lock kept of it
						     beside its name, is offered under that name: opening it is
						     where the copy is put back. The sentence must not say the
						     vault's own file was found. -->
						<p class="text-body leading-relaxed text-txt">
							{#if found.copy}
								We found a copy of your vault <bdi class="font-mono">{found.name}</bdi> in
								<bdi>{found.folder}</bdi> (your home folder), kept when it last locked. The vault’s own
								file is not there. Open it to put the copy back.
							{:else}
								We found your vault: <bdi class="font-mono">{found.name}</bdi> in
								<bdi>{found.folder}</bdi> (your home folder).
							{/if}
						</p>
					</div>
					<button
						type="button"
						onclick={openFound}
						disabled={busy}
						class="mt-4 h-9 rounded-full border border-hairline px-4 text-small text-txt transition-colors hover:border-txt3 active:bg-surface2"
					>
						Open it
					</button>
				</div>
			{/if}
			<button
				type="button"
				onclick={onCreate}
				class="mx-auto {found
					? 'mt-6'
					: 'mt-9'} block h-[46px] w-full max-w-[300px] rounded-full bg-accent px-6 text-base font-medium text-canvas transition-colors hover:bg-accenthi"
			>
				Make a vault
			</button>
			<div class="mt-4 text-center">
				<button
					type="button"
					onclick={choose}
					class="text-small text-txt2 underline decoration-hairline underline-offset-4 transition-colors hover:text-txt"
				>
					I already have a vault file
				</button>
				<p class="mt-2 text-fine text-txt4">
					A .kdbx file from another app, or from another computer, will do
				</p>
			</div>
			{#if failure}
				<p class="mt-4 text-center text-small text-danger">{failure.message}</p>
			{/if}
		{/if}
	</div>
</div>
