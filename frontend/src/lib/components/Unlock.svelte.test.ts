import { flushSync, mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import Unlock from './Unlock.svelte';

const ipc = vi.hoisted(() => ({
	unlock: vi.fn(),
	unlockTakingOver: vi.fn(),
	snapshots: vi.fn(),
	chooseDatabase: vi.fn(),
	chooseSnapshot: vi.fn(),
	chooseRescue: vi.fn(),
	discardRescue: vi.fn(),
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

	const found = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === 'Open it'
	);
	found?.click();
	await vi.waitFor(() => expect(onChoose).toHaveBeenCalled());

	// The window sends nothing: the path is Rust's, built from the database the
	// reader chose, so no message from here can name a file.
	expect(ipc.chooseRescue).toHaveBeenCalledWith();
});

/** The copy holds the only version of that work, so Coffer never removes it on
 * its own. The reader's own press does, and the offer goes with it. */
it('takes the file away only when the reader says so', async () => {
	ipc.discardRescue.mockResolvedValue(undefined);
	const component = mount(Unlock, {
		target: host,
		props: {
			database,
			rescue: { name: 'personal.kdbx.unsaved.kdbx', written: null },
			onChoose: vi.fn(),
			onKeyFile: vi.fn(),
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();
	expect(ipc.discardRescue).not.toHaveBeenCalled();

	const found = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === 'Remove it'
	);
	found?.click();
	await vi.waitFor(() => expect(host.textContent).not.toContain('Work that never reached'));

	unmount(component);
});

/** The one case with no file to point at. It still has to be said: the reader
 * is about to unlock and find work missing. */
it('says so when a lock could not write the changes anywhere', () => {
	const component = mount(Unlock, {
		target: host,
		props: {
			database,
			lost: true,
			onChoose: vi.fn(),
			onKeyFile: vi.fn(),
			onCreate: vi.fn(),
			onUnlocked: vi.fn()
		}
	});
	flushSync();

	expect(host.textContent).toContain('could not write them anywhere');

	unmount(component);
});
