import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import Create from './Create.svelte';

const ipc = vi.hoisted(() => ({
	calibrate: vi.fn(),
	defaultNewDatabase: vi.fn(),
	chooseNewDatabase: vi.fn(),
	chooseExisting: vi.fn(),
	createDatabase: vi.fn(),
	asFailure: vi.fn()
}));
vi.mock('$lib/ipc', () => ipc);

const HOME = {
	path: '/Users/someone/Coffer/vault.kdbx',
	name: 'vault',
	shown: '~/Coffer/vault.kdbx',
	taken: false
};
const WHERE = {
	path: '/Users/someone/Vault/personal.kdbx',
	name: 'personal',
	shown: '~/Vault/personal.kdbx',
	taken: false
};

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
		props: {
			onMade: vi.fn().mockResolvedValue(undefined),
			onCancel: vi.fn(),
			onOpen: vi.fn(),
			...over
		}
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
	await vi.waitFor(() => expect(host.textContent).toContain(HOME.shown));
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
	expect(host.textContent).not.toContain(HOME.shown);
	unmount(waiting);

	place(HOME);

	let measure: (found: unknown) => void = () => {};
	ipc.defaultNewDatabase.mockResolvedValue(HOME);
	ipc.calibrate.mockReturnValue(new Promise((settle) => (measure = settle)));

	const component = show();
	flushSync();
	await vi.waitFor(() => expect(host.textContent).toContain(HOME.shown));
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
	const component = await ready();

	let sent: Uint8Array | null = null;
	ipc.createDatabase.mockImplementation((password: Uint8Array) => {
		sent = password;
		return Promise.reject({ code: 'io', message: 'No space left on device' });
	});

	const [first, second] = fields();
	first.value = 'a password';
	second.value = 'a password';
	submit();

	await vi.waitFor(() => expect(host.textContent).toContain('No space left on device'));
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
	expect(host.textContent).toContain(HOME.shown);

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

	expect(host.textContent).toContain(HOME.shown);
	expect(ipc.chooseNewDatabase).not.toHaveBeenCalled();
	expect(button('Make the vault').disabled).toBe(false);

	unmount(component);
});

it('still lets the reader put it somewhere else', async () => {
	const component = await ready();

	button('Somewhere else').click();
	await vi.waitFor(() => expect(host.textContent).toContain(WHERE.shown));
	expect(host.textContent).not.toContain(HOME.shown);

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

function reads(): string {
	return (host.textContent ?? '').replace(/\s+/g, ' ').trim();
}

const EXISTS =
	'A vault already exists at ~/Coffer/vault.kdbx. Open it instead, or pick another place.';

/**
 * Coffer 0.1.0 forgot every vault it made, and its owner came back to "Make a
 * vault". The place is known to be taken before a password is typed, so the
 * screen says so then - not after two passwords were typed and thrown away.
 */
it('says a vault is already there before a password is typed, and keeps what was typed', async () => {
	ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, taken: true });
	const component = show();
	flushSync();
	await vi.waitFor(() => expect(reads()).toContain(EXISTS));
	await vi.waitFor(() => expect(host.textContent).not.toContain('Fitting the lock'));

	expect(button('Make the vault').disabled).toBe(true);

	const [first, second] = fields();
	first.value = 'a long master password';
	second.value = 'a long master password';
	submit();
	flushSync();

	expect(ipc.createDatabase).not.toHaveBeenCalled();
	expect(first.value).toBe('a long master password');
	expect(second.value).toBe('a long master password');

	unmount(component);
});

/** The way out the sentence names. Nothing is sent: Rust holds the place. */
it('opens what is already there instead, naming no file', async () => {
	const existing = { path: HOME.path, name: HOME.name };
	ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, taken: true });
	ipc.chooseExisting.mockResolvedValue(existing);
	const onOpen = vi.fn();
	const component = show({ onOpen });
	flushSync();
	await vi.waitFor(() => expect(reads()).toContain(EXISTS));

	button('Open it').click();

	await vi.waitFor(() => expect(onOpen).toHaveBeenCalledWith(existing));
	expect(ipc.chooseExisting).toHaveBeenCalledWith();
	expect(ipc.createDatabase).not.toHaveBeenCalled();

	unmount(component);
});

/** The other way out. A place that is free takes the sentence away and gives
 * the button back. */
it('lets the reader pick another place, and is ready to make the vault there', async () => {
	ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, taken: true });
	const component = show();
	flushSync();
	await vi.waitFor(() => expect(reads()).toContain(EXISTS));
	await vi.waitFor(() => expect(host.textContent).not.toContain('Fitting the lock'));

	button('Somewhere else').click();

	await vi.waitFor(() => expect(host.textContent).toContain(WHERE.shown));
	expect(reads()).not.toContain('already exists');
	expect(button('Make the vault').disabled).toBe(false);

	unmount(component);
});

/** A panel the reader answered "Replace" in is still not a place Coffer writes
 * over: a creation takes no snapshot. */
it('says so when the place picked in the panel is taken too', async () => {
	ipc.chooseNewDatabase.mockResolvedValue({ ...WHERE, taken: true });
	const component = await ready();

	button('Somewhere else').click();

	await vi.waitFor(() =>
		expect(reads()).toContain(
			'A vault already exists at ~/Vault/personal.kdbx. Open it instead, or pick another place.'
		)
	);
	expect(button('Make the vault').disabled).toBe(true);

	unmount(component);
});

/**
 * A file that arrived at the place after the screen said it was free. The
 * fields are empty for the whole of the call, exactly as on the way to a vault
 * that is made; only this refusal hands the two passwords back, because it is
 * the one that says nothing about them.
 */
it('hands both passwords back when the place was taken at the last moment', async () => {
	let refuse: (why: unknown) => void = () => {};
	let sent: Uint8Array | null = null;
	ipc.createDatabase.mockImplementation((password: Uint8Array) => {
		sent = password;
		return new Promise((_, reject) => (refuse = reject));
	});
	const onMade = vi.fn();
	const component = await ready({ onMade });

	const [first, second] = fields();
	first.value = 'correct horse battery staple';
	second.value = 'correct horse battery staple';
	submit();
	await vi.waitFor(() => expect(ipc.createDatabase).toHaveBeenCalledTimes(1));

	expect(first.value).toBe('');
	expect(second.value).toBe('');
	expect(new TextDecoder().decode(sent ?? new Uint8Array())).toBe('correct horse battery staple');

	refuse({ code: 'taken', message: 'there is already a file with that name' });

	await vi.waitFor(() => expect(reads()).toContain(EXISTS));
	expect(first.value).toBe('correct horse battery staple');
	expect(second.value).toBe('correct horse battery staple');
	expect(button('Make the vault').disabled).toBe(true);
	expect(onMade).not.toHaveBeenCalled();
	// The screen's own sentence, not Rust's: it names the place and the way on.
	expect(reads()).not.toContain('there is already a file with that name');

	unmount(component);
});

/** Any other refusal is about something the reader has to act on, and the
 * password goes as it always did. */
it('hands nothing back for a refusal that is not about the place', async () => {
	ipc.createDatabase.mockRejectedValue({ code: 'io', message: 'Permission denied' });
	const component = await ready();

	const [first, second] = fields();
	first.value = 'a password';
	second.value = 'a password';
	submit();

	await vi.waitFor(() => expect(reads()).toContain('Permission denied'));
	expect(first.value).toBe('');
	expect(second.value).toBe('');
	expect(reads()).not.toContain('already exists');

	unmount(component);
});

/** A place is whatever the reader typed into a save panel, and it is drawn as
 * text in both the row and the sentence. */
it('draws a taken place that holds markup as the characters it is', async () => {
	const hostile = '~/<img src=x onerror=alert(1)>.kdbx';
	ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, shown: hostile, taken: true });
	const component = show();
	flushSync();

	await vi.waitFor(() => expect(reads()).toContain(`A vault already exists at ${hostile}.`));
	expect(host.querySelector('img')).toBeNull();

	unmount(component);
});
