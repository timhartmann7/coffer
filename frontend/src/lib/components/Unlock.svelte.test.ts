import { flushSync, mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import Unlock from './Unlock.svelte';

const ipc = vi.hoisted(() => ({
	unlock: vi.fn(),
	snapshots: vi.fn(),
	chooseDatabase: vi.fn(),
	chooseSnapshot: vi.fn(),
	asFailure: (thrown: unknown) => thrown as { code: string; message: string }
}));
vi.mock('$lib/ipc', () => ipc);

const database = { path: '/Users/someone/personal.kdbx', name: 'personal' };

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.snapshots.mockResolvedValue([]);
});

afterEach(() => {
	host.remove();
});

function open(onUnlocked = vi.fn().mockResolvedValue(undefined)) {
	const component = mount(Unlock, {
		target: host,
		props: { database, onChoose: vi.fn(), onUnlocked }
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
	expect(reads()).toContain('Choose another file');

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
			props: { database, reason, onChoose: vi.fn(), onUnlocked: vi.fn() }
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
		props: { database, reason: 'somethingLater', onChoose: vi.fn(), onUnlocked: vi.fn() }
	});
	flushSync();

	expect(host.textContent).toContain('The vault was locked.');
	unmount(component);
});
