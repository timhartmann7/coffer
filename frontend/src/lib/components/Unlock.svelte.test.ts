import { flushSync, mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import Unlock from './Unlock.svelte';

const ipc = vi.hoisted(() => ({
	unlock: vi.fn(),
	unlockTakingOver: vi.fn(),
	snapshots: vi.fn(),
	chooseDatabase: vi.fn(),
	chooseFound: vi.fn(),
	chooseSnapshot: vi.fn(),
	chooseRescue: vi.fn(),
	discardRescue: vi.fn(),
	putBackRescue: vi.fn(),
	leaveRescue: vi.fn(),
	chooseKeyFile: vi.fn(),
	forgetKeyFile: vi.fn(),
	asFailure: (thrown: unknown) => thrown as { code: string; message: string }
}));
vi.mock('$lib/ipc', () => ipc);

const database = { path: '/Users/someone/personal.kdbx', name: 'personal' };

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.snapshots.mockResolvedValue([]);
	ipc.forgetKeyFile.mockResolvedValue(undefined);
});

afterEach(() => {
	host.remove();
});

function open(onUnlocked = vi.fn().mockResolvedValue(undefined)) {
	const component = mount(Unlock, {
		target: host,
		props: {
			database,
			onChoose: vi.fn(),
			onKeyFile: vi.fn(),
			onCreate: vi.fn(),
			onUnlocked
		}
	});
	flushSync();
	return component;
}

function field(): HTMLInputElement {
	const found = host.querySelector('input');
	if (!found) throw new Error('there is no password field');
	return found;
}

function submit(password: string) {
	field().value = password;
	host
		.querySelector('form')
		?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
}

function reads(): string {
	return (host.textContent ?? '').replace(/\s+/g, ' ').trim();
}

function buttons(): string[] {
	return [...host.querySelectorAll('button')].map((each) => each.textContent?.trim() ?? '');
}

function button(label: string): HTMLButtonElement {
	const found = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === label
	);
	if (!found) throw new Error(`there is no button called ${label}`);
	return found;
}

it('sends the password as bytes and empties the field before the answer', async () => {
	ipc.unlock.mockResolvedValue(undefined);
	const component = open();

	submit('correct horse battery staple');
	expect(field().value).toBe('');

	await vi.waitFor(() => expect(ipc.unlock).toHaveBeenCalledTimes(1));
	const sent = ipc.unlock.mock.calls[0][0];
	expect(sent).toBeInstanceOf(Uint8Array);
	expect(new TextDecoder().decode(sent)).toBe('correct horse battery staple');

	return unmount(component);
});

it('marks the field only when the password is the thing that was wrong', async () => {
	ipc.unlock.mockRejectedValue({ code: 'wrongCredentials', message: 'wrong password or key file' });
	const component = open();

	submit('nope');
	await vi.waitFor(() => expect(reads()).toContain('wrong password or key file'));
	flushSync();

	expect(host.innerHTML).toContain('border-danger/60');
	expect(host.querySelector('use[href="#i-warn"]')).toBeNull();

	return unmount(component);
});

/** A database somebody else has open, a file that is gone, a file that is not a
 * database: none of those are a password the reader should retype. */
it('does not blame the password for a vault that will not open', async () => {
	ipc.unlock.mockRejectedValue({
		code: 'heldByAnother',
		message: 'the database is open in another process'
	});
	const component = open();

	submit('correct horse battery staple');
	await vi.waitFor(() => expect(reads()).toContain('the database is open in another process'));
	flushSync();

	expect(host.innerHTML).not.toContain('border-danger/60');
	expect(host.querySelector('use[href="#i-warn"]')).not.toBeNull();
	// The way out is the same pair the screen always offers, wherever it got to:
	// a vault that will not open is the moment somebody most wants another one.
	expect(reads()).toContain('Open another database');
	expect(reads()).toContain('Create new');

	return unmount(component);
});

it('offers the newest snapshot when the file itself will not open', async () => {
	ipc.unlock.mockRejectedValue({ code: 'damaged', message: 'the database body is damaged' });
	ipc.snapshots.mockResolvedValue([
		{ name: 'personal.kdbx.1.bak', index: 1, taken: '2026-08-27T18:40:00Z' }
	]);
	const component = open();

	submit('correct horse battery staple');
	await vi.waitFor(() => expect(reads()).toContain('personal.kdbx.1.bak'));
	flushSync();

	expect(reads()).toContain('A snapshot from 27 Aug 2026 sits beside it');
	expect(ipc.snapshots).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/** A filesystem that keeps no modification time still has a snapshot worth
 * offering, and the sentence must not say the word "unknown". */
it('offers a snapshot whose date the filesystem lost', async () => {
	ipc.unlock.mockRejectedValue({ code: 'damaged', message: 'the database body is damaged' });
	ipc.snapshots.mockResolvedValue([{ name: 'personal.kdbx.1.bak', index: 1, taken: null }]);
	const component = open();

	submit('correct horse battery staple');
	await vi.waitFor(() => expect(reads()).toContain('personal.kdbx.1.bak'));
	flushSync();

	expect(reads()).not.toContain('unknown');
	expect(reads()).toContain('It opens with the same password');

	return unmount(component);
});

/** Choosing another database while a key is deriving would be answered by an
 * unlock that finished afterwards. */
it('will not let another database be chosen while one is opening', async () => {
	let finish: () => void = () => {};
	ipc.unlock.mockReturnValue(
		new Promise<void>((resolve) => {
			finish = resolve;
		})
	);
	const component = open();

	submit('correct horse battery staple');
	await tick();
	flushSync();

	const another = [...host.querySelectorAll('button')].find(
		(each) => each.textContent?.trim() === 'Open another database'
	);
	another?.click();
	flushSync();
	expect(ipc.chooseDatabase).not.toHaveBeenCalled();

	finish();
	return unmount(component);
});

/** Screen 05: the same password path, with a sentence above it saying what
 * happened. The reason is Rust's word - the window that knew was destroyed,
 * which is what locking means here. */
it('says why the vault locked, when there is something to say', () => {
	for (const [reason, said] of [
		['idle', 'Nothing happened here for a while.'],
		['sleeping', 'This Mac went to sleep.'],
		['screenLocked', 'The screen locked.'],
		['sessionSwitched', 'Somebody else signed in on this Mac.']
	]) {
		const component = mount(Unlock, {
			target: host,
			props: {
				database,
				reason,
				onChoose: vi.fn(),
				onKeyFile: vi.fn(),
				onCreate: vi.fn(),
				onUnlocked: vi.fn()
			}
		});
		flushSync();

		expect(host.textContent).toContain('Locked');
		expect(host.textContent).toContain(said);
		expect(host.textContent).toContain('wiped out of memory');
		// One password path, not two: the field is the same one.
		expect(host.querySelector('input[type="password"]')).not.toBeNull();

		unmount(component);
		host.innerHTML = '';
	}
});

/** A lock the reader asked for has nothing to explain, and a screen that said
 * "you were away" after they pressed the button would be wrong. */
it('says nothing about a lock nobody has to explain', () => {
	const component = open();

	expect(host.textContent).toContain('COFFER');
	expect(host.textContent).not.toContain('Locked');

	unmount(component);
});

/** A word from a later version of Rust than this window. Better a plain
 * sentence than an empty screen. */
it('has something to say about a reason it does not know', () => {
	const component = mount(Unlock, {
		target: host,
		props: {
			database,
			reason: 'somethingLater',
			onChoose: vi.fn(),
			onKeyFile: vi.fn(),
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();

	expect(host.textContent).toContain('The vault was locked.');
	unmount(component);
});

/**
 * A lock file a crash left behind cannot be told from one a Coffer is holding
 * right now: the process id belongs to a Mac that has rebooted since, or the
 * host name has changed. Without this the vault could never be opened again
 * from inside the application, and the only way back in was deleting a sibling
 * file nothing on the screen names.
 */
it('offers to take over a lock, and takes the password again to do it', async () => {
	ipc.unlock.mockRejectedValue({
		code: 'heldByAnother',
		message: 'someone has it open on another-mac, since 2026-08-30T10:00:00Z'
	});
	ipc.unlockTakingOver.mockResolvedValue(undefined);
	const component = open();

	submit('correct horse battery staple');
	await vi.waitFor(() => expect(reads()).toContain('another-mac'));
	flushSync();

	const anyway = [...host.querySelectorAll('button')].find(
		(each) => each.textContent?.trim() === 'Open it anyway'
	);
	expect(anyway, 'a lock nobody can prove stale left no way in').toBeDefined();
	anyway?.click();
	flushSync();

	// Nothing has happened yet: the first password is derived and gone, and
	// keeping it here to retry with is the one thing this screen must not do.
	expect(ipc.unlockTakingOver).not.toHaveBeenCalled();
	expect(reads()).toContain('Type the password again');

	submit('correct horse battery staple');
	await vi.waitFor(() => expect(ipc.unlockTakingOver).toHaveBeenCalledTimes(1));
	expect(ipc.unlock, 'the second attempt went through the ordinary door').toHaveBeenCalledTimes(1);

	return unmount(component);
});

/** Nothing decides a take-over on the reader's behalf: an ordinary unlock is
 * the ordinary command, every time. */
it('never takes a lock over without being asked', async () => {
	ipc.unlock.mockResolvedValue(undefined);
	const component = open();

	submit('correct horse battery staple');
	await vi.waitFor(() => expect(ipc.unlock).toHaveBeenCalledTimes(1));
	expect(ipc.unlockTakingOver).not.toHaveBeenCalled();

	return unmount(component);
});

/** Coffer never makes a vault that wants a key file, so a database from another
 * client whose owner chose one there had no way in at all. */
it('asks for a key file and shows the one that was chosen', async () => {
	const chosen = { path: '/Users/someone/personal.key', name: 'personal' };
	ipc.chooseKeyFile.mockResolvedValue(chosen);
	const onKeyFile = vi.fn();

	const component = mount(Unlock, {
		target: host,
		props: {
			database,
			onChoose: vi.fn(),
			onKeyFile,
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();

	const offer = [...host.querySelectorAll('button')].find(
		(each) => each.textContent?.trim() === 'This vault also needs a key file'
	);
	expect(offer).toBeDefined();
	offer?.click();

	await vi.waitFor(() => expect(onKeyFile).toHaveBeenCalledWith(chosen));
	return unmount(component);
});

it('takes a key file back off when the reader picked the wrong one', async () => {
	const onKeyFile = vi.fn();
	const component = mount(Unlock, {
		target: host,
		props: {
			database,
			keyFile: { path: '/Users/someone/personal.key', name: 'personal' },
			onChoose: vi.fn(),
			onKeyFile,
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();

	expect(reads()).toContain('personal');
	host.querySelector<HTMLButtonElement>('[aria-label="Do not use a key file"]')?.click();

	await vi.waitFor(() => expect(ipc.forgetKeyFile).toHaveBeenCalledTimes(1));
	await vi.waitFor(() => expect(onKeyFile).toHaveBeenCalledWith(null));

	return unmount(component);
});

/**
 * A snapshot is a copy of the vault that is already chosen, so it opens with the
 * same credentials and Rust keeps the key file for it. The screen has to say the
 * same thing: one that cleared the key file while Rust was still using it would
 * be a screen nobody could reason about.
 */
it('keeps the key file when the reader opens a snapshot of the same vault', async () => {
	const keyFile = { path: '/Users/someone/personal.key', name: 'personal' };
	ipc.unlock.mockRejectedValue({ code: 'damaged', message: 'the database body is damaged' });
	ipc.snapshots.mockResolvedValue([
		{ name: 'personal.kdbx.1.bak', index: 1, taken: '2026-08-27T18:40:00Z' }
	]);
	ipc.chooseSnapshot.mockResolvedValue({
		path: '/Users/someone/personal.kdbx.1.bak',
		name: 'personal.kdbx.1'
	});
	const onKeyFile = vi.fn();

	const component = mount(Unlock, {
		target: host,
		props: {
			database,
			keyFile,
			onChoose: vi.fn(),
			onKeyFile,
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();

	submit('correct horse battery staple');
	await vi.waitFor(() => expect(reads()).toContain('personal.kdbx.1.bak'));
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('Open personal.kdbx.1.bak'))
		?.click();
	await vi.waitFor(() => expect(ipc.chooseSnapshot).toHaveBeenCalledTimes(1));

	expect(onKeyFile, 'the screen threw away a key file Rust is still using').not.toHaveBeenCalled();

	return unmount(component);
});

/** Any other file is a different vault, and its key file is not this one's. */
it('forgets the key file when another database is chosen', async () => {
	const onKeyFile = vi.fn();
	ipc.chooseDatabase.mockResolvedValue({ path: '/Users/someone/other.kdbx', name: 'other' });

	const component = mount(Unlock, {
		target: host,
		props: {
			database,
			keyFile: { path: '/Users/someone/personal.key', name: 'personal' },
			onChoose: vi.fn(),
			onKeyFile,
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Open another database')
		?.click();

	await vi.waitFor(() => expect(onKeyFile).toHaveBeenCalledWith(null));

	return unmount(component);
});

/**
 * The one screen alive after a lock, and the only place a copy the lock had to
 * write can be reported at all: the window that knew was destroyed with the
 * vault.
 */
it('offers the file a lock left behind, and never names it itself', async () => {
	ipc.chooseRescue.mockResolvedValue({
		path: '/Users/someone/personal.kdbx.unsaved.kdbx',
		name: 'personal.kdbx.unsaved'
	});
	const onChoose = vi.fn();
	mount(Unlock, {
		target: host,
		props: {
			database,
			rescue: { name: 'personal.kdbx.unsaved.kdbx', written: '2026-09-01T11:00:00Z' },
			onChoose,
			onKeyFile: vi.fn(),
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();

	expect(host.textContent).toContain('Work that never reached the vault');
	expect(host.textContent).toContain('personal.kdbx.unsaved.kdbx');

	button('Open the copy to look').click();
	await vi.waitFor(() => expect(onChoose).toHaveBeenCalled());

	// The window sends nothing: the path is Rust's, built from the database the
	// reader chose, so no message from here can name a file.
	expect(ipc.chooseRescue).toHaveBeenCalledWith();
});

/** The copy holds the only version of that work, so Coffer never removes it on
 * its own, and one press of the reader's does not either: it asks first, in
 * words, with the way out first and focused. Only the answer removes it, and
 * the offer goes with it. */
it('takes the file away only when the reader says so, twice', async () => {
	ipc.discardRescue.mockResolvedValue(undefined);
	const component = mount(Unlock, {
		target: host,
		props: {
			database,
			rescue: { name: 'personal.kdbx.unsaved.kdbx', written: null },
			file: { there: true, written: null },
			onChoose: vi.fn(),
			onKeyFile: vi.fn(),
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();

	// A copy whose time the filesystem did not keep ends its sentence where
	// the name does.
	expect(reads()).toContain('put it in personal.kdbx.unsaved.kdbx. It opens');

	button('Remove it…').click();
	flushSync();
	expect(ipc.discardRescue).not.toHaveBeenCalled();
	expect(reads()).toContain('Remove the only copy of those changes?');
	expect(document.activeElement?.textContent?.trim()).toBe('Keep it');

	button('Keep it').click();
	flushSync();
	expect(ipc.discardRescue).not.toHaveBeenCalled();
	expect(reads()).not.toContain('Remove the only copy');
	expect(reads()).toContain('Work that never reached the vault');

	button('Remove it…').click();
	flushSync();
	host
		.querySelector('[data-confirm] button')
		?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
	flushSync();
	expect(ipc.discardRescue).not.toHaveBeenCalled();

	button('Remove it…').click();
	flushSync();
	button('Remove').click();
	await vi.waitFor(() => expect(host.textContent).not.toContain('Work that never reached'));
	expect(ipc.discardRescue).toHaveBeenCalledTimes(1);
	expect(ipc.discardRescue).toHaveBeenCalledWith();

	unmount(component);
});

/** A screen with a lock's copy beside a vault whose file has gone. */
function standingIn(onChoose = vi.fn()) {
	const component = mount(Unlock, {
		target: host,
		props: {
			database,
			rescue: { name: 'personal.kdbx.unsaved.kdbx', written: '2026-09-01T14:05:00Z' },
			file: { there: false, written: null },
			onChoose,
			onKeyFile: vi.fn(),
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();
	return component;
}

/**
 * The vault's file went, and the copy is all there is. Putting it back is the
 * one thing to do, so it is the accent and nothing asks for a password that
 * could only open nothing; the move sends nothing, because the paths are
 * Rust's.
 */
it('puts the copy back in place of a vault whose file has gone', async () => {
	const back = { path: database.path, name: database.name };
	ipc.putBackRescue.mockResolvedValue(back);
	const onChoose = vi.fn();
	const component = standingIn(onChoose);

	expect(reads()).toContain('Your vault file is not there any more');
	expect(host.querySelector('form')?.hidden).toBe(true);

	// The copy is the whole vault now, and the question says so.
	button('Remove it…').click();
	flushSync();
	expect(reads()).toContain('Remove the only copy of your vault?');
	expect(reads()).not.toContain('of those changes');
	expect(document.activeElement?.textContent?.trim()).toBe('Keep it');
	button('Keep it').click();
	flushSync();
	expect(button('Put this copy back as my vault').className).toContain('bg-accent');
	expect(buttons()).not.toContain('Open the copy to look');

	button('Put this copy back as my vault').click();
	await vi.waitFor(() => expect(onChoose).toHaveBeenCalledWith(back));
	expect(ipc.putBackRescue).toHaveBeenCalledWith();
	expect(ipc.putBackRescue).toHaveBeenCalledTimes(1);

	unmount(component);
});

/**
 * Rust refuses the move when a file came back to the vault's name after the
 * screen was drawn, or the copy went. Either way the screen says so in words
 * about the move - not "there is already a file with that name", which is a
 * sentence about making a vault - and reads the disk again, which is what the
 * repeated choice of the same database asks the page to do.
 */
it('says why the copy was not put back and asks what is true now', async () => {
	for (const [code, said] of [
		['taken', 'A file is back where your vault was, so the copy was left where it is.'],
		['gone', 'The copy is not there any more.'],
		['heldByAnother', 'someone has this vault open on a-mac']
	]) {
		ipc.putBackRescue.mockRejectedValue({
			code,
			message: code === 'heldByAnother' ? 'someone has this vault open on a-mac' : 'no'
		});
		const onChoose = vi.fn();
		const component = standingIn(onChoose);

		button('Put this copy back as my vault').click();
		await vi.waitFor(() => expect(reads()).toContain(said));
		expect(onChoose).toHaveBeenCalledWith(database);
		// A lock somebody holds on the copy is not an offer to take one over:
		// that offer belongs to opening a vault, and nothing here opens one.
		expect(reads()).not.toContain('Open it anyway');

		unmount(component);
	}
});

/**
 * A disk that keeps no second name for a file cannot take the copy back
 * without a password: there is no way there to put a whole file at a name and
 * be refused if something arrived first. The screen says so, and opening the
 * copy - from inside which it becomes the vault - is the card's call instead.
 */
it('sends the reader into the copy when this disk cannot take it back unopened', async () => {
	ipc.putBackRescue.mockRejectedValue({
		code: 'needsOpening',
		message: 'this disk cannot take the copy back without it being opened'
	});
	const copied = {
		path: '/Users/someone/personal.kdbx.unsaved.kdbx',
		name: 'personal.kdbx.unsaved'
	};
	ipc.chooseRescue.mockResolvedValue(copied);
	const onChoose = vi.fn();
	const component = standingIn(onChoose);

	button('Put this copy back as my vault').click();
	await vi.waitFor(() =>
		expect(reads()).toContain(
			'This disk cannot take the copy back without it being opened. Open it, and make it your vault from inside.'
		)
	);
	expect(buttons()).not.toContain('Put this copy back as my vault');
	expect(reads()).not.toContain('with nothing to type');
	expect(button('Open the copy to look').className).toContain('bg-accent');

	button('Open the copy to look').click();
	await vi.waitFor(() => expect(onChoose).toHaveBeenCalledWith(copied));

	unmount(component);
});

/**
 * A vault that is there is never gone over from the unlock screen. The copy
 * is opened to be looked at, and the password form stays, because the vault
 * can still simply be unlocked.
 */
it('offers to look at the copy when the vault is still there', () => {
	const component = mount(Unlock, {
		target: host,
		props: {
			database,
			rescue: { name: 'personal.kdbx.unsaved.kdbx', written: '2026-09-01T14:05:00Z' },
			file: { there: true, written: '2026-09-01T14:07:00Z' },
			onChoose: vi.fn(),
			onKeyFile: vi.fn(),
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();

	expect(buttons()).toContain('Open the copy to look');
	expect(buttons()).not.toContain('Put this copy back as my vault');
	expect(host.querySelector('form')?.hidden).toBe(false);
	expect(reads()).toContain('put it in personal.kdbx.unsaved.kdbx on 1 Sep at 14:05. It opens');

	unmount(component);
});

/**
 * The unlock screen of the copy itself says what it is, and the way back to
 * the vault is a press on it rather than a file panel. Nothing is sent: Rust
 * reads the vault off the copy's name. The key file stays, since the copy
 * opens with it.
 */
it('takes the reader from a copy back to the vault it was taken from', async () => {
	const vault = { path: database.path, name: database.name };
	ipc.leaveRescue.mockResolvedValue(vault);
	const onChoose = vi.fn();
	const onKeyFile = vi.fn();
	const component = mount(Unlock, {
		target: host,
		props: {
			database: {
				path: '/Users/someone/personal.kdbx.unsaved.kdbx',
				name: 'personal.kdbx.unsaved'
			},
			copy: {
				vault: 'personal.kdbx',
				saved: '2026-09-01T14:05:00Z',
				keptAs: 'personal.kdbx.1.bak',
				vaultFile: { there: true, written: null }
			},
			file: { there: true, written: null },
			onChoose,
			onKeyFile,
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();

	expect(reads()).toContain('This is the copy a lock saved on 1 Sep at 14:05 beside personal.kdbx');

	button('Back to my vault').click();
	await vi.waitFor(() => expect(onChoose).toHaveBeenCalledWith(vault));
	expect(ipc.leaveRescue).toHaveBeenCalledWith();
	expect(onKeyFile).not.toHaveBeenCalled();

	unmount(component);
});

/** The one case with no file to point at. It still has to be said: the reader
 * is about to unlock and find work missing - and so does what survived, which
 * is the vault's file as it was, or nothing at all when that has gone too. */
it('says so when a lock could not write the changes anywhere, and what survived', () => {
	for (const [file, said] of [
		[
			{ there: true, written: '2026-09-01T14:02:00Z' },
			'Your vault file is as it was on 1 Sep at 14:02.'
		],
		[{ there: true, written: null }, 'Your vault file is as it was before those changes.'],
		[{ there: false, written: null }, 'Your vault file is not where it was, either.']
	] as const) {
		const component = mount(Unlock, {
			target: host,
			props: {
				database,
				lost: true,
				file,
				onChoose: vi.fn(),
				onKeyFile: vi.fn(),
				onCreate: vi.fn(),
				onUnlocked: vi.fn()
			}
		});
		flushSync();

		expect(reads()).toContain('could not write them anywhere');
		expect(reads()).toContain(said);

		unmount(component);
	}
});

/** A lock of a copy opened to look that could write nothing leaves the
 * reader on the copy's screen, and what survived is the copy: its time, not
 * the vault's, and never under the vault's name. */
it('says what survived of the copy when the lost lock was of a copy', () => {
	for (const [file, said] of [
		[{ there: true, written: '2026-09-01T14:02:00Z' }, 'This copy is as it was on 1 Sep at 14:02.'],
		[{ there: true, written: null }, 'This copy is as it was before those changes.'],
		[{ there: false, written: null }, 'This copy is not where it was, either.']
	] as const) {
		const component = mount(Unlock, {
			target: host,
			props: {
				database: {
					path: '/Users/someone/personal.kdbx.unsaved.kdbx',
					name: 'personal.kdbx.unsaved'
				},
				lost: true,
				file,
				copy: {
					vault: 'personal.kdbx',
					saved: '2026-09-01T14:02:00Z',
					keptAs: 'personal.kdbx.1.bak',
					vaultFile: { there: true, written: '2026-09-01T09:00:00Z' }
				},
				onChoose: vi.fn(),
				onKeyFile: vi.fn(),
				onCreate: vi.fn(),
				onUnlocked: vi.fn()
			}
		});
		flushSync();

		expect(reads()).toContain(said);
		expect(reads()).not.toContain('Your vault file');

		unmount(component);
	}
});

/**
 * A lock that found something being typed and saved it says so - in one
 * sentence that names nothing. After a lock no title is left in memory, and
 * this screen is not going to be what keeps one: the flag is all it is given.
 * A lock that found nothing, or whose save went beside the vault instead,
 * says nothing here; the rescue line speaks for that one.
 */
it('says what was being typed was saved, and names nothing', () => {
	for (const [typed, reason] of [
		[true, 'sleeping'],
		[true, null],
		[false, 'idle']
	] as const) {
		const component = mount(Unlock, {
			target: host,
			props: {
				database,
				typed,
				reason,
				onChoose: vi.fn(),
				onKeyFile: vi.fn(),
				onCreate: vi.fn(),
				onUnlocked: vi.fn()
			}
		});
		flushSync();

		const said = host.textContent ?? '';
		expect(said.includes('What you were typing was saved before locking.'), `${typed}`).toBe(typed);
		expect(said).not.toContain('could not write them anywhere');
		unmount(component);
	}
});

/** The first-run screen, with nothing remembered and whatever Rust found. */
function firstRun(found: { name: string; folder: string } | null, onChoose = vi.fn()) {
	const component = mount(Unlock, {
		target: host,
		props: {
			database: null,
			found,
			onChoose,
			onKeyFile: vi.fn(),
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();
	return component;
}

/**
 * Coffer 0.1.0 forgot every vault it made, so its owner comes back to the
 * screen for somebody with nothing. The vault is still in Coffer's folder, and
 * the screen says so above the offer to make a second one.
 */
it("offers a vault found in Coffer's folder above the offer to make one", async () => {
	const onChoose = vi.fn();
	const opened = { path: '/Users/someone/Coffer/vault.kdbx', name: 'vault' };
	ipc.chooseFound.mockResolvedValue(opened);
	const component = firstRun({ name: 'vault.kdbx', folder: 'Coffer' }, onChoose);

	expect(reads()).toContain('We found your vault: vault.kdbx in Coffer (your home folder).');
	expect(reads().indexOf('We found your vault')).toBeLessThan(reads().indexOf('Make a vault'));

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Open it')
		?.click();

	await vi.waitFor(() => expect(onChoose).toHaveBeenCalledWith(opened));
	// Rust looks again: nothing this window sends names a file.
	expect(ipc.chooseFound).toHaveBeenCalledWith();

	unmount(component);
});

it('says nothing about a vault when none was found', () => {
	const component = firstRun(null);

	expect(reads()).toContain('Your passwords will live here');
	expect(reads()).not.toContain('We found your vault');
	expect(
		[...host.querySelectorAll('button')].some((each) => each.textContent?.trim() === 'Open it')
	).toBe(false);

	unmount(component);
});

/** A vault that went away between the screen being drawn and the press is
 * reported, and the screen is still the one that can make a vault. */
it('says so when the vault it found has gone', async () => {
	const onChoose = vi.fn();
	ipc.chooseFound.mockRejectedValue({ code: 'gone', message: 'the database file is gone' });
	const component = firstRun({ name: 'vault.kdbx', folder: 'Coffer' }, onChoose);

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Open it')
		?.click();

	await vi.waitFor(() => expect(reads()).toContain('the database file is gone'));
	expect(onChoose).not.toHaveBeenCalled();
	expect(reads()).toContain('Make a vault');

	unmount(component);
});

/** A launch that remembers its vault goes straight to the password; the offer
 * belongs to the first-run screen and nowhere else. */
it('offers nothing found when there is a vault to unlock', () => {
	const component = mount(Unlock, {
		target: host,
		props: {
			database,
			found: { name: 'vault.kdbx', folder: 'Coffer' },
			onChoose: vi.fn(),
			onKeyFile: vi.fn(),
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();

	expect(reads()).not.toContain('We found your vault');
	expect(host.querySelector('input[type="password"]')).not.toBeNull();

	unmount(component);
});

/** The name is whatever somebody called a file, and it is drawn as text. */
it('draws a found name that holds markup as the characters it is', () => {
	const hostile = '<img src=x onerror=alert(1)>.kdbx';
	const component = firstRun({ name: hostile, folder: 'Coffer' });

	expect(host.querySelector('img')).toBeNull();
	expect(reads()).toContain(`We found your vault: ${hostile} in Coffer`);

	unmount(component);
});
