import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { applying, run } from '$lib/menu.svelte';
import type { Target } from '$lib/model';
import type { Stubbed } from '$lib/stubbed';
import Create from './Create.svelte';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('$lib/ipc'), async (real) =>
	Object.assign(ipc, (await import('$lib/stubbed')).stubbed(await real()))
);

const HOME: Target = { shown: '~/Coffer/vault.kdbx', standing: 'free' };
const WHERE: Target = { shown: '~/Vault/personal.kdbx', standing: 'free' };

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.calibrate.mockResolvedValue({ iterations: 122, seconds: 1.004 });
	ipc.defaultNewDatabase.mockResolvedValue(HOME);
	ipc.chooseNewDatabase.mockResolvedValue(WHERE);
	ipc.createDatabase.mockResolvedValue(undefined);
});

afterEach(() => host.remove());

function show(over: Record<string, unknown> = {}) {
	return mount(Create, {
		target: host,
		props: {
			onMade: vi.fn().mockResolvedValue(undefined),
			onCancel: vi.fn(),
			onOpen: vi.fn(),
			onChoose: vi.fn().mockResolvedValue(undefined),
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
	ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, standing: 'vault' });
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
	const existing = { path: '/Users/someone/Coffer/vault.kdbx', name: 'vault' };
	ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, standing: 'vault' });
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
	ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, standing: 'vault' });
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
	ipc.chooseNewDatabase.mockResolvedValue({ ...WHERE, standing: 'vault' });
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

	ipc.target.mockResolvedValue({ ...HOME, standing: 'vault' });
	refuse({ code: 'taken', message: 'there is already a file with that name' });

	await vi.waitFor(() => expect(reads()).toContain(EXISTS));
	// Read again rather than assumed: what arrived is what the sentence names.
	expect(ipc.target).toHaveBeenCalledTimes(1);
	expect(first.value).toBe('correct horse battery staple');
	expect(second.value).toBe('correct horse battery staple');
	expect(sent ?? new Uint8Array([1])).toSatisfy((bytes: Uint8Array) =>
		bytes.every((byte) => byte === 0)
	);
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
	ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, shown: hostile, standing: 'vault' });
	const component = show();
	flushSync();

	await vi.waitFor(() => expect(reads()).toContain(`A vault already exists at ${hostile}.`));
	expect(host.querySelector('img')).toBeNull();

	unmount(component);
});

/**
 * Every copy of the password the screen made is wiped as soon as the vault is
 * made, before the load that follows: that load is several calls long, and the
 * only reason for the copy - a place taken at the last moment - cannot happen
 * any more.
 */
it('wipes its copy of the password as soon as the vault is made', async () => {
	const copies: Uint8Array[] = [];
	const slice = Uint8Array.prototype.slice;
	const spy = vi.spyOn(Uint8Array.prototype, 'slice').mockImplementation(function (
		this: Uint8Array,
		...range: [number?, number?]
	) {
		const copy = slice.apply(this, range);
		copies.push(copy);
		return copy;
	});
	let wiped: boolean | null = null;
	const onMade = vi.fn(async () => {
		wiped = copies.length > 0 && copies.every((copy) => copy.every((byte) => byte === 0));
	});
	const component = await ready({ onMade });

	const [first, second] = fields();
	first.value = 'correct horse battery staple';
	second.value = 'correct horse battery staple';
	submit();

	await vi.waitFor(() => expect(onMade).toHaveBeenCalledTimes(1));
	spy.mockRestore();
	expect(wiped).toBe(true);

	unmount(component);
});

/**
 * What is at the place is said for what it is. Only a vault, or the copy a lock
 * left of one, can be opened instead; an empty file a killed creation left, a
 * folder or a link to nothing is named and pointed past, never offered as a
 * vault that would open as nothing.
 */
it('names what is at the place, and offers to open only a vault', async () => {
	const said: [Target['standing'], string, boolean][] = [
		[
			'copy',
			'The vault that was at ~/Coffer/vault.kdbx is gone, and the copy a lock saved of it is beside that name. Open it to put the copy back, or pick another place.',
			true
		],
		[
			'empty',
			'An empty file is at ~/Coffer/vault.kdbx, left by a vault that was never finished. Nothing is made over a file: move it away in the Finder, or pick Somewhere else.',
			false
		],
		[
			'other',
			'Something that is not a vault is at ~/Coffer/vault.kdbx: a folder, or a link that leads nowhere. Pick Somewhere else.',
			false
		]
	];

	for (const [standing, sentence, openable] of said) {
		ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, standing });
		const component = show();
		flushSync();
		await vi.waitFor(() => expect(reads()).toContain(sentence));
		await vi.waitFor(() => expect(host.textContent).not.toContain('Fitting the lock'));

		expect(reads()).not.toContain('already exists');
		expect(button('Make the vault').disabled, standing).toBe(true);
		expect(
			[...host.querySelectorAll('button')].some((each) => each.textContent?.trim() === 'Open it'),
			standing
		).toBe(openable);

		unmount(component);
	}
});

/**
 * The vault the sentence named stopped being one before the press. The place
 * is read again, so the screen stops offering to open a folder, and the reader
 * is told why the press did nothing.
 */
it('reads the place again when what was there is not a vault any more', async () => {
	ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, standing: 'vault' });
	ipc.chooseExisting.mockRejectedValue({ code: 'gone', message: 'the database file is gone' });
	ipc.target.mockResolvedValue({ ...HOME, standing: 'other' });
	const onOpen = vi.fn();
	const component = show({ onOpen });
	flushSync();
	await vi.waitFor(() => expect(reads()).toContain(EXISTS));
	await vi.waitFor(() => expect(host.textContent).not.toContain('Fitting the lock'));

	button('Open it').click();

	await vi.waitFor(() => expect(reads()).toContain('Something that is not a vault is at'));
	expect(reads()).toContain('the database file is gone');
	expect(reads()).not.toContain('already exists');
	expect(
		[...host.querySelectorAll('button')].some((each) => each.textContent?.trim() === 'Open it')
	).toBe(false);
	expect(button('Make the vault').disabled).toBe(true);
	expect(onOpen).not.toHaveBeenCalled();

	unmount(component);
});

/** A vault that went away altogether leaves the place free, and the screen is
 * ready to make one there. */
it('is ready to make the vault when what was there has gone', async () => {
	ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, standing: 'vault' });
	ipc.chooseExisting.mockRejectedValue({ code: 'gone', message: 'the database file is gone' });
	ipc.target.mockResolvedValue(HOME);
	const component = show();
	flushSync();
	await vi.waitFor(() => expect(reads()).toContain(EXISTS));
	await vi.waitFor(() => expect(host.textContent).not.toContain('Fitting the lock'));

	button('Open it').click();

	await vi.waitFor(() => expect(reads()).not.toContain('already exists'));
	expect(button('Make the vault').disabled).toBe(false);

	unmount(component);
});

/** A refusal that says nothing about the place leaves the place as it was
 * said, and the press can be made again. */
it('says why opening was refused, and leaves the offer standing', async () => {
	ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, standing: 'vault' });
	ipc.chooseExisting.mockRejectedValue({
		code: 'refused',
		message: 'lock the vault before opening another'
	});
	const component = show();
	flushSync();
	await vi.waitFor(() => expect(reads()).toContain(EXISTS));

	button('Open it').click();

	await vi.waitFor(() => expect(reads()).toContain('lock the vault before opening another'));
	expect(ipc.target).not.toHaveBeenCalled();
	expect(reads()).toContain(EXISTS);
	expect(button('Open it').disabled).toBe(false);

	unmount(component);
});

/** Two presses while the first is on its way open the vault once. */
it('opens what is there once, however often it is pressed', async () => {
	let answer: (database: unknown) => void = () => {};
	ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, standing: 'vault' });
	ipc.chooseExisting.mockReturnValue(new Promise((settle) => (answer = settle)));
	const onOpen = vi.fn();
	const component = show({ onOpen });
	flushSync();
	await vi.waitFor(() => expect(reads()).toContain(EXISTS));

	button('Open it').click();
	flushSync();
	button('Open it').click();
	button('Open it').dispatchEvent(new MouseEvent('click'));

	const existing = { path: '/Users/someone/Coffer/vault.kdbx', name: 'vault' };
	answer(existing);

	await vi.waitFor(() => expect(onOpen).toHaveBeenCalledWith(existing));
	expect(ipc.chooseExisting).toHaveBeenCalledTimes(1);
	expect(onOpen).toHaveBeenCalledTimes(1);

	unmount(component);
});

/** A copy a lock left arrived beside the name during the creation: the place
 * is read again and says so, and the passwords come back. */
it('names a copy that turned up beside the place at the last moment', async () => {
	ipc.createDatabase.mockRejectedValue({
		code: 'taken',
		message: 'the copy a lock left of a vault by that name is beside it'
	});
	ipc.target.mockResolvedValue({ ...HOME, standing: 'copy' });
	const component = await ready();

	const [first, second] = fields();
	first.value = 'a password';
	second.value = 'a password';
	submit();

	await vi.waitFor(() => expect(reads()).toContain('the copy a lock saved of it'));
	expect(first.value).toBe('a password');
	expect(button('Make the vault').disabled).toBe(true);

	unmount(component);
});

/**
 * A place the screen points past is one the reader can clear in the Finder,
 * and nothing they press here would read it again: Make stays disabled and
 * there is no Open it. So the place is read again each time the window gets
 * the focus back, and a place that came free is ready to make the vault in.
 */
it('reads a place that is not free again when the reader comes back to the window', async () => {
	for (const standing of ['empty', 'other', 'vault', 'copy'] as const) {
		ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, standing });
		ipc.target.mockReset();
		ipc.target.mockResolvedValue(HOME);
		const component = show();
		flushSync();
		await vi.waitFor(() => expect(host.textContent).not.toContain('Fitting the lock'));
		expect(button('Make the vault').disabled, standing).toBe(true);

		window.dispatchEvent(new FocusEvent('focus'));

		await vi.waitFor(() => expect(button('Make the vault').disabled, standing).toBe(false));
		expect(reads()).not.toContain('Nothing is made over a file');
		expect(reads()).not.toContain('Pick Somewhere else');
		expect(ipc.target).toHaveBeenCalledTimes(1);

		unmount(component);
	}
});

/** A free place has nothing to be cleared, and coming back reads nothing. */
it('reads nothing when the reader comes back to a free place', async () => {
	ipc.target.mockReset();
	const component = await ready();

	window.dispatchEvent(new FocusEvent('focus'));
	flushSync();

	expect(ipc.target).not.toHaveBeenCalled();
	unmount(component);
});

/** The panel the reader picked in hands the focus back as it closes, so the
 * place from before the choice is still being read when the choice lands. The
 * choice is the later word on where the vault goes. */
it('keeps a place picked while the one before it was being read again', async () => {
	ipc.defaultNewDatabase.mockResolvedValue({ ...HOME, standing: 'empty' });
	ipc.target.mockReset();
	const before = Promise.withResolvers<Target>();
	ipc.target.mockReturnValue(before.promise);
	const component = show();
	flushSync();
	await vi.waitFor(() => expect(host.textContent).not.toContain('Fitting the lock'));

	window.dispatchEvent(new FocusEvent('focus'));
	button('Somewhere else').click();
	await vi.waitFor(() => expect(host.textContent).toContain(WHERE.shown));
	before.resolve({ ...HOME, standing: 'empty' });
	await Promise.resolve();
	flushSync();

	expect(host.textContent).toContain(WHERE.shown);
	expect(button('Make the vault').disabled).toBe(false);

	unmount(component);
});

/**
 * Open Vault… in the menu bar applies wherever no vault is open, and this
 * screen is one of those places: a reader who came to make a vault and
 * remembered they have one opens it from here, through the window's own
 * picker. Not while the vault is being made, though - the creation in flight
 * would land after the session had been pointed elsewhere, be refused, and
 * leave a vault on the disk that nothing opened.
 */
it('opens another vault from the menu, and not while one is being made', async () => {
	const onChoose = vi.fn().mockResolvedValue(undefined);
	const component = await ready({ onChoose });
	try {
		expect(applying()).toContain('openVault');
		run('openVault');
		expect(onChoose).toHaveBeenCalledTimes(1);

		let made: () => void = () => {};
		ipc.createDatabase.mockReturnValue(new Promise<void>((resolve) => (made = resolve)));
		const [first, second] = fields();
		first.value = 'a password';
		second.value = 'a password';
		submit();
		flushSync();
		expect(applying(), 'offered while the vault was being made').not.toContain('openVault');
		run('openVault');
		await Promise.resolve();
		expect(onChoose).toHaveBeenCalledTimes(1);
		made();

		// A panel that could not open says so here, where the reader is.
		onChoose.mockRejectedValueOnce({ code: 'io', message: 'the panel did not open' });
		await vi.waitFor(() => expect(applying()).toContain('openVault'));
		run('openVault');
		await vi.waitFor(() => expect(host.textContent).toContain('the panel did not open'));
	} finally {
		unmount(component);
	}
});

/** The two fields are the master password's one field, as the change in the
 * settings draws it: the wrong-password border on both when they differ, and
 * an input method's Return in the first left to the input method rather than
 * taken for a move to the second. The screen draws neither the eye nor the
 * check the mockup puts on these two fields, and both stay masked. */
it('takes both passwords in the master password field, masked and without the eye', async () => {
	const component = await ready();
	const [first, second] = fields();
	expect(host.querySelector('button[aria-label="Show the password"]')).toBeNull();
	expect([first.type, second.type]).toEqual(['password', 'password']);

	first.focus();
	const composing = new KeyboardEvent('keydown', {
		key: 'Enter',
		keyCode: 229,
		bubbles: true,
		cancelable: true
	});
	first.dispatchEvent(composing);
	expect(composing.defaultPrevented).toBe(true);
	expect(document.activeElement, 'a conversion moved on to the second field').toBe(first);

	first.value = 'one';
	second.value = 'two';
	submit();
	flushSync();
	for (const each of [first, second]) {
		expect(each.getAttribute('aria-invalid')).toBe('true');
		expect(each.parentElement?.className).toContain('border-danger/60');
	}

	unmount(component);
});
