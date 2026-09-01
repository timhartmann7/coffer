import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import Create from './Create.svelte';

const ipc = vi.hoisted(() => ({
	calibrate: vi.fn(),
	defaultNewDatabase: vi.fn(),
	chooseNewDatabase: vi.fn(),
	createDatabase: vi.fn(),
	asFailure: vi.fn()
}));
vi.mock('$lib/ipc', () => ipc);

const HOME = { path: '/Users/someone/Coffer/vault.kdbx', name: 'vault' };
const WHERE = { path: '/Users/someone/Vault/personal.kdbx', name: 'personal' };

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.calibrate.mockResolvedValue({ iterations: 122, seconds: 1.004 });
	ipc.defaultNewDatabase.mockResolvedValue(HOME);
	ipc.chooseNewDatabase.mockResolvedValue(WHERE);
	ipc.createDatabase.mockResolvedValue(undefined);
	// The same reading the real one does: a command rejects with the value Rust
	// serialised, and anything else is not one.
	ipc.asFailure.mockImplementation((thrown: unknown) =>
		thrown && typeof (thrown as { message?: unknown }).message === 'string'
			? thrown
			: { code: 'other', message: 'Coffer could not finish that.' }
	);
});

afterEach(() => host.remove());

function show(over: Record<string, unknown> = {}) {
	return mount(Create, {
		target: host,
		props: { onMade: vi.fn().mockResolvedValue(undefined), onCancel: vi.fn(), ...over }
	});
}

function fields(): HTMLInputElement[] {
	return [...host.querySelectorAll<HTMLInputElement>('input[type="password"]')];
}

function submit() {
	host.querySelector('form')?.dispatchEvent(new Event('submit', { cancelable: true }));
}

function button(label: string): HTMLButtonElement {
	const found = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === label
	);
	if (!found) throw new Error(`no button labelled ${label}`);
	return found;
}

async function ready(over: Record<string, unknown> = {}) {
	const component = show(over);
	flushSync();
	await vi.waitFor(() => expect(host.textContent).not.toContain('Fitting the lock'));
	await vi.waitFor(() => expect(host.textContent).toContain(HOME.path));
	return component;
}

/**
 * The measurement runs as soon as the screen opens, and the screen says so only
 * while it is happening. A reader came to make a vault; the number of Argon2id
 * passes is not something they can act on, and the wait is.
 */
it('says what it is doing while it measures, and stops when it is done', async () => {
	const component = show();
	flushSync();

	expect(host.textContent).toContain('Fitting the lock to this Mac');

	await vi.waitFor(() => expect(host.textContent).not.toContain('Fitting the lock'));
	expect(ipc.calibrate).toHaveBeenCalledTimes(1);

	// Nothing of the measurement is left on the screen, in any of the words it
	// used to be spelled with.
	expect(host.textContent).not.toMatch(/argon|passes|header/i);

	unmount(component);
});

/** Nothing can be made before there is somewhere to put it and a measurement to
 * put in it. Both arrive on their own now, so each is held back in turn. */
it('will not make a vault before it knows where or how hard', async () => {
	let place: (where: unknown) => void = () => {};
	ipc.defaultNewDatabase.mockReturnValue(new Promise((settle) => (place = settle)));

	const waiting = show();
	flushSync();
	expect(button('Make the vault').disabled).toBe(true);
	expect(host.textContent).not.toContain(HOME.path);
	unmount(waiting);

	place(HOME);

	let measure: (found: unknown) => void = () => {};
	ipc.defaultNewDatabase.mockResolvedValue(HOME);
	ipc.calibrate.mockReturnValue(new Promise((settle) => (measure = settle)));

	const component = show();
	flushSync();
	await vi.waitFor(() => expect(host.textContent).toContain(HOME.path));
	expect(button('Make the vault').disabled).toBe(true);

	measure({ iterations: 122, seconds: 1.004 });
	await vi.waitFor(() => expect(button('Make the vault').disabled).toBe(false));

	unmount(component);
});

it('sends the password when both fields hold the same one', async () => {
	const onMade = vi.fn().mockResolvedValue(undefined);
	const component = await ready({ onMade });

	const [first, second] = fields();
	first.value = 'correct horse battery staple';
	second.value = 'correct horse battery staple';
	submit();

	await vi.waitFor(() => expect(onMade).toHaveBeenCalledTimes(1));
	expect(ipc.createDatabase).toHaveBeenCalledTimes(1);
	// The fields are emptied before the call that takes a second even begins.
	expect(first.value).toBe('');
	expect(second.value).toBe('');

	unmount(component);
});

it('sends nothing when the two fields hold different passwords', async () => {
	const component = await ready();

	const [first, second] = fields();
	first.value = 'correct horse battery staple';
	second.value = 'correct horse battery stapl';
	submit();
	flushSync();

	expect(ipc.createDatabase).not.toHaveBeenCalled();
	expect(host.textContent).toContain('Those two are not the same.');

	unmount(component);
});

/**
 * Compared as bytes rather than as strings. Two passwords that differ only in
 * how they are composed look the same to a lossy conversion, and a vault opened
 * by neither of them is not something anybody finds out until later.
 */
it('compares the two passwords byte for byte', async () => {
	const component = await ready();

	const [first, second] = fields();
	// The same letter, written two ways: one code point, and a letter with a
	// combining mark. Equal to a reader, different to a KeePass file.
	first.value = '\u00e9clair';
	second.value = 'e\u0301clair';
	submit();
	flushSync();

	expect(ipc.createDatabase).not.toHaveBeenCalled();
	expect(host.textContent).toContain('Those two are not the same.');

	unmount(component);
});

it('will not make a vault with no password at all', async () => {
	const component = await ready();

	const [first, second] = fields();
	first.value = '';
	second.value = '';
	submit();
	flushSync();

	expect(ipc.createDatabase).not.toHaveBeenCalled();
	expect(host.textContent).toContain('A vault needs a master password.');

	unmount(component);
});

/** The buffer is wiped whether the command worked or not, and the fields are
 * emptied before the call that takes a second even begins. */
it('empties both fields and wipes the bytes, whatever happens', async () => {
	ipc.createDatabase.mockRejectedValue({ code: 'refused', message: 'there is already a file' });
	const component = await ready();

	let sent: Uint8Array | null = null;
	ipc.createDatabase.mockImplementation((password: Uint8Array) => {
		sent = password;
		return Promise.reject({ code: 'refused', message: 'there is already a file' });
	});

	const [first, second] = fields();
	first.value = 'a password';
	second.value = 'a password';
	submit();

	await vi.waitFor(() => expect(host.textContent).toContain('there is already a file'));
	expect(first.value).toBe('');
	expect(second.value).toBe('');
	// The real `createDatabase` wipes what it was given; this one is a mock, so
	// what is checked is that the screen hands over a buffer and keeps nothing.
	expect(sent).not.toBeNull();

	unmount(component);
});

it('says what Rust refused, and leaves the screen where it was', async () => {
	ipc.createDatabase.mockRejectedValue({
		code: 'refused',
		message: 'that name belongs to a file Coffer keeps beside a vault'
	});
	const onMade = vi.fn();
	const component = await ready({ onMade });

	const [first, second] = fields();
	first.value = 'a password';
	second.value = 'a password';
	submit();

	await vi.waitFor(() => expect(host.textContent).toContain('beside a vault'));
	expect(onMade).not.toHaveBeenCalled();
	expect(host.textContent).toContain(HOME.path);

	unmount(component);
});

/** A forgotten password is the end of the data, so the warning is at full size
 * and in plain words rather than in small type behind a checkbox. */
it('says the password cannot be recovered, in the largest words on the screen', async () => {
	const component = show();
	flushSync();

	expect(host.textContent).toContain('The password cannot be recovered.');
	expect(host.textContent).toContain('nobody can get the data back');

	unmount(component);
});

/** The bar is one of two literal classes. A computed width compiles to nothing,
 * and the window's style rules would drop an inline one silently. */
it('draws the measuring bar out of a fixed set of widths', async () => {
	// Both answers are held back in turn, because the bar is the one thing that
	// says which of them the screen is still waiting for.
	let place: (where: unknown) => void = () => {};
	ipc.defaultNewDatabase.mockReturnValue(new Promise((settle) => (place = settle)));
	ipc.calibrate.mockReturnValue(new Promise(() => {}));

	const component = show();
	flushSync();
	expect(host.querySelector('.w-1\\/3.bg-accent')).not.toBeNull();

	place(HOME);
	await vi.waitFor(() => expect(host.querySelector('.w-2\\/3.bg-accent')).not.toBeNull());

	unmount(component);
});

/**
 * The slice is accepted on five actions from a clean machine to a saved entry,
 * and two of them used to go on answering a save panel. The place is settled
 * before anybody arrives, so making a vault is a password and nothing else.
 */
it('arrives with a place already chosen, and no panel opened', async () => {
	const component = await ready();

	expect(host.textContent).toContain(HOME.path);
	expect(ipc.chooseNewDatabase).not.toHaveBeenCalled();
	expect(button('Make the vault').disabled).toBe(false);

	unmount(component);
});

it('still lets the reader put it somewhere else', async () => {
	const component = await ready();

	button('Somewhere else').click();
	await vi.waitFor(() => expect(host.textContent).toContain(WHERE.path));
	expect(host.textContent).not.toContain(HOME.path);

	unmount(component);
});

/** The cursor starts in the field, because the password is the only thing this
 * screen asks anybody to type. */
it('puts the cursor where the typing goes', async () => {
	const component = await ready();

	expect(document.activeElement).toBe(fields()[0]);

	unmount(component);
});

/**
 * Return in the first field submitted the form with the second still empty, and
 * `make` empties both before it compares them - so one stray press destroyed the
 * password and blamed the reader for mistyping it.
 */
it('does not throw the password away when return is pressed too early', async () => {
	const component = await ready();

	const [first, second] = fields();
	first.value = 'a long master password';
	first.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
	flushSync();

	expect(first.value).toBe('a long master password');
	expect(document.activeElement).toBe(second);
	expect(ipc.createDatabase).not.toHaveBeenCalled();
	expect(host.textContent).not.toContain('not the same');

	unmount(component);
});
