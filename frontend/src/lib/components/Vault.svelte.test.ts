import { createRawSnippet, flushSync, mount as draw, tick, unmount as release } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { drawing, entry, field, generated, group, kinds, row, version } from '$lib/fixtures';
import { focused } from '$lib/focus.svelte';
import { applying, run } from '$lib/menu.svelte';
import type { Entry, EntryRow, Group, Version } from '$lib/model';
import { reactive } from '$lib/props.svelte';
import type { Stubbed } from '$lib/stubbed';
import Vault from './Vault.svelte';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('$lib/ipc'), async (real) =>
	Object.assign(ipc, (await import('$lib/stubbed')).stubbed(await real()))
);

const database = { path: '/Users/someone/personal.kdbx', name: 'personal' };

/** The revision of the vault a list of versions here was read at, unless a
 * test says otherwise. */
const REVISION = 1;

/** What the commands that list versions answer, paired with the entry they were
 * asked about the way `ipc.ts` pairs them. */
function listing(versions: Version[] = [], revision = REVISION) {
	return (entry: string) => Promise.resolve({ entry, revision, versions });
}

const kept = row({ title: 'node-3', username: 'deploy', tags: ['prod'] });
const other = row({ title: 'Postgres', username: 'svc_app' });
const deleted = row({ title: 'thrown away' });

const root = group({
	name: 'Root',
	entries: [kept],
	sections: [
		// A folder with a folder in it, so the tree draws the chevron that expands
		// without selecting. That press is the one the dismiss guard is about.
		group({ name: 'Work', entries: [other], sections: [group({ name: 'Clients' })] }),
		group({ name: 'Recycle Bin', isRecycleBin: true, entries: [deleted] })
	]
});

/**
 * Every screen a test drew and has not let go of. A screen answers the menu bar
 * through a module every test in this file shares, so one that a failed test
 * left drawn would go on answering in the tests after it: one regression read
 * as a dozen, and an item offered by a screen nobody is looking at.
 */
const drawn = new Set<object>();

const mount: typeof draw = (component, options) => {
	const made = draw(component, options);
	drawn.add(made);
	return made;
};

const unmount: typeof release = (component, options) => {
	drawn.delete(component);
	return release(component, options);
};

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.copy.mockResolvedValue(60);
	ipc.versions.mockImplementation(listing());
	ipc.save.mockResolvedValue(undefined);
	ipc.tree.mockResolvedValue(root);
	ipc.draft.mockResolvedValue(undefined);
});

afterEach(() => {
	for (const left of drawn) void release(left);
	drawn.clear();
	host.remove();
	// A selection one test made is still standing in the next one otherwise,
	// and a stand-in for the selection has to go before it can be cleared.
	vi.restoreAllMocks();
	document.getSelection()?.removeAllRanges();
});

/** The settings screen, which this component only ever renders and never
 * builds: what is in it belongs to the window above. */
const sheet = createRawSnippet(() => ({ render: () => '<p>The settings</p>' }));

function open(
	over: {
		readOnly?: boolean;
		settings?: typeof sheet;
		onSettings?: () => void;
		onTree?: (tree: typeof root) => void;
	} = {}
) {
	return mount(Vault, {
		target: host,
		props: {
			database,
			root,
			kinds: kinds(),
			readOnly: false,
			onSettings: vi.fn(),
			onTree: vi.fn(),
			...over
		}
	});
}

/** What the pane reads as, with the whitespace of the template collapsed the
 * way a browser collapses it on screen. */
function reads(): string {
	return (host.textContent ?? '').replace(/\s+/g, ' ').trim();
}

function search(): HTMLInputElement {
	const found = host.querySelector('input');
	if (!found) throw new Error('there is no search field');
	return found;
}

function type(query: string) {
	const field = search();
	field.value = query;
	field.dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
}

/** The folder list that is open: "+ Entry"'s, or the one above an entry's
 * title. */
function picker(): HTMLInputElement {
	const filter = host.querySelector<HTMLInputElement>('input[role="combobox"]');
	if (!filter) throw new Error('no folder list is open');
	return filter;
}

/** A key pressed in the open folder list. */
function inPicker(key: string, over: KeyboardEventInit = {}) {
	picker().dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, ...over }));
	flushSync();
}

/** An edit the way a reader makes one: typed, and then the focus leaving. A
 * field only clicked through writes nothing, so a value set without the typing
 * is not an edit. */
function write(field: HTMLInputElement, value: string) {
	field.value = value;
	field.dispatchEvent(new Event('input', { bubbles: true }));
	field.dispatchEvent(new Event('blur'));
}

it('shows every entry the vault holds except the ones that were deleted', () => {
	const component = open();
	flushSync();

	expect(host.textContent).toContain('node-3');
	expect(host.textContent).toContain('Postgres');
	expect(host.textContent).not.toContain('thrown away');
	expect(reads()).toContain('2 entries here · 2 in the vault');

	return unmount(component);
});

it('narrows the list as the reader types, and says so when nothing is left', () => {
	const component = open();
	flushSync();

	type('postgres');
	expect(host.textContent).toContain('Postgres');
	expect(host.textContent).not.toContain('node-3');

	type('stripe');
	expect(host.textContent).toContain('Nothing matches');
	expect(host.textContent).toContain('Notes and custom fields are not searched');

	const clear = [...host.querySelectorAll('button')].find(
		(each) => each.textContent?.trim() === 'Clear the search'
	);
	clear?.click();
	flushSync();
	expect(host.textContent).toContain('node-3');

	return unmount(component);
});

it('counts a folder on its own when one is chosen', () => {
	const component = open();
	flushSync();

	const work = [...host.querySelectorAll('button')].find((each) =>
		each.textContent?.includes('Work')
	);
	work?.click();
	flushSync();

	expect(reads()).toContain('1 entry here · 2 in the vault');
	expect(host.textContent).not.toContain('node-3');

	return unmount(component);
});

it('opens an entry and shows it beside the list', async () => {
	ipc.entry.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: [field({ name: 'Title', kind: 'title', value: 'node-3', empty: false })]
		})
	);

	const component = open();
	flushSync();

	const opener = [...host.querySelectorAll('button')].find((each) =>
		each.textContent?.includes('node-3')
	);
	opener?.click();

	await vi.waitFor(() => expect(ipc.entry).toHaveBeenCalledWith(kept.id));
	flushSync();

	expect((host.querySelector('h1 input') as HTMLInputElement | null)?.value).toBe('node-3');

	return unmount(component);
});

/** The two shortcuts the mockup puts on the rows. They act on the entry that is
 * open, and they never touch the value themselves. Cmd+C is the window's own
 * key; Copy Login is the menu bar's, and its key reaches the page only to be
 * left for the menu. */
it('copies the open entry through Rust on the keyboard and from the menu', async () => {
	ipc.entry.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: [
				field({ name: 'UserName', kind: 'username', value: 'deploy', empty: false }),
				field({ name: 'Password', kind: 'password', value: null, empty: false })
			]
		})
	);

	const component = open();
	flushSync();
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('node-3'))
		?.click();
	await vi.waitFor(() => expect(ipc.entry).toHaveBeenCalled());
	flushSync();

	window.dispatchEvent(new KeyboardEvent('keydown', { key: 'c', metaKey: true }));
	await vi.waitFor(() => expect(ipc.copy).toHaveBeenCalledWith(kept.id, 'Password', null));
	await tick();
	expect(reads()).toContain('Password copied. The clipboard clears in 1 minute.');

	const key = new KeyboardEvent('keydown', { key: 'b', metaKey: true, cancelable: true });
	window.dispatchEvent(key);
	await tick();
	expect(key.defaultPrevented, 'the page took the key the menu bar answers').toBe(false);
	expect(ipc.copy).toHaveBeenCalledTimes(1);

	// The two shortcuts are a key apart, and the notice of one must not read as
	// the notice of the other: a login pasted into a password box was the result.
	run('copyLogin');
	await vi.waitFor(() => expect(ipc.copy).toHaveBeenCalledWith(kept.id, 'UserName', null));

	await tick();
	expect(reads()).toContain('Login copied. The clipboard clears in 1 minute.');
	expect(reads()).not.toContain('Password copied.');

	return unmount(component);
});

/** The pane marks the login, the address, the notes and a revealed value as
 * selectable on purpose. A reader who selected one of them and pressed the copy
 * shortcut asked for that, not for the entry's password. */
it('leaves a copy the reader selected alone', async () => {
	ipc.entry.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
		})
	);

	const component = open();
	flushSync();
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('node-3'))
		?.click();
	await vi.waitFor(() => expect(ipc.entry).toHaveBeenCalled());
	flushSync();

	vi.spyOn(document, 'getSelection').mockReturnValue({ isCollapsed: false } as Selection);
	window.dispatchEvent(new KeyboardEvent('keydown', { key: 'c', metaKey: true }));
	await tick();
	expect(ipc.copy).not.toHaveBeenCalled();

	vi.spyOn(document, 'getSelection').mockReturnValue({ isCollapsed: true } as Selection);
	window.dispatchEvent(new KeyboardEvent('keydown', { key: 'c', metaKey: true }));
	await vi.waitFor(() => expect(ipc.copy).toHaveBeenCalledWith(kept.id, 'Password', null));

	return unmount(component);
});

/** Opens node-3 with the fields given, and waits until the pane is drawn. */
async function showing(fields: ReturnType<typeof field>[]) {
	ipc.entry.mockResolvedValue(entry({ id: kept.id, group: root.id, fields }));
	const component = open();
	flushSync();
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('node-3'))
		?.click();
	await vi.waitFor(() => expect(ipc.entry).toHaveBeenCalled());
	await tick();
	flushSync();
	return component;
}

/**
 * Cmd+C copies what the reader is looking at. With the focus on the row of a
 * field of their own - which is where Show puts it - that is the field, and
 * the window does not go on to copy the password as well.
 */
it('copies the value on the row the focus is on, not the password', async () => {
	ipc.reveal.mockResolvedValue('sk-live-9f3a2b');
	const component = await showing([
		field({ name: 'Password', kind: 'password', value: null, empty: false }),
		field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false })
	]);

	host.querySelector<HTMLButtonElement>('[aria-label="Show API token"]')?.click();
	await vi.waitFor(() =>
		expect(document.activeElement?.getAttribute('aria-label')).toBe('Hide API token')
	);

	const pressed = new KeyboardEvent('keydown', {
		key: 'c',
		metaKey: true,
		bubbles: true,
		cancelable: true
	});
	document.activeElement?.dispatchEvent(pressed);
	await vi.waitFor(() => expect(ipc.copy).toHaveBeenCalledWith(kept.id, 'API token', null));
	expect(ipc.copy).toHaveBeenCalledTimes(1);
	await tick();
	expect(reads()).toContain('“\u2068API token\u2069” copied. The clipboard clears in 1 minute.');

	return unmount(component);
});

/**
 * The reader selected part of a revealed password and pressed Cmd+C. The
 * window steps aside for the selection, the system fires its copy at the node
 * holding it, and the node sends that part to Rust instead of to the plain
 * pasteboard - with the same notice as any other copy.
 */
it('hands the part of a revealed value the reader selected to Rust, and says so', async () => {
	ipc.reveal.mockResolvedValue('correct horse battery staple');
	const component = await showing([
		field({ name: 'Password', kind: 'password', value: null, empty: false })
	]);

	[...host.querySelectorAll('button')].find((each) => each.textContent?.trim() === 'Show')?.click();
	const node = host.querySelector('[data-value]') as HTMLElement;
	await vi.waitFor(() => expect(node.textContent).toBe('correct horse battery staple'));

	const range = document.createRange();
	range.setStart(node.firstChild as Text, 8);
	range.setEnd(node.firstChild as Text, 13);
	document.getSelection()?.removeAllRanges();
	document.getSelection()?.addRange(range);

	const pressed = new KeyboardEvent('keydown', {
		key: 'c',
		metaKey: true,
		bubbles: true,
		cancelable: true
	});
	window.dispatchEvent(pressed);
	expect(pressed.defaultPrevented, 'the window copied the password over the selection').toBe(false);

	const copied = new ClipboardEvent('copy', { bubbles: true, cancelable: true });
	node.dispatchEvent(copied);
	expect(copied.defaultPrevented, 'the system copied a secret').toBe(true);
	await vi.waitFor(() =>
		expect(ipc.copy).toHaveBeenCalledWith(kept.id, 'Password', { from: 8, to: 13 })
	);
	expect(ipc.copy).toHaveBeenCalledTimes(1);
	await tick();
	expect(reads()).toContain('Part of the password copied. The clipboard clears in 1 minute.');

	return unmount(component);
});

/**
 * A selection from the password down into a revealed token reaches two
 * values, and Rust copies one field at a time, so nothing is copied. A Cmd+C
 * that did nothing without a word left the reader pasting whatever the
 * pasteboard held before, so the window says why.
 */
it('says why a copy reaching two revealed values copied nothing', async () => {
	ipc.reveal.mockImplementation((_entry: string, name: string) =>
		Promise.resolve(name === 'Password' ? 'correct horse' : 'sk-live-9f3a2b')
	);
	const component = await showing([
		field({ name: 'Password', kind: 'password', value: null, empty: false }),
		field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false })
	]);

	[...host.querySelectorAll('button')].find((each) => each.textContent?.trim() === 'Show')?.click();
	host.querySelector<HTMLButtonElement>('[aria-label="Show API token"]')?.click();
	const [password, token] = [...host.querySelectorAll<HTMLElement>('[data-value]')];
	await vi.waitFor(() => expect(password?.textContent).toBe('correct horse'));
	await vi.waitFor(() => expect(token?.textContent).toBe('sk-live-9f3a2b'));

	const range = document.createRange();
	range.setStart(password?.firstChild as Text, 8);
	range.setEnd(token?.firstChild as Text, 4);
	document.getSelection()?.removeAllRanges();
	document.getSelection()?.addRange(range);

	const copied = new ClipboardEvent('copy', { bubbles: true, cancelable: true });
	password?.dispatchEvent(copied);
	expect(copied.defaultPrevented, 'the system copied two secrets').toBe(true);
	await tick();
	expect(reads()).toContain('Select one value at a time to copy it.');
	expect(ipc.copy).not.toHaveBeenCalled();

	return unmount(component);
});

/** A previous version's value goes through a command of its own, which finds
 * the value by the version's position, and gets the same notice. */
it('copies a value out of a previous version through Rust', async () => {
	ipc.versions.mockImplementation(
		listing([version({ index: 0, modified: '2025-06-02T12:00:00Z' })])
	);
	ipc.version.mockResolvedValue(
		entry({ fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })] })
	);
	ipc.copyVersion.mockResolvedValue(60);
	const component = await showing([
		field({ name: 'Password', kind: 'password', value: null, empty: false })
	]);

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('Versions'))
		?.click();
	flushSync();
	[...host.querySelectorAll('button')].find((each) => each.textContent?.trim() === 'View')?.click();
	await vi.waitFor(() =>
		expect(host.querySelector('[aria-label="Copy Password as it was"]')).not.toBeNull()
	);
	host.querySelector<HTMLButtonElement>('[aria-label="Copy Password as it was"]')?.click();

	await vi.waitFor(() =>
		expect(ipc.copyVersion).toHaveBeenCalledWith(
			kept.id,
			{ index: 0, revision: REVISION },
			'Password',
			null
		)
	);
	expect(ipc.copy).not.toHaveBeenCalled();
	await tick();
	expect(reads()).toContain('Password copied. The clipboard clears in 1 minute.');

	return unmount(component);
});

/** A copy that failed is not a copy. It must not be drawn as one, and the
 * message it puts up must not outlive a copy that follows it. */
it('says a copy failed without dressing it as one that worked', async () => {
	vi.useFakeTimers();
	try {
		ipc.entry.mockResolvedValue(
			entry({
				id: kept.id,
				group: root.id,
				fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
			})
		);
		ipc.copy.mockRejectedValueOnce({ code: 'noVault', message: 'no database is open' });

		const component = open();
		flushSync();
		[...host.querySelectorAll('button')]
			.find((each) => each.textContent?.includes('node-3'))
			?.click();
		await vi.advanceTimersByTimeAsync(0);
		flushSync();

		window.dispatchEvent(new KeyboardEvent('keydown', { key: 'c', metaKey: true }));
		await vi.advanceTimersByTimeAsync(0);
		flushSync();

		const toast = () => host.querySelector('[data-notice]');
		expect(toast()?.textContent).toContain('no database is open');
		expect(toast()?.querySelector('use[href="#i-warn"]')).not.toBeNull();
		expect(toast()?.querySelector('use[href="#i-copy"]')).toBeNull();

		// A copy that works while the failure is still up gets its own message
		// and its own five seconds: the failure's timer must not take it away.
		ipc.copy.mockResolvedValue(60);
		ipc.versions.mockImplementation(listing());
		ipc.save.mockResolvedValue(undefined);
		window.dispatchEvent(new KeyboardEvent('keydown', { key: 'c', metaKey: true }));
		await vi.advanceTimersByTimeAsync(0);
		flushSync();
		expect(toast()?.textContent).toContain('Password copied. The clipboard clears in 1 minute.');
		expect(toast()?.querySelector('use[href="#i-copy"]')).not.toBeNull();

		await vi.advanceTimersByTimeAsync(4_000);
		flushSync();
		expect(toast()?.textContent).toContain('Password copied. The clipboard clears in 1 minute.');

		// And then it goes, rather than sitting in the corner for the whole
		// minute it is talking about.
		await vi.advanceTimersByTimeAsync(2_000);
		flushSync();
		expect(toast()).toBeNull();

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** The sentence in the corner, as the reader reads it. */
const notice = () =>
	(host.querySelector('[data-notice]')?.textContent ?? '').replace(/\s+/g, ' ').trim();

/** The button whose name is exactly this, matched as text rather than through
 * a selector a name with quotes or an override in it would break. */
function named(label: string, within: ParentNode = host): HTMLButtonElement {
	const found = [...within.querySelectorAll('button')].find(
		(each) => each.getAttribute('aria-label') === label
	);
	if (!found) throw new Error(`no button named ${JSON.stringify(label)}`);
	return found;
}

/**
 * A copy says which field it took. "Copied." read the same after a login and
 * after a password a key apart, and the reader pasted their email address into
 * a password box. A row of the list, a field of the open entry, a field as a
 * previous version held it and the part of a value the reader selected each
 * reach Rust a different way, and every one of them has to arrive at the
 * notice with the name of the field it copied, not the name of the last one.
 */
it('names the field a copy took, from a row, the open entry, a version or a selection', async () => {
	const mail = row({ title: 'Mail', username: 'me@example.com', hasPassword: true });
	const tree = group({ name: 'Root', entries: [mail] });
	ipc.tree.mockResolvedValue(tree);
	ipc.entry.mockResolvedValue(
		entry({
			id: mail.id,
			group: tree.id,
			fields: [
				field({ name: 'Title', kind: 'title', value: 'Mail', empty: false }),
				field({ name: 'UserName', kind: 'username', value: 'me@example.com', empty: false }),
				field({ name: 'Password', kind: 'password', value: null, empty: false, protected: true }),
				field({ name: 'URL', kind: 'url', value: 'https://mail.example.com', empty: false }),
				field({ name: 'Notes', kind: 'notes', value: 'codes in the safe', empty: false }),
				field({ name: 'PIN', kind: 'custom', value: null, empty: false, protected: true })
			]
		})
	);
	ipc.reveal.mockResolvedValue('4921');
	ipc.versions.mockImplementation(listing([version({ index: 0 })]));
	ipc.version.mockResolvedValue(
		entry({
			fields: [
				field({ name: 'Password', kind: 'password', value: null, empty: false, protected: true }),
				field({ name: 'PIN', kind: 'custom', value: null, empty: false, protected: true })
			]
		})
	);
	ipc.copyVersion.mockResolvedValue(60);
	const clears = ' The clipboard clears in 1 minute.';

	const component = mount(Vault, {
		target: host,
		props: {
			database,
			root: tree,
			kinds: kinds(),
			readOnly: false,
			onSettings: vi.fn(),
			onTree: vi.fn()
		}
	});
	try {
		flushSync();

		named('Copy password').click();
		await vi.waitFor(() => expect(notice()).toBe(`Password copied.${clears}`));
		expect(ipc.copy).toHaveBeenLastCalledWith(mail.id, 'Password', null);
		named('Copy login').click();
		await vi.waitFor(() => expect(notice()).toBe(`Login copied.${clears}`));
		expect(ipc.copy).toHaveBeenLastCalledWith(mail.id, 'UserName', null);

		[...host.querySelectorAll('button')]
			.find((each) => each.textContent?.includes('Mail'))
			?.click();
		await vi.waitFor(() =>
			expect(host.querySelector('[aria-label="Copy address"]')).not.toBeNull()
		);

		for (const [label, name, said] of [
			['Copy address', 'URL', 'Address copied.'],
			['Copy notes', 'Notes', 'Notes copied.'],
			['Copy PIN', 'PIN', '“\u2068PIN\u2069” copied.'],
			['Copy login', 'UserName', 'Login copied.']
		]) {
			named(label).click();
			await vi.waitFor(() => expect(notice(), label).toBe(`${said}${clears}`));
			expect(ipc.copy).toHaveBeenLastCalledWith(mail.id, name, null);
		}

		named('Show PIN').click();
		const pin = await vi.waitFor(() => {
			const found = [...host.querySelectorAll<HTMLElement>('[data-value]')].find(
				(each) => each.textContent === '4921'
			);
			if (!found) throw new Error('the PIN was not revealed');
			return found;
		});
		const range = document.createRange();
		range.setStart(pin.firstChild as Text, 1);
		range.setEnd(pin.firstChild as Text, 3);
		document.getSelection()?.removeAllRanges();
		document.getSelection()?.addRange(range);
		pin.dispatchEvent(new ClipboardEvent('copy', { bubbles: true, cancelable: true }));
		await vi.waitFor(() => expect(notice()).toBe(`Part of “\u2068PIN\u2069” copied.${clears}`));
		expect(ipc.copy).toHaveBeenLastCalledWith(mail.id, 'PIN', { from: 1, to: 3 });
		document.getSelection()?.removeAllRanges();

		[...host.querySelectorAll('button')]
			.find((each) => each.textContent?.includes('Versions'))
			?.click();
		flushSync();
		[...host.querySelectorAll('button')]
			.find((each) => each.textContent?.trim() === 'View')
			?.click();
		await vi.waitFor(() =>
			expect(host.querySelector('[aria-label="Copy PIN as it was"]')).not.toBeNull()
		);
		named('Copy PIN as it was').click();
		await vi.waitFor(() => expect(notice()).toBe(`“\u2068PIN\u2069” copied.${clears}`));
		expect(ipc.copyVersion).toHaveBeenLastCalledWith(
			mail.id,
			{ index: 0, revision: REVISION },
			'PIN',
			null
		);
		named('Copy Password as it was').click();
		await vi.waitFor(() => expect(notice()).toBe(`Password copied.${clears}`));
		expect(ipc.copyVersion).toHaveBeenLastCalledWith(
			mail.id,
			{ index: 0, revision: REVISION },
			'Password',
			null
		);
	} finally {
		await unmount(component);
	}
});

/**
 * A field of the reader's own is theirs to name, and a name that happens to be
 * spelled like a standard field's, or like a property every object has, is
 * still theirs. A notice that said "Password copied." for a field called
 * "password" told the reader the entry's password was on the pasteboard when
 * it was not; one that looked the name up among an object's own properties
 * would say something else entirely for "constructor". A name with a
 * right-to-left override in it is kept off the rest of the sentence.
 */
it('quotes a field of the reader’s own in the notice, whatever its name is spelled like', async () => {
	const names = ['password', 'url', 'Notes ', '__proto__', 'constructor', 'toString', 'a\u202Ebc'];
	const component = await showing([
		field({ name: 'Password', kind: 'password', value: null, empty: false, protected: true }),
		...names.map((name) =>
			field({ name, kind: 'custom', value: `the value of ${name}`, empty: false })
		)
	]);
	try {
		for (const name of names) {
			named(`Copy ${name}`).click();
			await vi.waitFor(() =>
				expect(notice(), JSON.stringify(name)).toBe(
					`“\u2068${name}\u2069” copied. The clipboard clears in 1 minute.`
				)
			);
			expect(ipc.copy).toHaveBeenLastCalledWith(kept.id, name, null);
		}
	} finally {
		await unmount(component);
	}
});

it('puts the reader in the search field on the item the field advertises', async () => {
	const component = open();
	flushSync();
	type('node');

	search().blur();
	expect(document.activeElement).not.toBe(search());
	run('find');
	await tick();

	expect(document.activeElement).toBe(search());
	expect(
		[search().selectionStart, search().selectionEnd],
		'what was typed is not selected'
	).toEqual([0, 4]);

	return unmount(component);
});

/**
 * The one thing a vault with two clients has to get right. The save is refused
 * before anything is written, and each of the three ways out keeps something:
 * the version on disk, both, or this one - with what was there going into the
 * snapshot chain on the way.
 */
it('asks which version to keep when the file changed underneath it', async () => {
	ipc.entry.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: [field({ name: 'Title', kind: 'title', value: 'node-3', empty: false })]
		})
	);
	ipc.save.mockRejectedValue({
		code: 'externalChange',
		message: 'the database changed on disk after Coffer opened it'
	});
	ipc.rival.mockResolvedValue({ modified: '2026-08-29T18:47:00Z', entries: 49 });
	ipc.setField.mockResolvedValue(entry({ id: kept.id, group: root.id }));

	const component = open();
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('node-3'))
		?.click();
	await vi.waitFor(() => expect(ipc.entry).toHaveBeenCalled());
	flushSync();

	// An edit, which is what triggers a save.
	const title = host.querySelector('h1 input') as HTMLInputElement;
	write(title, 'node-4');

	await vi.waitFor(() =>
		expect(host.textContent).toContain('The file changed while you were working')
	);
	flushSync();

	// Both sides are named, and the one that cannot be read says so rather than
	// showing a number nobody can stand behind.
	expect(host.textContent).toContain('49 entries');

	// The safe outcome is the accent one; the destructive one is the red one and
	// it is last.
	const named = [...host.querySelectorAll('button')]
		.map((each) => each.textContent?.trim())
		.filter((each) => each === 'Take the file on disk' || each === 'Keep mine');
	expect(named).toEqual(['Take the file on disk', 'Keep mine']);

	return unmount(component);
});

it('takes the version on disk when that is what the reader chose', async () => {
	ipc.save.mockRejectedValue({ code: 'externalChange', message: 'the database changed on disk' });
	ipc.rival.mockResolvedValue({ modified: null, entries: null });
	ipc.reload.mockResolvedValue(root);
	ipc.createEntry.mockResolvedValue({ tree: root, entry: kept.id });
	ipc.entry.mockResolvedValue(entry({ id: kept.id, group: root.id }));
	ipc.versions.mockImplementation(listing());

	const onTree = vi.fn();
	const component = open({ onTree });
	flushSync();

	// Making an entry is a change like any other, and it saves.
	// In "All entries" of a vault with a folder, "+ Entry" asks where, on the
	// top of the vault, which Return gives.
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Entry')
		?.click();
	flushSync();
	inPicker('Enter');
	expect(ipc.createEntry).toHaveBeenCalledWith(root.id, 'login');
	await vi.waitFor(() => expect(host.textContent).toContain('The file changed'));
	flushSync();

	// A file that will not open with this password says so rather than claiming
	// a number.
	expect(host.textContent).toContain('will not open with this password');

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Take the file on disk')
		?.click();
	await vi.waitFor(() => expect(ipc.reload).toHaveBeenCalledTimes(1));
	flushSync();
	// With a number, so that a draft of anything typed before it and arriving
	// after it is dropped rather than written over the file just read.
	expect(ipc.reload).toHaveBeenCalledWith(expect.any(Number));

	expect(host.textContent).not.toContain('The file changed');
	expect(onTree).toHaveBeenCalledWith(root);

	return unmount(component);
});

it('keeps this version and writes over the file when asked to', async () => {
	ipc.save.mockRejectedValue({ code: 'externalChange', message: 'the database changed on disk' });
	ipc.rival.mockResolvedValue({ modified: null, entries: 3 });
	ipc.saveOver.mockResolvedValue(undefined);
	ipc.createEntry.mockResolvedValue({ tree: root, entry: kept.id });
	ipc.entry.mockResolvedValue(entry({ id: kept.id, group: root.id }));

	const component = open();
	flushSync();

	// In "All entries" of a vault with a folder, "+ Entry" asks where, on the
	// top of the vault, which Return gives.
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Entry')
		?.click();
	flushSync();
	inPicker('Enter');
	expect(ipc.createEntry).toHaveBeenCalledWith(root.id, 'login');
	await vi.waitFor(() => expect(host.textContent).toContain('The file changed'));
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Keep mine')
		?.click();
	await vi.waitFor(() => expect(ipc.saveOver).toHaveBeenCalledTimes(1));
	flushSync();

	expect(host.textContent).not.toContain('The file changed');

	return unmount(component);
});

/**
 * A file that is not there any more: renamed in Finder, moved, deleted, or on a
 * disk that was unplugged. Every save from here on fails, so a screen that only
 * raised a notice left the reader editing into a window with a whole session's
 * work in memory and nothing in the application able to write it anywhere.
 */
it('asks what to do when the vault file is gone, rather than only reporting it', async () => {
	ipc.save.mockRejectedValue({ code: 'gone', message: 'the database file is gone' });
	ipc.rival.mockResolvedValue({ modified: null, entries: null });
	ipc.saveOver.mockResolvedValue(undefined);
	ipc.createEntry.mockResolvedValue({ tree: root, entry: kept.id });
	ipc.entry.mockResolvedValue(entry({ id: kept.id, group: root.id }));

	const component = open();
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Entry')
		?.click();
	flushSync();
	inPicker('Enter');
	expect(ipc.createEntry).toHaveBeenCalledWith(root.id, 'login');
	await vi.waitFor(() => expect(host.textContent).toContain('not there any more'));
	flushSync();

	// Nothing to reload from a file that is gone, so that way out is not offered.
	const named = [...host.querySelectorAll('button')].map((each) => each.textContent?.trim());
	expect(named).toContain('Put it back');
	expect(named).toContain('Keep it elsewhere');
	expect(named).not.toContain('Take the file on disk');

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Put it back')
		?.click();
	await vi.waitFor(() => expect(ipc.saveOver).toHaveBeenCalledTimes(1));
	flushSync();

	expect(host.textContent).not.toContain('not there any more');

	return unmount(component);
});

/** The other way out keeps the work somewhere else, and must not then try to
 * read back a file that is not there. */
it('keeps the work elsewhere when the vault file is gone', async () => {
	ipc.save.mockRejectedValue({ code: 'gone', message: 'the database file is gone' });
	ipc.rival.mockResolvedValue({ modified: null, entries: null });
	ipc.saveCopy.mockResolvedValue({ path: '/Users/someone/rescued.kdbx', name: 'rescued' });
	ipc.createEntry.mockResolvedValue({ tree: root, entry: kept.id });
	ipc.entry.mockResolvedValue(entry({ id: kept.id, group: root.id }));

	const component = open();
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Entry')
		?.click();
	flushSync();
	inPicker('Enter');
	expect(ipc.createEntry).toHaveBeenCalledWith(root.id, 'login');
	await vi.waitFor(() => expect(host.textContent).toContain('not there any more'));
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Keep it elsewhere')
		?.click();
	await vi.waitFor(() => expect(ipc.saveCopy).toHaveBeenCalledTimes(1));
	flushSync();

	expect(ipc.reload, 'it read back a file that is not there').not.toHaveBeenCalled();
	expect(host.textContent).toContain('Kept as “\u2068rescued\u2069”');

	return unmount(component);
});

/** A vault Coffer will not write back offers nothing that would only be
 * refused. */
it('offers no change at all on a database it cannot write', () => {
	const component = open({ readOnly: true });
	flushSync();

	const named = [...host.querySelectorAll('button')].map((each) => each.textContent?.trim());
	expect(named).not.toContain('Entry');
	expect(host.querySelector('[aria-label="New folder"]')).toBeNull();
	expect(host.textContent).toContain('Read only');

	return unmount(component);
});

/**
 * There is no save button, so every change has to reach the file by itself.
 * Dropping a version and clearing a history are changes like any other: a
 * screen that left them in memory would lose them at the next lock.
 */
it('writes the file after a version is dropped', async () => {
	ipc.entry.mockResolvedValue(entry({ id: kept.id, group: root.id }));
	ipc.versions.mockImplementation(listing([{ index: 0, modified: '2021-06-02T12:00:00Z' }]));
	ipc.deleteVersion.mockImplementation(listing());

	const component = open();
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('node-3'))
		?.click();
	await vi.waitFor(() => expect(ipc.versions).toHaveBeenCalled());
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('Versions'))
		?.click();
	flushSync();

	expect(ipc.save).not.toHaveBeenCalled();
	host.querySelector<HTMLButtonElement>('[aria-label="Delete this version"]')?.click();
	flushSync();
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Drop it')
		?.click();
	await vi.waitFor(() => expect(ipc.save).toHaveBeenCalledTimes(1));

	return unmount(component);
});

/**
 * A row in the list is drawn from the tree. An edit that changed the pane and
 * not the tree would leave the list and the search index holding a value that
 * is no longer in the file.
 */
it('redraws the list after an entry is edited', async () => {
	const renamed = { ...kept, title: 'node-4' };
	ipc.entry.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: [field({ name: 'Title', kind: 'title', value: 'node-3', empty: false })]
		})
	);
	ipc.setField.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: [field({ name: 'Title', kind: 'title', value: 'node-4', empty: false })]
		})
	);
	ipc.tree.mockResolvedValue(group({ ...root, entries: [renamed], sections: [] }));

	const onTree = vi.fn();
	const component = open({ onTree });
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('node-3'))
		?.click();
	await vi.waitFor(() => expect(ipc.entry).toHaveBeenCalled());
	flushSync();

	const title = host.querySelector('h1 input') as HTMLInputElement;
	write(title, 'node-4');

	await vi.waitFor(() => expect(ipc.tree).toHaveBeenCalled());
	await vi.waitFor(() =>
		expect(onTree).toHaveBeenCalledWith(expect.objectContaining({ entries: [renamed] }))
	);

	return unmount(component);
});

/**
 * The two copy shortcuts act on the entry that is open. A reader who has
 * selected something in a field is copying that, and the platform's own copy
 * has to be the one that happens.
 */
it('leaves a copy made inside a field to the field', async () => {
	ipc.entry.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
		})
	);

	const component = open();
	flushSync();
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('node-3'))
		?.click();
	await vi.waitFor(() => expect(ipc.entry).toHaveBeenCalled());
	flushSync();

	const writing = host.querySelector('input') as HTMLInputElement;
	const pressed = new KeyboardEvent('keydown', { key: 'c', metaKey: true, bubbles: true });
	writing.dispatchEvent(pressed);
	flushSync();

	expect(ipc.copy).not.toHaveBeenCalled();
	expect(pressed.defaultPrevented).toBe(false);

	return unmount(component);
});

/** The same plus, in the folder pane: it opens the field that asks for a name
 * and it closes it again. */
it('closes the folder name it opened when the plus is pressed again', () => {
	const component = open();
	flushSync();

	const plus = host.querySelector('[aria-label="New folder"]') as HTMLButtonElement;
	plus.click();
	flushSync();
	expect(host.querySelector('[aria-label="The name of the new folder"]')).not.toBeNull();

	const press = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
	const opener = host.querySelector(
		'[aria-label="Never mind the new folder"]'
	) as HTMLButtonElement;
	opener.dispatchEvent(press);
	expect(press.defaultPrevented).toBe(true);

	opener.click();
	flushSync();

	expect(host.querySelector('[aria-label="The name of the new folder"]')).toBeNull();
	expect(ipc.createGroup).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * The status bar used to tick a countdown once a second in the corner of every
 * screen, which moves for no reason a reader can act on. The settings are what
 * that corner carries now.
 */
it('offers the settings from the status bar, and counts nothing down', () => {
	const onSettings = vi.fn();
	const component = open({ onSettings });
	flushSync();

	expect(reads()).not.toContain('Locks in');

	const button = host.querySelector<HTMLButtonElement>('button[aria-label="Settings"]');
	expect(button, 'the status bar offers no way to the settings').not.toBeNull();
	button?.click();
	expect(onSettings).toHaveBeenCalledTimes(1);

	unmount(component);
});

/** Two `ml-auto` siblings in a flex row do not both push right: the second one
 * lands wherever the first one left it, which is the middle of the status bar. */
it('keeps everything on the right in one group', () => {
	const component = open({ readOnly: true });
	flushSync();

	const pushed = [...host.querySelectorAll('.ml-auto')];
	expect(pushed).toHaveLength(1);
	expect(pushed[0].textContent).toContain('Read only');
	expect(pushed[0].textContent).toContain('Settings');

	unmount(component);
});

/**
 * The way out of an open entry, for a pointer.
 *
 * Escape has always closed it and nothing else did, so an entry opened with the
 * mouse could only be put away with the keyboard. The empty part of either pane
 * to the left of it is what a reader reaches for, and it is only the empty part:
 * a press on a row arrives at the same element on its way up, and that one means
 * what it says.
 *
 * Both halves are asserted, and separately. A test that only pressed the empty
 * part would pass with the guard deleted, and one that only pressed a row would
 * pass with the whole handler deleted.
 */
async function opened(): Promise<HTMLElement[]> {
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('node-3'))
		?.click();
	await vi.waitFor(() => expect(ipc.entry).toHaveBeenCalledWith(kept.id));
	flushSync();
	expect(host.querySelector('h1 input'), 'the entry never opened').not.toBeNull();

	const empty = [...host.querySelectorAll<HTMLElement>('[role="presentation"]')];
	expect(empty, 'the folders and the list offer nowhere to press').toHaveLength(2);
	return empty;
}

it('puts the open entry away when the empty part of the list is pressed', async () => {
	ipc.entry.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: [field({ name: 'Title', kind: 'title', value: 'node-3', empty: false })]
		})
	);

	const component = open();
	flushSync();
	const [, list] = await opened();

	// A press that landed on a row of the list is a press on that row. It reaches
	// the same element on the way up, and the entry stays open.
	const row = list.querySelector('button');
	if (!row) throw new Error('the list drew no rows');
	row.dispatchEvent(new MouseEvent('click', { bubbles: true }));
	await tick();
	flushSync();
	expect(host.querySelector('h1 input'), 'a press on a row put the entry away').not.toBeNull();

	list.dispatchEvent(new MouseEvent('click', { bubbles: true }));
	flushSync();
	expect(host.querySelector('h1 input'), 'the list kept the entry open').toBeNull();

	unmount(component);
});

it('puts the open entry away when the empty part of the folders is pressed', async () => {
	ipc.entry.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: [field({ name: 'Title', kind: 'title', value: 'node-3', empty: false })]
		})
	);

	const component = open();
	flushSync();
	const [folders] = await opened();

	// Not through a folder: selecting one closes the entry by another route
	// entirely, and a test that went that way would prove nothing about this one.
	folders.dispatchEvent(new MouseEvent('click', { bubbles: true }));
	flushSync();
	expect(host.querySelector('h1 input'), 'the folders kept the entry open').toBeNull();

	unmount(component);
});

/**
 * The guard the folder pane's dismiss handler needs, which nothing else states.
 *
 * The tree draws a chevron that expands a folder without selecting it, and that
 * chevron is inside the same element the empty part of the pane is. Without the
 * guard, opening a folder to look for something closes the entry being looked
 * at.
 */
it('leaves the open entry alone when a folder is only expanded', async () => {
	ipc.entry.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: [field({ name: 'Title', kind: 'title', value: 'node-3', empty: false })]
		})
	);

	const component = open();
	flushSync();
	await opened();

	const chevron = host.querySelector<HTMLButtonElement>('button[aria-label^="Expand"]');
	expect(chevron, 'the tree drew no folder to expand').not.toBeNull();
	chevron?.click();
	flushSync();

	expect(host.querySelector('h1 input'), 'expanding a folder put the entry away').not.toBeNull();

	unmount(component);
});

/**
 * The settings sit over the panes and not over the status bar, because the way
 * in to them is in the status bar. A settings screen that took the whole window
 * moved its own button from the bottom right corner to the top left one, in the
 * moment it was pressed.
 */
it('keeps the way out of the settings where the way in was', () => {
	const onSettings = vi.fn();
	const component = open({ onSettings, settings: sheet });
	flushSync();

	expect(reads(), 'the settings are not on the screen').toContain('The settings');

	const corner = host.querySelector('.ml-auto');
	const back = corner?.querySelector<HTMLButtonElement>('button[aria-label="Back to the vault"]');
	expect(back, 'the settings moved their own button out of the corner').not.toBeNull();

	back?.click();
	expect(onSettings).toHaveBeenCalledTimes(1);

	unmount(component);
});

/**
 * A sheet that only covers the vault is a vault that can still be typed into.
 *
 * Every value in an entry is a live field that writes what is in it the moment
 * focus leaves, so two presses of Tab out of the settings used to land in an
 * entry nobody could see and the next press saved what had been typed there.
 * `inert` is what takes the covered pane out of the tab order, out of hit
 * testing and out of the accessibility tree at once.
 */
it('makes the vault under the settings unreachable, not merely hidden', async () => {
	ipc.entry.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: [
				field({ name: 'Title', kind: 'title', value: 'node-3', empty: false }),
				field({ name: 'Notes', kind: 'notes', value: 'the reboot window', empty: false })
			]
		})
	);

	const component = open({ settings: sheet });
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('node-3'))
		?.click();
	await vi.waitFor(() => expect(ipc.entry).toHaveBeenCalledWith(kept.id));
	flushSync();

	const notes = host.querySelector<HTMLTextAreaElement>('textarea[aria-label="Notes"]');
	expect(notes, 'the covered entry never drew its notes').not.toBeNull();

	const shut = notes?.closest('[inert]');
	expect(shut, 'the entry under the settings is only painted over').not.toBeNull();
	expect(shut?.contains(host.querySelector('.ml-auto')), 'the status bar went inert too').toBe(
		false
	);

	unmount(component);
});

/** Escape is the way back out of everything else here, and the list it would
 * otherwise act on is not on the screen. */
it('lets Escape out of the settings, and leaves the list alone while they are open', () => {
	const onSettings = vi.fn();
	const component = open({ onSettings, settings: sheet });
	flushSync();

	expect(applying(), 'the menu offers the list under the settings').not.toContain('find');
	run('find');
	flushSync();
	expect(document.activeElement, 'a shortcut reached the list under the settings').not.toBe(
		search()
	);

	window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
	flushSync();
	expect(onSettings).toHaveBeenCalledTimes(1);

	// A control inside the settings that answered its own Escape has answered it:
	// the chip that opens a list of timeouts takes one to close the list, and the
	// whole screen used to close behind it.
	const answered = new KeyboardEvent('keydown', { key: 'Escape', cancelable: true });
	answered.preventDefault();
	window.dispatchEvent(answered);
	flushSync();
	expect(
		onSettings,
		'an Escape a control had already taken closed the screen'
	).toHaveBeenCalledTimes(1);

	unmount(component);
});

/**
 * A file that a previous version of some other entry names has to keep the
 * number it has, so now and then an entry in the bin cannot be erased yet.
 *
 * Emptying is best-effort in Rust: whatever can go has gone by the time the
 * refusal comes back. The screen therefore has to read the tree again and say
 * what to do next, rather than report a failure and leave a bin drawn fuller
 * than it is with no way forward.
 */
it('says what to do when part of the bin cannot be emptied yet', async () => {
	const emptied = group({
		name: 'Root',
		entries: [kept],
		sections: [group({ name: 'Recycle Bin', isRecycleBin: true, entries: [] })]
	});
	ipc.emptyRecycleBin.mockRejectedValue({
		code: 'attachmentInHistory',
		message: 'earlier versions of an entry still hold that file in place'
	});
	ipc.tree.mockResolvedValue(emptied);

	const onTree = vi.fn();
	const component = open({ onTree });
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('Recycle Bin'))
		?.click();
	flushSync();
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Empty the bin')
		?.click();
	flushSync();
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Empty it')
		?.click();

	await vi.waitFor(() => expect(host.textContent).toContain('Some of it stayed'));
	// What did go is off the screen: the tree is read again rather than left
	// drawing a bin that is fuller than the file's.
	expect(onTree).toHaveBeenCalledWith(emptied);
	// And it reaches the file. Emptying is all-or-nothing per entry, so the ones
	// that went are out of the vault in memory and nowhere else until this runs.
	await vi.waitFor(() => expect(ipc.save).toHaveBeenCalled());

	return unmount(component);
});

/**
 * A save refused for anything but a conflict raises a notice that fades after
 * six seconds. A reader who missed it went on editing into a window whose every
 * write was failing, and the idle timer was what ended the session.
 */
it('keeps saying not saved after a write that only raised a notice', async () => {
	ipc.entry.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: [field({ name: 'Title', kind: 'title', value: 'node-3', empty: false })]
		})
	);
	ipc.setField.mockResolvedValue(entry({ id: kept.id, group: root.id }));
	ipc.save.mockRejectedValue({
		code: 'other',
		message: 'No space left on device'
	});

	const component = open();
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('node-3'))
		?.click();
	await vi.waitFor(() => expect(ipc.entry).toHaveBeenCalled());
	flushSync();

	const title = host.querySelector('h1 input') as HTMLInputElement;
	write(title, 'node-4');

	await vi.waitFor(() => expect(host.textContent).toContain('Not saved'));

	// It is not the toast, which is what the defect was: the notice goes and
	// this does not.
	vi.useFakeTimers();
	try {
		await vi.advanceTimersByTimeAsync(30_000);
		flushSync();
		expect(host.textContent).toContain('Not saved');
	} finally {
		vi.useRealTimers();
	}

	return unmount(component);
});

/** An entry with a field of the reader's own, and a password for the copy
 * shortcut to take. */
function withPin(id: string) {
	return entry({
		id,
		group: root.id,
		fields: [
			field({ name: 'Title', kind: 'title', value: 'node-3', empty: false }),
			field({ name: 'Password', kind: 'password', value: null, empty: false }),
			field({ name: 'PIN', kind: 'custom', protected: true, value: null, empty: false })
		]
	});
}

/** The notice in the corner, and the button on it. */
const toast = () => host.querySelector<HTMLElement>('[data-notice]');
const undo = () => toast()?.querySelector<HTMLButtonElement>('button') ?? null;

function press(key: string, target: EventTarget = window, shiftKey = false): KeyboardEvent {
	const event = new KeyboardEvent('keydown', {
		key,
		metaKey: true,
		shiftKey,
		bubbles: true,
		cancelable: true
	});
	target.dispatchEvent(event);
	return event;
}

/** Everything the window is waiting on, with the clock standing still. */
async function settled() {
	for (let round = 0; round < 10; round += 1) await vi.advanceTimersByTimeAsync(0);
	flushSync();
}

/** What Rust answers an undo whose removal is no longer the last thing that
 * happened to its entry. */
const SUPERSEDED = {
	code: 'superseded',
	message: 'the entry has changed since that field came off'
};

/** Opens node-3 and waits until Rust has read it. */
async function openKept() {
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('node-3'))
		?.click();
	await settled();
}

/** Opens node-3, takes its PIN off, and waits for whatever the window then says. */
async function removePin() {
	ipc.entry.mockImplementation((id: string) => Promise.resolve(withPin(id)));
	ipc.removeField.mockReset();
	ipc.removeField.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: withPin(kept.id).fields.filter((each) => each.name !== 'PIN')
		})
	);
	ipc.undoRemoval.mockReset();
	ipc.undoRemoval.mockResolvedValue(withPin(kept.id));

	const component = open();
	flushSync();
	await openKept();

	host.querySelector<HTMLButtonElement>('[aria-label="Remove the field PIN"]')?.click();
	await settled();
	return component;
}

/**
 * A field of the reader's own used to go on one press with nothing said, and the
 * way back was a version nobody was told about. The notice says what went and
 * offers it back once the removal is in the file, and taking it back is one
 * call to Rust with the entry and the field's name: which version puts it back
 * is decided there, under the lock the restore runs in, so nothing can land
 * between the question and the restore.
 */
it('offers a removed field back and takes it back in one call to Rust', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin();

		expect(ipc.removeField).toHaveBeenCalledWith(kept.id, 'PIN', false);
		expect(toast()?.textContent).toContain('Field “\u2068PIN\u2069” removed');
		expect(undo()?.textContent).toContain('Undo');
		expect(ipc.save).toHaveBeenCalledTimes(1);

		// Heard as well as seen, in a region that was there before the words.
		const region = host.querySelector('[role="status"]');
		expect(region?.getAttribute('aria-live')).toBe('polite');
		expect(region?.textContent).toContain('Field “\u2068PIN\u2069” removed');

		undo()?.click();
		await settled();

		expect(ipc.undoRemoval).toHaveBeenCalledTimes(1);
		expect(ipc.undoRemoval).toHaveBeenCalledWith(kept.id, 'PIN');
		expect(ipc.restoreVersion, 'the undo named a position of its own').not.toHaveBeenCalled();
		// The undo is a change like any other, and reaches the file.
		expect(ipc.save).toHaveBeenCalledTimes(2);

		await vi.advanceTimersByTimeAsync(1_000);
		flushSync();
		expect(toast(), 'the offer stayed up after it was taken').toBeNull();

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** Eight seconds, and then the offer is gone with the notice: a key pressed
 * after that is a key pressed about something else. */
it('withdraws the offer once its eight seconds are up', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin();

		await vi.advanceTimersByTimeAsync(7_900);
		flushSync();
		expect(undo(), 'the offer went early').not.toBeNull();

		await vi.advanceTimersByTimeAsync(100);
		flushSync();
		expect(press('z').defaultPrevented).toBe(false);
		await settled();
		expect(ipc.undoRemoval).not.toHaveBeenCalled();

		await vi.advanceTimersByTimeAsync(1_000);
		flushSync();
		expect(toast()).toBeNull();

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/**
 * Cmd+Z is the offer's key everywhere except where the reader is writing. In a
 * text field it is the field's own undo - taking back typing - and the window
 * leaves the key alone for it.
 */
it('takes the removal back on Cmd+Z, except in a field being written', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin();

		const typed = press('z', search());
		await settled();
		expect(typed.defaultPrevented, 'the search field lost its own undo').toBe(false);
		expect(ipc.undoRemoval).not.toHaveBeenCalled();

		// Redo is not undo.
		press('z', window, true);
		await settled();
		expect(ipc.undoRemoval).not.toHaveBeenCalled();

		expect(press('z').defaultPrevented).toBe(true);
		await settled();
		expect(ipc.undoRemoval).toHaveBeenCalledWith(kept.id, 'PIN');

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** The button and the key, and the button twice, are one undo. A second would
 * be refused by Rust at best, and said to have failed. */
it('runs an undo once however many ways it is asked for', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin();

		const button = undo();
		button?.click();
		press('z');
		button?.click();
		await settled();
		press('z');
		await settled();

		expect(ipc.undoRemoval).toHaveBeenCalledTimes(1);

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/**
 * Pressing Undo with the focus in a field that holds typing used to leave the
 * field on the way to the button: its typing was written, that write withdrew
 * the offer, and the click that followed did nothing while the notice went as
 * though it had worked. The press does not take the focus now. The field keeps
 * its typing, unwritten, and the removal is taken back.
 */
it('takes a removal back while a field holds typing, and leaves the typing where it is', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin();
		const title = titleField();
		if (!title) throw new Error('node-3 never opened');
		title.focus();
		title.value = 'node-4';
		title.dispatchEvent(new Event('input', { bubbles: true }));

		const button = undo();
		const down = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
		button?.dispatchEvent(down);
		expect(down.defaultPrevented, 'the press would take the focus from the title').toBe(true);
		// What a browser does with a press nobody stopped.
		if (!down.defaultPrevented) title.blur();
		button?.click();
		await settled();

		expect(ipc.undoRemoval).toHaveBeenCalledWith(kept.id, 'PIN');
		expect(ipc.setField, 'the typing was written in the middle of the undo').not.toHaveBeenCalled();
		expect(document.activeElement).toBe(titleField());
		expect(titleField()?.value).toBe('node-4');

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/**
 * A vault that keeps no versions, or a size limit the version does not fit,
 * loses the field at the next save. Rust refuses the first press, the row asks,
 * and once the reader has said so the notice says the field went forever and
 * offers nothing.
 */
it('asks before a removal nothing could take back, and says it went forever', async () => {
	vi.useFakeTimers();
	try {
		ipc.entry.mockImplementation((id: string) => Promise.resolve(withPin(id)));
		ipc.removeField.mockReset();
		ipc.removeField
			.mockRejectedValueOnce({
				code: 'forGood',
				message: 'this vault keeps no version to bring that field back from'
			})
			.mockResolvedValue(
				entry({
					id: kept.id,
					group: root.id,
					fields: withPin(kept.id).fields.filter((each) => each.name !== 'PIN')
				})
			);
		const component = open();
		flushSync();
		await openKept();

		host.querySelector<HTMLButtonElement>('[aria-label="Remove the field PIN"]')?.click();
		await settled();
		expect(pane()?.querySelector('[data-confirm]')?.textContent).toContain('can’t be undone');
		expect(toast(), 'the question was reported as a failure').toBeNull();
		expect(ipc.save).not.toHaveBeenCalled();

		pressed('Remove', pane() ?? host);
		await settled();

		expect(ipc.removeField).toHaveBeenLastCalledWith(kept.id, 'PIN', true);
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(toast()?.textContent).toContain('Field “\u2068PIN\u2069” removed forever');
		expect(undo()).toBeNull();
		expect(press('z').defaultPrevented).toBe(false);
		await settled();
		expect(ipc.undoRemoval).not.toHaveBeenCalled();

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/**
 * An undo acts on the entry it is about, by its id, whatever the pane shows by
 * the time it is pressed. Opening another entry is not taking anything back,
 * so the offer stays for its eight seconds, and the undo restores node-3 and
 * leaves the pane on the entry the reader chose.
 */
it('keeps the offer when another entry is opened, and takes back the entry it is about', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin();

		[...host.querySelectorAll('button')]
			.find((each) => each.textContent?.includes('Postgres'))
			?.click();
		await settled();
		expect(ipc.entry).toHaveBeenLastCalledWith(other.id);
		expect(undo()).not.toBeNull();
		ipc.entry.mockClear();

		expect(press('z').defaultPrevented).toBe(true);
		await settled();
		expect(ipc.undoRemoval).toHaveBeenCalledWith(kept.id, 'PIN');
		expect(ipc.save).toHaveBeenCalledTimes(2);
		expect(ipc.entry, 'the undo took the pane from Postgres').not.toHaveBeenCalledWith(kept.id);

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** Putting the pane away is not taking anything back either, and the undo does
 * not open the entry again. */
it('keeps the offer when the entry is put away', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin();

		window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
		await settled();
		expect(pane()).toBeNull();

		press('z');
		await settled();
		expect(ipc.undoRemoval).toHaveBeenCalledWith(kept.id, 'PIN');
		expect(pane(), 'the undo opened the entry again').toBeNull();

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** A newer notice is about something newer. The undo belonged to the sentence
 * that is no longer on the screen. */
it('withdraws the offer when a newer notice takes its place', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin();

		press('c');
		await settled();
		expect(toast()?.textContent).toContain('Password copied.');
		expect(undo()).toBeNull();

		press('z');
		await settled();
		expect(ipc.undoRemoval).not.toHaveBeenCalled();

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/**
 * Another change to the entry reaches the file after the removal, and the
 * offer is still up. It is Rust that decides, at the press, that the removal is
 * no longer the last thing that happened: it restores nothing, and the reader
 * is told the removal can no longer be undone. The change stays, and nothing
 * more is written.
 */
it('leaves an undo overtaken by another change to Rust, which refuses it', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin();
		ipc.setField.mockResolvedValue(withPin(kept.id));
		ipc.undoRemoval.mockRejectedValue(SUPERSEDED);

		write(titleField() as HTMLInputElement, 'node-4');
		await settled();
		expect(ipc.save).toHaveBeenCalledTimes(2);
		expect(undo(), 'another change withdrew the offer').not.toBeNull();

		undo()?.click();
		await settled();

		expect(ipc.undoRemoval).toHaveBeenCalledTimes(1);
		expect(toast()?.textContent).toContain(
			'Something has changed since, so that can no longer be undone.'
		);
		expect(undo()).toBeNull();
		expect(ipc.save, 'a refusal is not a change').toHaveBeenCalledTimes(2);

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/**
 * A removal the save refused is in memory and not in the file, and the window
 * already said so. An offer over that notice would push the one sentence that
 * matters off the screen.
 */
it('offers nothing over a save that failed', async () => {
	vi.useFakeTimers();
	try {
		ipc.save.mockRejectedValue({ code: 'other', message: 'No space left on device' });
		const component = await removePin();

		expect(toast()?.textContent).toContain('No space left on device');
		expect(undo()).toBeNull();
		expect(reads()).toContain('Not saved');

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** Any other refusal at the press is said in its own words, and nothing is
 * written. */
it('says why an undo Rust would not do failed', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin();
		ipc.undoRemoval.mockRejectedValue({
			code: 'noSuchEntry',
			message: 'there is no such entry in this database'
		});

		undo()?.click();
		await settled();

		expect(toast()?.textContent).toContain('there is no such entry in this database');
		expect(ipc.save).toHaveBeenCalledTimes(1);

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** A lock takes the window down, and the offer and its clock with it. */
it('leaves nothing to undo once the window is gone', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin();
		await unmount(component);

		press('z');
		await vi.advanceTimersByTimeAsync(10_000);
		expect(ipc.undoRemoval).not.toHaveBeenCalled();
		expect(vi.getTimerCount(), 'a notice clock outlived the window').toBe(0);
	} finally {
		vi.useRealTimers();
	}
});

/**
 * The folder's own question, now the one every confirmation in the window is.
 * It asks in words, keeps the folder on the way out and on Escape - without
 * the Escape reaching the list as well - and moves it only on the red answer,
 * which says where it goes. Once it has gone to the bin it is offered back.
 */
it('asks before a folder goes, and keeps it on the way out', async () => {
	const work = root.sections[0];
	const binned = group({
		...root.sections[1],
		sections: [
			{
				...work,
				binned: { since: '2026-08-29T14:00:00Z', within: null, from: root.id },
				deletion: 'forever'
			}
		]
	});
	ipc.deleteGroup.mockResolvedValue(group({ ...root, sections: [binned] }));
	ipc.putBackGroup.mockResolvedValue(root);

	const component = open();
	flushSync();
	[...host.querySelectorAll('button')].find((each) => each.textContent?.includes('Work'))?.click();
	flushSync();

	const trash = () => host.querySelector<HTMLButtonElement>('[aria-label="Delete this folder"]');
	// The trash has room, like every other: a box of its own, last in the row
	// and a step off the safe icons, where a press meant for one of them used
	// to land on it.
	expect(trash()?.className).toMatch(/\bh-7\b.*\bw-7\b/);
	expect(trash()?.className).toContain('hover:bg-dangerwash');
	expect(trash()?.className).toContain('ml-2');
	expect(trash()?.nextElementSibling).toBeNull();
	expect(trash()?.previousElementSibling?.getAttribute('aria-label')).toBe('New folder');
	trash()?.click();
	flushSync();
	expect(host.querySelector('[data-confirm]')?.textContent).toContain(
		'Move “\u2068Work\u2069” and everything in it to the Recycle Bin?'
	);
	const keep = [...host.querySelectorAll('button')].find(
		(each) => each.textContent?.trim() === 'Keep it'
	);
	expect(document.activeElement).toBe(keep);

	keep?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
	flushSync();
	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(ipc.deleteGroup).not.toHaveBeenCalled();
	expect(reads(), 'the Escape went on to the list').toContain('1 entry here');

	trash()?.click();
	flushSync();
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Move to Recycle Bin')
		?.click();
	await vi.waitFor(() => expect(ipc.deleteGroup).toHaveBeenCalledWith(work.id, 'bin'));
	await vi.waitFor(() => expect(ipc.save).toHaveBeenCalledTimes(1));
	await vi.waitFor(() =>
		expect(toast()?.textContent).toContain('Moved “\u2068Work\u2069” to the Recycle Bin')
	);

	undo()?.click();
	await vi.waitFor(() => expect(ipc.putBackGroup).toHaveBeenCalledWith(work.id));
	await vi.waitFor(() => expect(ipc.save).toHaveBeenCalledTimes(2));

	return unmount(component);
});

/** Emptying the bin is the one deletion nothing comes back from, and the way
 * out of the question leaves every entry in it. */
it('keeps the bin as it is when the reader keeps it', () => {
	const component = open();
	flushSync();
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('Recycle Bin'))
		?.click();
	flushSync();
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Empty the bin')
		?.click();
	flushSync();

	expect(host.querySelector('[data-confirm]')?.textContent).toContain(
		'Delete everything in the bin forever? Nothing in it can be put back afterwards'
	);
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Keep it')
		?.click();
	flushSync();

	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(ipc.emptyRecycleBin).not.toHaveBeenCalled();
	expect(host.textContent).toContain('thrown away');

	return unmount(component);
});

/** Everything in the vault as a reader who deleted a few things would have it:
 * a folder of their own in the bin with an entry inside it, and an entry that
 * went on its own. */
function binnedVault() {
	const personal = group({ name: 'Personal', entries: [row({ title: 'Bank' })] });
	const work = group({ name: 'Work' });
	const deleted = group({
		name: 'Banking',
		binned: { since: '2026-08-26T09:00:00Z', within: null, from: personal.id },
		deletion: 'forever'
	});
	const card = row({
		title: 'Visa',
		binned: { since: '2026-08-26T09:00:00Z', within: deleted.id, from: personal.id }
	});
	const banking = { ...deleted, entries: [card] };
	const mail = row({
		title: 'Old mail',
		binned: { since: '2026-08-28T10:00:00Z', within: null, from: work.id }
	});
	const bin = group({
		name: 'Recycle Bin',
		isRecycleBin: true,
		deletion: 'forever',
		entries: [mail],
		sections: [banking]
	});
	const tree = group({ name: 'Root', deletion: 'forever', sections: [personal, work, bin] });
	return { tree, personal, work, bin, banking, card, mail };
}

/** The same vault after the folder in the bin went back where it came from. */
function putBack(vault: ReturnType<typeof binnedVault>) {
	const banking = {
		...vault.banking,
		binned: null,
		deletion: 'bin' as const,
		entries: [{ ...vault.card, binned: null }]
	};
	return group({
		...vault.tree,
		sections: [
			{ ...vault.personal, sections: [banking] },
			vault.work,
			{ ...vault.bin, sections: [] }
		]
	});
}

function titled(id: string, title: string, over: Parameters<typeof entry>[0] = {}) {
	return entry({
		id,
		group: root.id,
		fields: [field({ name: 'Title', kind: 'title', value: title, empty: false })],
		...over
	});
}

/** A window on a given tree, with the clock where the dates in it make sense. */
function mounted(tree: typeof root, readOnly = false) {
	vi.useFakeTimers();
	vi.setSystemTime(new Date('2026-08-29T14:30:00Z'));
	ipc.tree.mockResolvedValue(tree);
	const onTree = vi.fn();
	const onSettings = vi.fn();
	const component = mount(Vault, {
		target: host,
		props: { database, root: tree, kinds: kinds(), readOnly, onSettings, onTree }
	});
	flushSync();
	return { component, onTree, onSettings };
}

function pressed(label: string, within: ParentNode = host) {
	const found = [...within.querySelectorAll('button')].find(
		(each) => each.textContent?.trim() === label || each.textContent?.includes(label)
	);
	if (!found) throw new Error(`no button for ${label}`);
	found.click();
}

/** The card in the list that says the folder being shown is in the bin. */
const folderCard = () =>
	[...host.querySelectorAll<HTMLElement>('[data-binned]')].find(
		(card) => !card.closest('section')
	) ?? null;
/** The card in the entry pane that says the entry is in the bin. */
const entryCard = () => host.querySelector<HTMLElement>('section [data-binned]');

/**
 * A folder that went into the bin is a folder there: a row of its own with
 * when it went and where from, above the entries that went on their own - and
 * its entries are inside it rather than poured out around it.
 */
it('shows a deleted folder as a folder in the bin, saying when and where from', async () => {
	const vault = binnedVault();
	const { component } = mounted(vault.tree);
	try {
		pressed('Recycle Bin');
		flushSync();

		const folders = [...host.querySelectorAll('[data-folder]')];
		expect(folders).toHaveLength(1);
		expect(folders[0].textContent).toContain('Banking');
		expect(folders[0].textContent).toContain('Deleted 3 days ago · from “\u2068Personal\u2069”');
		expect(reads()).toContain('Old mail');
		expect(reads()).toContain('Deleted yesterday · from “\u2068Work\u2069”');
		expect(reads(), 'the folder was poured out into the bin').not.toContain('Visa');
		// Nothing is made in the bin.
		expect(host.querySelector('[aria-label="New folder"]')).toBeNull();
		expect(reads()).not.toContain('+ Entry');
		expect(reads()).toContain('Empty the bin');
		// The bin itself is not in the bin: nothing offers to put it back or to
		// delete it from inside.
		expect(folderCard()).toBeNull();
		expect(host.querySelector('[aria-label="Delete this folder"]')).toBeNull();

		(folders[0] as HTMLElement).click();
		flushSync();
		expect(folderCard()?.textContent).toContain('Banking');
		expect(folderCard()?.textContent).toContain(
			'In the Recycle Bin since 26 Aug · was in “\u2068Personal\u2069”'
		);
		expect(reads()).toContain('Visa');
		expect(reads()).toContain('Deleted 3 days ago · with “\u2068Banking\u2069”');
		expect(reads(), 'emptying is for the whole bin, not a folder in it').not.toContain(
			'Empty the bin'
		);
		expect(host.querySelector('[aria-label="Delete this folder"]')).toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * A search in the bin looks inside the folders that went into it. The reader
 * looking for a deleted entry does not know which folder it went in with, and
 * a search that stopped at the bin's own entries said nothing matched while
 * the entry sat one folder down. The row it finds says which folder that was,
 * and the folder rows come back with the empty query.
 */
it('searches the folders in the bin too, and says which one a row went in with', async () => {
	const vault = binnedVault();
	const { component } = mounted(vault.tree);
	try {
		pressed('Recycle Bin');
		flushSync();
		expect(reads(), 'the folder was poured out into the bin').not.toContain('Visa');

		type('visa');
		expect(reads()).not.toContain('Nothing matches');
		expect(reads()).toContain('Visa');
		expect(reads()).toContain('Deleted 3 days ago · with “\u2068Banking\u2069”');
		expect(reads(), 'a search answered with the mail it did not match').not.toContain('Old mail');
		expect(host.querySelector('[data-folder]')).toBeNull();

		type('');
		expect(reads()).not.toContain('Visa');
		expect(host.querySelector('[data-folder]')).not.toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A search is over the vault the reader works in. What is in the bin is not
 * in "All entries" and does not answer a search there. */
it('leaves the bin out of All entries and out of a search there', async () => {
	const vault = binnedVault();
	const { component } = mounted(vault.tree);
	try {
		expect(reads()).toContain('Bank');
		expect(reads()).not.toContain('Old mail');
		type('mail');
		expect(reads()).toContain('Nothing matches');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * Moving an entry to the bin is the one press in the pane that used to lose
 * it, and it is taken back from the notice that follows: for eight seconds, on
 * the notice's button and on Cmd+Z, once. Taking it back opens it again.
 */
it('moves the open entry to the bin and offers it back', async () => {
	const bank = row({ title: 'node-3' });
	const before = group({
		name: 'Root',
		entries: [bank],
		sections: [group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever' })]
	});
	const after = group({
		...before,
		entries: [],
		sections: [
			group({
				name: 'Recycle Bin',
				isRecycleBin: true,
				deletion: 'forever',
				entries: [
					{ ...bank, binned: { since: '2026-08-29T14:30:00Z', within: null, from: before.id } }
				]
			})
		]
	});
	ipc.entry.mockResolvedValue(titled(bank.id, 'node-3'));
	ipc.deleteEntries.mockResolvedValue(after);
	ipc.putBackEntries.mockResolvedValue(before);
	ipc.save.mockResolvedValue(undefined);

	const { component, onTree } = mounted(before);
	try {
		pressed('node-3');
		await settled();
		ipc.entry.mockClear();

		pressed('Move to Recycle Bin');
		await settled();

		expect(ipc.deleteEntries).toHaveBeenCalledWith(
			[{ entry: bank.id, deletion: 'bin' }],
			expect.any(Number)
		);
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(onTree).toHaveBeenLastCalledWith(after);
		expect(host.querySelector('h1'), 'the pane stayed open on a deleted entry').toBeNull();
		expect(toast()?.textContent).toContain('Moved “\u2068node-3\u2069” to the Recycle Bin');
		expect(undo()?.textContent).toContain('Undo');

		expect(press('z').defaultPrevented).toBe(true);
		press('z');
		undo()?.click();
		await settled();

		expect(ipc.putBackEntries).toHaveBeenCalledTimes(1);
		expect(ipc.putBackEntries).toHaveBeenCalledWith([bank.id]);
		expect(ipc.save).toHaveBeenCalledTimes(2);
		expect(onTree).toHaveBeenLastCalledWith(before);
		expect(ipc.entry, 'the entry taken back was not opened again').toHaveBeenCalledWith(bank.id);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Eight seconds, like every offer, and a press after that takes nothing
 * back. */
it('withdraws the offer of a moved entry once its eight seconds are up', async () => {
	const bank = row({ title: 'node-3' });
	const before = group({ name: 'Root', entries: [bank] });
	const bin = group({
		name: 'Recycle Bin',
		isRecycleBin: true,
		deletion: 'forever',
		entries: [{ ...bank, binned: { since: null, within: null, from: before.id } }]
	});
	ipc.entry.mockResolvedValue(titled(bank.id, 'node-3'));
	ipc.deleteEntries.mockResolvedValue(group({ ...before, entries: [], sections: [bin] }));

	const { component } = mounted(before);
	try {
		pressed('node-3');
		await settled();
		pressed('Move to Recycle Bin');
		await settled();

		await vi.advanceTimersByTimeAsync(8_000);
		flushSync();
		expect(press('z').defaultPrevented).toBe(false);
		await settled();
		expect(ipc.putBackEntries).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * What happened is read off the tree Rust answered with. An entry the pane
 * expected to go to the bin and that is not in the file any more went for
 * good, and an offer to put it back would be an offer to do something nobody
 * can.
 */
it('says an entry went for good when it is not in the tree that came back', async () => {
	const bank = row({ title: 'node-3' });
	const before = group({ name: 'Root', entries: [bank] });
	ipc.entry.mockResolvedValue(titled(bank.id, 'node-3', { deletion: 'bin' }));
	ipc.deleteEntries.mockResolvedValue(group({ ...before, entries: [] }));

	const { component } = mounted(before);
	try {
		pressed('node-3');
		await settled();
		pressed('Move to Recycle Bin');
		await settled();

		expect(toast()?.textContent).toContain('Deleted “\u2068node-3\u2069” forever');
		expect(undo()).toBeNull();
		expect(press('z').defaultPrevented).toBe(false);
		await settled();
		expect(ipc.putBackEntries).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A move whose save failed is in memory and not in the file. The failure is
 * the sentence that matters, and an offer would push it off the screen. */
it('offers nothing back over a move whose save failed', async () => {
	const bank = row({ title: 'node-3' });
	const before = group({ name: 'Root', entries: [bank] });
	const bin = group({
		name: 'Recycle Bin',
		isRecycleBin: true,
		deletion: 'forever',
		entries: [{ ...bank, binned: { since: null, within: null, from: before.id } }]
	});
	ipc.entry.mockResolvedValue(titled(bank.id, 'node-3'));
	ipc.deleteEntries.mockResolvedValue(group({ ...before, entries: [], sections: [bin] }));
	ipc.save.mockRejectedValue({ code: 'io', message: 'No space left on device' });

	const { component } = mounted(before);
	try {
		pressed('node-3');
		await settled();
		pressed('Move to Recycle Bin');
		await settled();

		expect(toast()?.textContent).toContain('No space left on device');
		expect(undo()).toBeNull();
		expect(reads()).toContain('Not saved');
	} finally {
		await unmount(component);
		ipc.save.mockReset();
		ipc.save.mockResolvedValue(undefined);
		vi.useRealTimers();
	}
});

/**
 * An entry opened in the bin is read only, and its card is the way out: Put
 * back takes it home and the pane stays on it, read again as an entry like any
 * other; Delete forever asks first and is not offered back.
 */
it('puts an entry back from the bin, or deletes it for good after asking', async () => {
	const vault = binnedVault();
	const inBin = titled(vault.mail.id, 'Old mail', {
		binned: vault.mail.binned,
		deletion: 'forever'
	});
	const home = titled(vault.mail.id, 'Old mail', { group: vault.work.id });
	ipc.entry.mockResolvedValue(inBin);
	ipc.putBackEntries.mockResolvedValue(vault.tree);

	const { component } = mounted(vault.tree);
	try {
		pressed('Recycle Bin');
		flushSync();
		pressed('Old mail');
		await settled();

		expect(entryCard()?.textContent).toContain(
			'In the Recycle Bin since 28 Aug · was in “\u2068Work\u2069”'
		);
		expect(host.querySelector('section h1 input'), 'the title can be written into').toBeNull();

		ipc.entry.mockResolvedValue(home);
		pressed('Put back', entryCard() ?? host);
		await settled();
		expect(ipc.putBackEntries).toHaveBeenCalledWith([vault.mail.id]);
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(entryCard(), 'the pane still says the entry is in the bin').toBeNull();
		expect(host.querySelector('section h1 input')).not.toBeNull();

		// And the other way out, from the bin again.
		ipc.entry.mockResolvedValue(inBin);
		pressed('Old mail');
		await settled();
		ipc.deleteEntries.mockResolvedValue(
			group({
				...vault.tree,
				sections: [vault.personal, vault.work, { ...vault.bin, entries: [] }]
			})
		);
		pressed('Delete forever…', entryCard() ?? host);
		flushSync();
		expect(host.querySelector('[data-confirm]')?.textContent).toContain(
			'Delete “\u2068Old mail\u2069” forever? This can’t be undone.'
		);
		expect(ipc.deleteEntries).not.toHaveBeenCalled();
		pressed('Delete forever', host.querySelector('[data-confirm]') ?? host);
		await settled();

		expect(ipc.deleteEntries).toHaveBeenCalledWith(
			[{ entry: vault.mail.id, deletion: 'forever' }],
			expect.any(Number)
		);
		expect(toast()?.textContent).toContain('Deleted “\u2068Old mail\u2069” forever');
		expect(undo()).toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * A folder put back takes the entries in it along. One of them open in the
 * pane is read again, so the pane does not go on offering to put back an entry
 * that has already left the bin with its folder.
 */
it('puts a folder back and reads the entry open inside it again', async () => {
	const vault = binnedVault();
	const after = putBack(vault);
	ipc.entry.mockResolvedValue(
		titled(vault.card.id, 'Visa', { binned: vault.card.binned, deletion: 'forever' })
	);
	ipc.putBackGroup.mockResolvedValue(after);

	const { component, onTree } = mounted(vault.tree);
	try {
		pressed('Recycle Bin');
		flushSync();
		(host.querySelector('[data-folder]') as HTMLElement).click();
		flushSync();
		pressed('Visa');
		await settled();
		expect(entryCard()).not.toBeNull();

		ipc.entry.mockResolvedValue(titled(vault.card.id, 'Visa'));
		pressed('Put back', folderCard() ?? host);
		await settled();

		expect(ipc.putBackGroup).toHaveBeenCalledWith(vault.banking.id);
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(onTree).toHaveBeenLastCalledWith(after);
		expect(ipc.entry).toHaveBeenLastCalledWith(vault.card.id);
		expect(entryCard(), 'the pane still offers to put back an entry that is out').toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A folder deleted for good from inside the bin asks first, goes, and the
 * list goes up to the folder it was in. */
it('deletes a folder in the bin for good after asking, and goes up a level', async () => {
	const vault = binnedVault();
	const after = group({
		...vault.tree,
		sections: [vault.personal, vault.work, { ...vault.bin, sections: [] }]
	});
	ipc.deleteGroup.mockResolvedValue(after);

	const { component } = mounted(vault.tree);
	try {
		pressed('Recycle Bin');
		flushSync();
		(host.querySelector('[data-folder]') as HTMLElement).click();
		flushSync();

		pressed('Delete forever…', folderCard() ?? host);
		flushSync();
		expect(host.querySelector('[data-confirm]')?.textContent).toContain(
			'Delete “\u2068Banking\u2069” and everything in it forever? This can’t be undone.'
		);
		pressed('Delete forever', host.querySelector('[data-confirm]') ?? host);
		await settled();

		expect(ipc.deleteGroup).toHaveBeenCalledWith(vault.banking.id, 'forever');
		expect(toast()?.textContent).toContain('Deleted “\u2068Banking\u2069” forever');
		expect(undo()).toBeNull();
		expect(reads(), 'the list did not go back up to the bin').toContain('Old mail');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * A vault that keeps no bin, or a folder the bin is inside, deletes a folder
 * for good. Rust has already said so, and the question says it too.
 */
it('says so when deleting a folder is for good', async () => {
	const work = group({ name: 'Work', deletion: 'forever' });
	const tree = group({ name: 'Root', sections: [work] });
	ipc.deleteGroup.mockResolvedValue(group({ ...tree, sections: [] }));

	const { component } = mounted(tree);
	try {
		pressed('Work');
		flushSync();
		host.querySelector<HTMLButtonElement>('[aria-label="Delete this folder"]')?.click();
		flushSync();

		expect(host.querySelector('[data-confirm]')?.textContent).toContain(
			'Delete “\u2068Work\u2069” and everything in it forever? This can’t be undone.'
		);
		pressed('Delete forever', host.querySelector('[data-confirm]') ?? host);
		await settled();

		expect(ipc.deleteGroup).toHaveBeenCalledWith(work.id, 'forever');
		expect(toast()?.textContent).toContain('Deleted “\u2068Work\u2069” forever');
		expect(undo()).toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * The empty bin used to promise that entries stay until it is emptied by
 * hand, which stopped being all of the truth the moment one could be put back
 * or deleted on its own. A bin holding only a folder is not empty.
 */
it('says what the bin keeps, and does not call a bin with a folder in it empty', async () => {
	const empty = group({
		name: 'Root',
		sections: [group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever' })]
	});
	const first = mounted(empty);
	try {
		pressed('Recycle Bin');
		flushSync();
		expect(reads()).toContain('The recycle bin is empty');
		expect(reads()).toContain('Anything moved here waits until it is put back or deleted forever.');
		expect(reads()).not.toContain('emptied by hand');
		expect(reads()).not.toContain('Empty the bin');
	} finally {
		await unmount(first.component);
		vi.useRealTimers();
	}

	const vault = binnedVault();
	const onlyFolder = group({
		...vault.tree,
		sections: [vault.personal, vault.work, { ...vault.bin, entries: [] }]
	});
	const second = mounted(onlyFolder);
	try {
		pressed('Recycle Bin');
		flushSync();
		expect(reads()).not.toContain('The recycle bin is empty');
		expect(host.querySelectorAll('[data-folder]')).toHaveLength(1);
		expect(reads()).toContain('Empty the bin');
	} finally {
		await unmount(second.component);
		vi.useRealTimers();
	}
});

/** A database Coffer will not write back shows its bin and offers nothing it
 * would only refuse: no Put back, no Delete forever, no emptying. */
it('offers no way out of the bin on a database it cannot write', async () => {
	const vault = binnedVault();
	const { component } = mounted(vault.tree, true);
	try {
		pressed('Recycle Bin');
		flushSync();
		expect(reads()).not.toContain('Empty the bin');
		(host.querySelector('[data-folder]') as HTMLElement).click();
		flushSync();

		expect(folderCard()?.textContent).toContain('was in “\u2068Personal\u2069”');
		expect(reads()).not.toContain('Put back');
		expect(reads()).not.toContain('Delete forever');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Folder names in the bin are values out of somebody's database, and they
 * are written as text wherever the bin shows them. */
it('writes the names of folders in the bin as text', async () => {
	const hostile = '<img src=x onerror="alert(1)">';
	const from = group({ name: hostile });
	const gone = group({
		name: hostile,
		binned: { since: null, within: null, from: from.id },
		deletion: 'forever'
	});
	const tree = group({
		name: 'Root',
		sections: [
			from,
			group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever', sections: [gone] })
		]
	});
	const { component } = mounted(tree);
	try {
		pressed('Recycle Bin');
		flushSync();
		expect(host.querySelector('[data-folder]')?.textContent).toContain(
			`Deleted from “\u2068${hostile}\u2069”`
		);
		(host.querySelector('[data-folder]') as HTMLElement).click();
		flushSync();
		expect(folderCard()?.textContent).toContain(
			`In the Recycle Bin · was in “\u2068${hostile}\u2069”`
		);
		expect(host.querySelector('img')).toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * The button stays on the screen until Rust answers. A double press used to be
 * two deletions: the first moved the entry to the bin, and the second found it
 * there and took it out of the file.
 */
it('moves an entry to the bin once however quickly the button is pressed again', async () => {
	const bank = row({ title: 'node-3' });
	const before = group({ name: 'Root', entries: [bank] });
	const bin = group({
		name: 'Recycle Bin',
		isRecycleBin: true,
		deletion: 'forever',
		entries: [{ ...bank, binned: { since: null, within: null, from: before.id } }]
	});
	ipc.entry.mockResolvedValue(titled(bank.id, 'node-3'));
	ipc.deleteEntries.mockResolvedValue(group({ ...before, entries: [], sections: [bin] }));

	const { component } = mounted(before);
	try {
		pressed('node-3');
		await settled();
		const button = [...host.querySelectorAll('button')].find(
			(each) => each.textContent?.trim() === 'Move to Recycle Bin'
		);
		button?.click();
		button?.click();
		await settled();

		expect(ipc.deleteEntries).toHaveBeenCalledTimes(1);
		expect(toast()?.textContent).toContain('Moved “\u2068node-3\u2069” to the Recycle Bin');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * The window going behind another may be the reader reaching for the lid, and
 * a lid closed inside the pause after the last key would lose that word. So
 * what is typed is told to Rust the moment the window loses focus - still
 * without leaving the field, which has written nothing yet.
 */
it('tells Rust what is being typed the moment the window loses focus', async () => {
	ipc.entry.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: [field({ name: 'Notes', kind: 'notes', value: '', empty: true })]
		})
	);
	const component = open();
	try {
		flushSync();
		[...host.querySelectorAll('button')]
			.find((each) => each.textContent?.includes('node-3'))
			?.click();
		await vi.waitFor(() => expect(host.querySelector('[aria-label="Notes"]')).not.toBeNull());
		flushSync();

		const notes = host.querySelector('[aria-label="Notes"]') as HTMLTextAreaElement;
		notes.value = 'Wi-Fi: the long one on the router';
		notes.dispatchEvent(new Event('input', { bubbles: true }));
		expect(ipc.draft).not.toHaveBeenCalled();

		window.dispatchEvent(new Event('blur'));
		expect(ipc.draft).toHaveBeenCalledWith(
			kept.id,
			'Notes',
			'Wi-Fi: the long one on the router',
			false,
			false,
			expect.any(Number)
		);
		expect(ipc.setField, 'the window going behind wrote the note').not.toHaveBeenCalled();
	} finally {
		await unmount(component);
	}
});

/** An entry deleted with something typed into it takes the typing with it, and
 * the deletion carries a number newer than the draft, so the draft is not
 * written into the entry if it is put back. */
it('deletes an entry with a number newer than anything typed into it', async () => {
	const bank = row({ title: 'node-3' });
	const before = group({
		name: 'Root',
		entries: [bank],
		sections: [group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever' })]
	});
	ipc.entry.mockResolvedValue(titled(bank.id, 'node-3'));
	ipc.deleteEntries.mockResolvedValue(before);
	ipc.save.mockResolvedValue(undefined);

	const { component } = mounted(before);
	try {
		pressed('node-3');
		await settled();
		const title = host.querySelector('h1 input') as HTMLInputElement;
		title.value = 'node-3, renamed half';
		title.dispatchEvent(new Event('input', { bubbles: true }));
		window.dispatchEvent(new Event('blur'));
		const drafted = ipc.draft.mock.lastCall?.[5] as number;

		pressed('Move to Recycle Bin');
		await settled();

		expect(ipc.deleteEntries).toHaveBeenCalledWith(
			[{ entry: bank.id, deletion: 'bin' }],
			expect.any(Number)
		);
		expect(ipc.deleteEntries.mock.lastCall?.[1]).toBeGreaterThan(drafted);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * Two logins a reader updates one after the other. Rust answers one command at
 * a time and a save holds it for a whole key derivation, so an entry chosen
 * right after an edit is read a second later, behind that save.
 */
function twoLogins() {
	const gmail = row({ title: 'Gmail', username: 'me@example.com' });
	const drive = row({ title: 'Google Drive', username: 'drive@example.com' });
	const tree = group({
		name: 'Root',
		entries: [gmail, drive],
		sections: [group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever' })]
	});
	return { gmail, drive, tree };
}

/** The entry Rust reads for a row, with the title given. */
function readOf(from: EntryRow, title = from.title ?? '') {
	return entry({
		id: from.id,
		group: from.group,
		fields: [
			field({ name: 'Title', kind: 'title', value: title, empty: false }),
			field({ name: 'UserName', kind: 'username', value: from.username, empty: false })
		]
	});
}

/** The entry pane, whichever of its two states it is in. */
const pane = () => host.querySelector<HTMLElement>('section');
const paneReads = () => (pane()?.textContent ?? '').replace(/\s+/g, ' ').trim();
/** The title of an entry Rust has read, which is a field. */
const titleField = () => host.querySelector<HTMLInputElement>('section h1 input');

/**
 * An entry opened in the bin is drawn with the bin's card above its login,
 * and the row already says it is in the bin. So the card stands there while the
 * entry is read, with its buttons drawn and not yet pressable, and the login
 * does not drop by the card's height when the entry arrives.
 */
it('keeps the place of the bin’s card while an entry in the bin is read', async () => {
	const vault = binnedVault();
	const reading = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.entry.mockReturnValueOnce(reading.promise);

	const { component } = mounted(vault.tree);
	try {
		pressed('Recycle Bin');
		flushSync();
		pressed('Old mail');
		flushSync();

		expect(paneReads()).toContain('Opening…');
		const waiting = entryCard();
		expect(waiting?.textContent).toContain(
			'In the Recycle Bin since 28 Aug · was in “\u2068Work\u2069”'
		);
		const login = [...(pane()?.querySelectorAll('span') ?? [])].find(
			(each) => each.textContent === 'Login'
		);
		expect(
			waiting && login && waiting.compareDocumentPosition(login),
			'the card is not above the login'
		).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
		const buttons = [...(waiting?.querySelectorAll('button') ?? [])];
		expect(buttons.map((each) => each.textContent?.trim())).toEqual([
			'Put back',
			'Delete forever…'
		]);
		expect(buttons.every((each) => each.disabled)).toBe(true);
		buttons[0].click();
		buttons[1].click();
		await settled();
		expect(ipc.putBackEntries).not.toHaveBeenCalled();
		expect(host.querySelector('[data-confirm]')).toBeNull();

		reading.resolve(
			titled(vault.mail.id, 'Old mail', { binned: vault.mail.binned, deletion: 'forever' })
		);
		await settled();
		expect(paneReads()).not.toContain('Opening…');
		expect(entryCard()?.textContent).toContain(
			'In the Recycle Bin since 28 Aug · was in “\u2068Work\u2069”'
		);
		expect([...(entryCard()?.querySelectorAll('button') ?? [])].some((each) => each.disabled)).toBe(
			false
		);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** An entry with no title and no login says so in the same words before and
 * after Rust has read it. */
it('says Untitled and No login in the same words while the entry is read', async () => {
	const bare = row({ title: '', username: '' });
	const tree = group({ name: 'Root', entries: [bare] });
	const reading = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.entry.mockReturnValueOnce(reading.promise);

	const { component } = mounted(tree);
	try {
		// The row has no words of its own to find it by.
		[...host.querySelectorAll('button')]
			.find((each) => each.className.includes('grid-cols-'))
			?.click();
		flushSync();
		expect(paneReads()).toContain('Opening…');
		const before = pane()?.querySelector('h1')?.textContent?.trim();
		expect(paneReads()).toContain('No login');

		reading.resolve(
			entry({
				id: bare.id,
				group: tree.id,
				fields: [
					field({ name: 'Title', kind: 'title', value: '', empty: true }),
					field({ name: 'UserName', kind: 'username', value: '', empty: true })
				]
			})
		);
		await settled();
		expect(paneReads()).not.toContain('Opening…');
		expect(titleField()?.placeholder).toBe(before);
		expect(pane()?.querySelector<HTMLInputElement>('[aria-label="Login"]')?.placeholder).toBe(
			'No login'
		);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('opens the chosen entry at once while the save before it holds Rust', async () => {
	const { gmail, drive, tree } = twoLogins();
	for (const each of [gmail, drive]) each.group = tree.id;
	const reading = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.entry.mockImplementation((id: string) =>
		id === drive.id ? reading.promise : Promise.resolve(readOf(gmail))
	);
	ipc.setField.mockResolvedValue(readOf(gmail, 'Gmail, personal'));

	const { component } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();
		const title = titleField();
		if (!title) throw new Error('Gmail never opened');

		const saving = Promise.withResolvers<void>();
		ipc.save.mockReturnValueOnce(saving.promise);
		write(title, 'Gmail, personal');
		await settled();
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(reads()).toContain('Saving…');

		pressed('Google Drive');
		flushSync();

		// The row is on the screen before Rust has said a word about the entry.
		expect(paneReads()).toContain('Google Drive');
		expect(paneReads()).toContain('drive@example.com');
		expect(paneReads()).toContain('Opening…');
		expect(paneReads()).not.toContain('Gmail');
		expect(pane()?.getAttribute('aria-busy')).toBe('true');
		expect(titleField(), 'something in an unread entry can be written into').toBeNull();
		expect(pane()?.querySelector('input, textarea')).toBeNull();
		expect(paneReads(), 'the last entry’s versions are still on the screen').not.toContain(
			'Versions'
		);

		saving.resolve();
		await settled();
		expect(paneReads()).toContain('Opening…');

		reading.resolve(readOf(drive));
		await settled();
		expect(titleField()?.value).toBe('Google Drive');
		expect(paneReads()).not.toContain('Opening…');
		expect(pane()?.hasAttribute('aria-busy')).toBe(false);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * The race the audit caught: the versions of the entry that was open answered
 * after the next entry had been read, and stayed under its name. They are
 * dropped on arrival, however late they come.
 */
it('never draws the versions of the entry that was left under the next one', async () => {
	const { gmail, drive, tree } = twoLogins();
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(id === gmail.id ? gmail : drive))
	);
	const gmails = Promise.withResolvers<{ entry: string; revision: number; versions: Version[] }>();
	ipc.versions.mockImplementation((id: string) =>
		id === gmail.id
			? gmails.promise
			: listing([version({ index: 0, modified: '2024-06-02T12:00:00Z' })])(id)
	);

	const { component } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();
		expect(ipc.versions).toHaveBeenCalledWith(gmail.id);

		pressed('Google Drive');
		await settled();
		gmails.resolve({
			entry: gmail.id,
			revision: REVISION,
			versions: [
				version({ index: 0, modified: '2021-06-02T12:00:00Z' }),
				version({ index: 1, modified: '2022-06-02T12:00:00Z' })
			]
		});
		await settled();

		expect(titleField()?.value).toBe('Google Drive');
		pressed('Versions');
		flushSync();
		expect(paneReads()).toContain('2024');
		expect(paneReads()).not.toContain('2021');
		expect(paneReads()).not.toContain('2022');
		expect(pane()?.querySelectorAll('[aria-label="Delete this version"]')).toHaveLength(1);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Restore and Delete send the entry on the screen and a position in its own
 * list, never a position from the list of the entry that was left. */
it('restores and deletes from the list of the entry on the screen', async () => {
	const { gmail, drive, tree } = twoLogins();
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(id === gmail.id ? gmail : drive))
	);
	const gmails = Promise.withResolvers<{ entry: string; revision: number; versions: Version[] }>();
	ipc.versions.mockImplementation((id: string) =>
		id === gmail.id
			? gmails.promise
			: listing([version({ index: 0, modified: '2024-06-02T12:00:00Z' })])(id)
	);
	ipc.restoreVersion.mockResolvedValue(readOf(drive));
	ipc.deleteVersion.mockImplementation(listing());

	const { component } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();
		pressed('Google Drive');
		await settled();
		gmails.resolve({
			entry: gmail.id,
			revision: REVISION,
			versions: [0, 1, 2].map((index) =>
				version({ index, modified: `202${index}-06-02T12:00:00Z` })
			)
		});
		await settled();

		pressed('Versions');
		flushSync();
		pressed('Restore', pane() ?? host);
		await settled();
		expect(ipc.restoreVersion).toHaveBeenCalledTimes(1);
		expect(ipc.restoreVersion).toHaveBeenCalledWith(drive.id, { index: 0, revision: REVISION });

		host.querySelector<HTMLButtonElement>('[aria-label="Delete this version"]')?.click();
		flushSync();
		pressed('Drop it');
		await settled();
		expect(ipc.deleteVersion).toHaveBeenCalledTimes(1);
		expect(ipc.deleteVersion).toHaveBeenCalledWith(drive.id, { index: 0, revision: REVISION });
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A to B and back to A before Rust has answered for either: B's answer
 * arrives into a pane that has left it, and is nobody's. */
it('lands only the entry chosen last when the reader goes to and fro', async () => {
	const { gmail, drive, tree } = twoLogins();
	const first = Promise.withResolvers<ReturnType<typeof entry>>();
	const second = Promise.withResolvers<ReturnType<typeof entry>>();
	const again = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.entry
		.mockReturnValueOnce(first.promise)
		.mockReturnValueOnce(second.promise)
		.mockReturnValueOnce(again.promise);

	const { component } = mounted(tree);
	try {
		pressed('Gmail');
		flushSync();
		pressed('Google Drive');
		flushSync();
		pressed('Gmail');
		flushSync();
		expect(ipc.entry.mock.calls).toEqual([[gmail.id], [drive.id], [gmail.id]]);
		expect(paneReads()).toContain('Gmail');
		expect(paneReads()).toContain('Opening…');

		first.resolve(readOf(gmail));
		second.resolve(readOf(drive));
		await settled();
		expect(titleField()?.value).toBe('Gmail');
		expect(paneReads()).not.toContain('Google Drive');

		again.resolve(readOf(gmail));
		await settled();
		expect(titleField()?.value).toBe('Gmail');
		expect(paneReads()).not.toContain('drive@example.com');
		// Nothing was listed for the entry the pane had left.
		expect(ipc.versions).not.toHaveBeenCalledWith(drive.id);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** An entry that is not in the vault any more by the time Rust reads it takes
 * the pane with it, and says why, rather than leaving "Opening…" up for
 * good. */
it('puts the pane away when the entry is gone by the time Rust reads it', async () => {
	const { drive, tree } = twoLogins();
	const reading = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.entry.mockReturnValueOnce(reading.promise);

	const { component } = mounted(tree);
	try {
		pressed('Google Drive');
		flushSync();
		expect(paneReads()).toContain('Opening…');

		reading.reject({ code: 'noSuchEntry', message: 'there is no such entry in this database' });
		await settled();

		expect(pane()).toBeNull();
		expect(toast()?.textContent).toContain('there is no such entry in this database');
		expect(ipc.versions).not.toHaveBeenCalledWith(drive.id);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The folder an entry was opening in went to the bin before Rust read the
 * entry. Its answer arrives after the pane was put away, and does not bring it
 * back. */
it('keeps the pane away when the entry’s folder goes while it is opening', async () => {
	const inside = row({ title: 'VPN' });
	const work = group({ name: 'Work', entries: [inside] });
	const bin = group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever' });
	const tree = group({ name: 'Root', sections: [work, bin] });
	const reading = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.entry.mockReturnValueOnce(reading.promise);
	ipc.deleteGroup.mockResolvedValue(
		group({ ...tree, sections: [{ ...bin, sections: [{ ...work, binned: null }] }] })
	);

	const { component } = mounted(tree);
	try {
		pressed('Work');
		flushSync();
		pressed('VPN');
		flushSync();
		expect(paneReads()).toContain('Opening…');

		host.querySelector<HTMLButtonElement>('[aria-label="Delete this folder"]')?.click();
		flushSync();
		pressed('Move to Recycle Bin', host.querySelector('[data-confirm]') ?? host);
		await settled();
		expect(ipc.deleteGroup).toHaveBeenCalledWith(work.id, 'bin');
		expect(pane()).toBeNull();

		reading.resolve(readOf(inside));
		await settled();
		expect(pane(), 'a late answer opened an entry nobody chose').toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * An edit to the entry that was open reaches Rust after the reader chose the
 * next one. The change is the file's - it is saved and the list is redrawn -
 * but the entry it came back as is not put over the entry now in the pane.
 */
it('saves an edit answered after the reader moved on, and leaves the pane where it is', async () => {
	const { gmail, drive, tree } = twoLogins();
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(id === gmail.id ? gmail : drive))
	);
	const writing = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.setField.mockReturnValueOnce(writing.promise);

	const { component, onTree } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();
		const title = titleField();
		if (!title) throw new Error('Gmail never opened');
		write(title, 'Gmail, personal');
		pressed('Google Drive');
		await settled();
		expect(titleField()?.value).toBe('Google Drive');
		ipc.versions.mockClear();

		writing.resolve(readOf(gmail, 'Gmail, personal'));
		await settled();

		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(onTree).toHaveBeenCalled();
		expect(titleField()?.value).toBe('Google Drive');
		expect(paneReads()).not.toContain('Gmail');
		expect(
			ipc.versions,
			'the versions of an entry nobody is looking at were read'
		).not.toHaveBeenCalledWith(gmail.id);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * A move to the bin answered after the reader opened another entry. The pane
 * stays on the entry they chose, and the move is still offered back: its undo
 * puts the entry back by its id, and opens it only into an empty pane, so the
 * one they are reading stays where it is.
 */
it('offers an entry moved to the bin back once another is open, and leaves that one open', async () => {
	const { gmail, drive, tree } = twoLogins();
	const bin = group({
		name: 'Recycle Bin',
		isRecycleBin: true,
		deletion: 'forever',
		entries: [{ ...gmail, binned: { since: null, within: null, from: tree.id } }]
	});
	const after = group({ ...tree, entries: [drive], sections: [bin] });
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(id === gmail.id ? gmail : drive))
	);
	const moving = Promise.withResolvers<typeof after>();
	ipc.deleteEntries.mockReturnValueOnce(moving.promise);

	const { component } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();
		pressed('Move to Recycle Bin');
		flushSync();
		pressed('Google Drive');
		await settled();

		moving.resolve(after);
		await settled();

		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(titleField()?.value).toBe('Google Drive');
		expect(toast()?.textContent).toContain('Moved “\u2068Gmail\u2069” to the Recycle Bin');
		expect(undo()).not.toBeNull();

		ipc.putBackEntries.mockResolvedValue(tree);
		ipc.entry.mockClear();
		expect(press('z').defaultPrevented).toBe(true);
		await settled();
		expect(ipc.putBackEntries).toHaveBeenCalledWith([gmail.id]);
		expect(ipc.save).toHaveBeenCalledTimes(2);
		expect(ipc.entry, 'the undo took the pane from Google Drive').not.toHaveBeenCalledWith(
			gmail.id
		);
		expect(titleField()?.value).toBe('Google Drive');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * Only a press on the entry already on its way waits. The guard used to hold
 * through the save after a move, so the reader who moved one entry, opened the
 * next and moved that one too in the second the save took saw nothing happen,
 * and the second entry stayed where it was with no word about it.
 */
it('moves another entry to the bin while the save of the first holds Rust', async () => {
	const { gmail, drive, tree } = twoLogins();
	const binned = (rows: EntryRow[]) =>
		group({
			name: 'Recycle Bin',
			isRecycleBin: true,
			deletion: 'forever',
			entries: rows.map((each) => ({
				...each,
				binned: { since: null, within: null, from: tree.id }
			}))
		});
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(id === gmail.id ? gmail : drive))
	);
	ipc.deleteEntries
		.mockResolvedValueOnce(group({ ...tree, entries: [drive], sections: [binned([gmail])] }))
		.mockResolvedValueOnce(group({ ...tree, entries: [], sections: [binned([gmail, drive])] }));
	const saving = Promise.withResolvers<void>();
	ipc.save.mockReturnValueOnce(saving.promise);

	const { component } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();
		pressed('Move to Recycle Bin');
		await settled();
		expect(ipc.save).toHaveBeenCalledTimes(1);

		pressed('Google Drive');
		await settled();
		pressed('Move to Recycle Bin', pane() ?? host);
		await settled();
		expect(
			ipc.deleteEntries,
			'the second move was dropped behind the first save'
		).toHaveBeenCalledTimes(2);
		expect(ipc.deleteEntries).toHaveBeenLastCalledWith(
			[{ entry: drive.id, deletion: 'bin' }],
			expect.any(Number)
		);
		expect(pane()).toBeNull();

		saving.resolve();
		await settled();
		expect(ipc.save).toHaveBeenCalledTimes(2);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * The one press the narrower guard still holds back: the open entry's own
 * move while its folder's move to the bin is on its way. Rust may take the
 * folder first and refuse the entry's move, for a choice the folder's move has
 * already made for it.
 */
it('does not move an entry on its own while its folder is on its way to the bin', async () => {
	const bank = row({ title: 'Bank' });
	const work = group({ name: 'Work', entries: [bank] });
	bank.group = work.id;
	const bin = group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever' });
	const tree = group({ name: 'Root', sections: [work, bin] });
	const gone = group({
		...tree,
		sections: [
			{
				...bin,
				sections: [
					{
						...work,
						binned: { since: null, within: null, from: tree.id },
						deletion: 'forever'
					}
				]
			}
		]
	});
	ipc.entry.mockResolvedValue(titled(bank.id, 'Bank', { group: work.id }));
	const moving = Promise.withResolvers<typeof gone>();
	ipc.deleteGroup.mockReturnValueOnce(moving.promise);

	const { component } = mounted(tree);
	try {
		pressed('Work');
		flushSync();
		pressed('Bank');
		await settled();
		host.querySelector<HTMLButtonElement>('[aria-label="Delete this folder"]')?.click();
		flushSync();
		pressed('Move to Recycle Bin', host.querySelector('[data-confirm]') ?? host);
		flushSync();
		expect(ipc.deleteGroup).toHaveBeenCalledWith(work.id, 'bin');

		pressed('Move to Recycle Bin', pane() ?? host);
		await settled();
		expect(ipc.deleteEntries, 'the entry went after its folder').not.toHaveBeenCalled();

		moving.resolve(gone);
		await settled();
		expect(pane()).toBeNull();
		expect(toast()?.textContent).toContain('Moved “\u2068Work\u2069” to the Recycle Bin');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * Rust refuses a deletion that would no longer do what the window showed: a
 * folder around the entry reached it first, behind a save, and the move the
 * reader agreed to would now erase. Nothing was deleted, nothing is written or
 * offered back, and the reader is told so plainly. The tree and the pane are
 * read again, so the button says what deleting the entry does now.
 */
it('says nothing was deleted when Rust refuses a deletion that changed, and reads again', async () => {
	const bank = row({ title: 'Bank' });
	const work = group({ name: 'Work', entries: [bank] });
	bank.group = work.id;
	const bin = group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever' });
	const tree = group({ name: 'Root', sections: [work, bin] });
	const now = group({
		...tree,
		sections: [
			{
				...bin,
				sections: [
					{ ...work, binned: { since: null, within: null, from: tree.id }, deletion: 'forever' }
				]
			}
		]
	});
	ipc.entry
		.mockResolvedValueOnce(titled(bank.id, 'Bank', { group: work.id, deletion: 'bin' }))
		.mockResolvedValue(titled(bank.id, 'Bank', { group: work.id, deletion: 'forever' }));
	ipc.deleteEntries.mockRejectedValueOnce({
		code: 'deletionChanged',
		message: 'that deletion would no longer do what was shown, so nothing was deleted'
	});

	const { component, onTree } = mounted(tree);
	try {
		pressed('Bank');
		await settled();
		ipc.tree.mockResolvedValue(now);
		pressed('Move to Recycle Bin', pane() ?? host);
		await settled();

		expect(ipc.deleteEntries).toHaveBeenCalledWith(
			[{ entry: bank.id, deletion: 'bin' }],
			expect.any(Number)
		);
		expect(toast()?.textContent).toContain('Choose again from the vault as it is now');
		expect(undo(), 'a deletion that did not happen was offered back').toBeNull();
		expect(ipc.save, 'a refusal changed nothing to write').not.toHaveBeenCalled();
		expect(onTree).toHaveBeenLastCalledWith(now);
		expect(pane()?.textContent).toContain('Delete forever…');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The same refusal for a folder, asked from the question over the folders. */
it('says nothing was deleted when Rust refuses a folder deletion that changed', async () => {
	const work = group({ name: 'Work' });
	const tree = group({ name: 'Root', sections: [work] });
	const now = group({ ...tree, sections: [{ ...work, deletion: 'forever' }] });
	ipc.deleteGroup.mockRejectedValueOnce({
		code: 'deletionChanged',
		message: 'that deletion would no longer do what was shown, so nothing was deleted'
	});

	const { component, onTree } = mounted(tree);
	try {
		pressed('Work');
		flushSync();
		ipc.tree.mockResolvedValue(now);
		host.querySelector<HTMLButtonElement>('[aria-label="Delete this folder"]')?.click();
		flushSync();
		pressed('Move to Recycle Bin', host.querySelector('[data-confirm]') ?? host);
		await settled();

		expect(ipc.deleteGroup).toHaveBeenCalledWith(work.id, 'bin');
		expect(toast()?.textContent).toContain('Choose again from the vault as it is now');
		expect(ipc.save).not.toHaveBeenCalled();
		expect(onTree).toHaveBeenLastCalledWith(now);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * An entry put back is read again before the save rather than after it. The
 * pane used to go on offering Put back and Delete forever on an entry that had
 * already left the bin for the whole second the save took, and a press there
 * was a refusal or a move nobody asked for.
 */
it('stops offering Put back on an entry as soon as Rust has put it back', async () => {
	const vault = binnedVault();
	ipc.entry.mockResolvedValue(
		titled(vault.mail.id, 'Old mail', { binned: vault.mail.binned, deletion: 'forever' })
	);
	ipc.putBackEntries.mockResolvedValue(vault.tree);
	const saving = Promise.withResolvers<void>();
	ipc.save.mockReturnValueOnce(saving.promise);

	const { component } = mounted(vault.tree);
	try {
		pressed('Recycle Bin');
		flushSync();
		pressed('Old mail');
		await settled();
		expect(entryCard()).not.toBeNull();

		ipc.entry.mockResolvedValue(titled(vault.mail.id, 'Old mail', { group: vault.work.id }));
		pressed('Put back', entryCard() ?? host);
		await settled();
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(entryCard(), 'the save is still running and the pane offers Put back').toBeNull();

		saving.resolve();
		await settled();
		expect(ipc.putBackEntries).toHaveBeenCalledTimes(1);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** An undo pressed with the pane empty, and an entry chosen during the second
 * its save takes: the entry that came back is put back, and not opened over the
 * one the reader chose. */
it('puts an entry back without taking the pane from the one chosen meanwhile', async () => {
	const { gmail, drive, tree } = twoLogins();
	const bin = group({
		name: 'Recycle Bin',
		isRecycleBin: true,
		deletion: 'forever',
		entries: [{ ...gmail, binned: { since: null, within: null, from: tree.id } }]
	});
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(id === gmail.id ? gmail : drive))
	);
	ipc.deleteEntries.mockResolvedValue(group({ ...tree, entries: [drive], sections: [bin] }));
	ipc.putBackEntries.mockResolvedValue(tree);

	const { component } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();
		pressed('Move to Recycle Bin');
		await settled();
		expect(pane()).toBeNull();
		expect(undo()).not.toBeNull();

		const saving = Promise.withResolvers<void>();
		ipc.save.mockReturnValueOnce(saving.promise);
		undo()?.click();
		await settled();
		expect(ipc.putBackEntries).toHaveBeenCalledWith([gmail.id]);

		pressed('Google Drive');
		await settled();
		ipc.entry.mockClear();
		saving.resolve();
		await settled();

		expect(titleField()?.value).toBe('Google Drive');
		expect(ipc.entry).not.toHaveBeenCalledWith(gmail.id);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A new entry made while the reader chose another: the new one is in the list
 * and in the file, and the pane stays with the later choice. */
it('leaves the pane on an entry chosen while a new one was being made', async () => {
	const { gmail, drive, tree } = twoLogins();
	const made = row({ title: '', username: '' });
	const grown = group({ ...tree, entries: [gmail, drive, made] });
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(id === gmail.id ? gmail : drive))
	);
	const making = Promise.withResolvers<{ tree: typeof grown; entry: string }>();
	ipc.createEntry.mockReturnValueOnce(making.promise);

	const { component, onTree } = mounted(tree);
	try {
		pressed('Entry');
		flushSync();
		pressed('Google Drive');
		await settled();

		making.resolve({ tree: grown, entry: made.id });
		await settled();

		expect(onTree).toHaveBeenCalledWith(grown);
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(titleField()?.value).toBe('Google Drive');
		expect(ipc.entry).not.toHaveBeenCalledWith(made.id);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A field removal is offered back once its save is done, and a reader may
 * have opened another entry by then. It is offered all the same: the undo is
 * about node-3 by its id, and it leaves the pane on the entry they chose. */
it('offers a removed field back after its save even when another entry is open', async () => {
	vi.useFakeTimers();
	try {
		ipc.entry.mockImplementation((id: string) => Promise.resolve(withPin(id)));
		ipc.removeField.mockReset();
		ipc.removeField.mockResolvedValue(
			entry({
				id: kept.id,
				group: root.id,
				fields: withPin(kept.id).fields.filter((each) => each.name !== 'PIN')
			})
		);
		ipc.undoRemoval.mockReset();
		ipc.undoRemoval.mockResolvedValue(withPin(kept.id));
		const saving = Promise.withResolvers<void>();
		ipc.save.mockReturnValueOnce(saving.promise);

		const component = open();
		flushSync();
		await openKept();
		host.querySelector<HTMLButtonElement>('[aria-label="Remove the field PIN"]')?.click();
		await settled();

		[...host.querySelectorAll('button')]
			.find((each) => each.textContent?.includes('Postgres'))
			?.click();
		await settled();
		saving.resolve();
		await settled();

		expect(toast()?.textContent).toContain('Field “\u2068PIN\u2069” removed');
		ipc.entry.mockClear();
		undo()?.click();
		await settled();
		expect(ipc.undoRemoval).toHaveBeenCalledWith(kept.id, 'PIN');
		expect(ipc.entry, 'the undo took the pane from Postgres').not.toHaveBeenCalledWith(kept.id);

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** Buttons in the entry pane whose words are `label` and nothing else. */
function inPane(label: string): HTMLButtonElement[] {
	return [...(pane()?.querySelectorAll('button') ?? [])].filter(
		(each) => each.textContent?.trim() === label
	);
}

const trashes = () =>
	pane()?.querySelectorAll<HTMLButtonElement>('[aria-label="Delete this version"]') ?? [];

/** What the Versions heading says: the word, and the count once there is a list. */
const heading = () => inPaneStarting('Versions')?.textContent?.replace(/\s+/g, ' ').trim() ?? '';

function inPaneStarting(words: string): HTMLButtonElement | undefined {
	return [...(pane()?.querySelectorAll('button') ?? [])].find((each) =>
		each.textContent?.trim().startsWith(words)
	);
}

/** node-3 read, its three versions listed, and its Versions block open. */
async function openVersions() {
	ipc.entry.mockImplementation((id: string) => Promise.resolve(titled(id, 'node-3')));
	ipc.versions.mockImplementation(
		listing([0, 1, 2].map((index) => version({ index, modified: `202${index}-06-02T12:00:00Z` })))
	);
	ipc.setField.mockResolvedValue(titled(kept.id, 'node-4'));
	pressed('node-3');
	await settled();
	inPaneStarting('Versions')?.click();
	flushSync();
	expect(heading()).toBe('Versions 3');
	expect(trashes()).toHaveLength(3);
}

/**
 * The blocker the review caught. An edit lands and is saved a second later,
 * the save prunes the oldest version, and every position in the list from
 * before it moves down by one. That list stayed drawn through the save, and
 * its trash dropped the neighbour of the version on its row, for good. Now the
 * list goes with the edit - rows, count and all - and comes back when the list
 * read after the save does.
 */
it('draws no version to act on between an edit and the list read after its save', async () => {
	const { component } = mounted(root);
	try {
		await openVersions();
		const saving = Promise.withResolvers<void>();
		ipc.save.mockReturnValueOnce(saving.promise);
		ipc.versions.mockClear();

		write(titleField() as HTMLInputElement, 'node-4');
		await settled();
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(trashes()).toHaveLength(0);
		expect(inPane('Restore')).toHaveLength(0);
		expect(inPane('View')).toHaveLength(0);
		expect(heading()).toBe('Versions');
		expect(ipc.versions).not.toHaveBeenCalled();

		saving.resolve();
		await settled();
		expect(ipc.versions).toHaveBeenCalledWith(kept.id);
		expect(
			ipc.save.mock.invocationCallOrder[0],
			'the list was read before the save that moves it'
		).toBeLessThan(ipc.versions.mock.invocationCallOrder[0]);
		expect(heading()).toBe('Versions 3');
		expect(trashes()).toHaveLength(3);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * A move is a change like any other to the positions a list of versions was
 * read at. The list of the entry that moved goes with the move and comes back
 * with the list read after its save, so nothing on it can be pressed and
 * refused as though the versions had changed while the reader was choosing.
 */
it('draws no version to act on while a move of the open entry is saved', async () => {
	ipc.moveEntries.mockResolvedValue({ tree: root, moved: [{ entry: kept.id, from: root.id }] });
	const { component } = mounted(root);
	try {
		await openVersions();
		const saving = Promise.withResolvers<void>();
		ipc.save.mockReturnValueOnce(saving.promise);
		ipc.versions.mockClear();

		await moveOpenTo('Work');
		expect(ipc.moveEntries).toHaveBeenCalledTimes(1);
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(trashes()).toHaveLength(0);
		expect(inPane('Restore')).toHaveLength(0);
		expect(heading()).toBe('Versions');

		saving.resolve();
		await settled();
		expect(ipc.versions).toHaveBeenCalledWith(kept.id);
		expect(heading()).toBe('Versions 3');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * Two edits whose saves overlap. Commands run side by side in Rust and take
 * the vault in no fixed order, so the list read after the first save can be
 * read after the second edit and answer after it landed. That list is already
 * out of date, and drawing it would offer for the whole second save a press
 * Rust can only refuse. It is dropped, and the list read after the second
 * save is the one drawn.
 */
it('drops a list asked for before a later edit landed', async () => {
	const { component } = mounted(root);
	try {
		await openVersions();
		const first = Promise.withResolvers<void>();
		const second = Promise.withResolvers<void>();
		ipc.save.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
		const stale = Promise.withResolvers<{ entry: string; revision: number; versions: Version[] }>();
		ipc.versions.mockReturnValueOnce(stale.promise);

		write(titleField() as HTMLInputElement, 'node-4');
		await settled();
		first.resolve();
		await settled();
		expect(ipc.versions).toHaveBeenCalledTimes(2);

		ipc.setField.mockResolvedValue(titled(kept.id, 'node-5'));
		write(titleField() as HTMLInputElement, 'node-5');
		await settled();
		expect(ipc.save).toHaveBeenCalledTimes(2);

		stale.resolve({
			entry: kept.id,
			revision: REVISION,
			versions: [0, 1, 2].map((index) => version({ index }))
		});
		await settled();
		expect(trashes(), 'a list read before the second edit').toHaveLength(0);
		expect(heading()).toBe('Versions');

		ipc.versions.mockImplementation(
			listing(
				[0, 1].map((index) => version({ index })),
				3
			)
		);
		second.resolve();
		await settled();
		expect(heading()).toBe('Versions 2');
		expect(trashes()).toHaveLength(2);
	} finally {
		await unmount(component);
	}
});

/** A question about dropping a version asks about a position, and an edit that
 * lands under it is about to move every position. The question goes, and
 * nothing is dropped. */
it('takes a question about a version away when an edit lands, and drops nothing', async () => {
	const { component } = mounted(root);
	try {
		await openVersions();
		trashes()[0]?.click();
		flushSync();
		expect(paneReads()).toContain('Drop this version?');

		const saving = Promise.withResolvers<void>();
		ipc.save.mockReturnValueOnce(saving.promise);
		write(titleField() as HTMLInputElement, 'node-4');
		await settled();
		expect(pane()?.querySelector('[data-confirm]')).toBeNull();

		saving.resolve();
		await settled();
		expect(pane()?.querySelector('[data-confirm]')).toBeNull();
		expect(ipc.deleteVersion).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Restore pressed twice while its save holds Rust used to restore twice: the
 * second press reached Rust after the save and put another version over the
 * one chosen. */
it('restores once when Restore is pressed again behind its save', async () => {
	const { component } = mounted(root);
	try {
		await openVersions();
		ipc.restoreVersion.mockResolvedValue(titled(kept.id, 'node-3'));
		const saving = Promise.withResolvers<void>();
		ipc.save.mockReturnValueOnce(saving.promise);

		const restores = inPane('Restore');
		restores[0]?.click();
		restores[0]?.click();
		restores[1]?.click();
		await settled();
		expect(inPane('Restore'), 'the list from before the restore is drawn').toHaveLength(0);

		saving.resolve();
		await settled();
		expect(inPane('Restore')).toHaveLength(3);
		expect(ipc.restoreVersion).toHaveBeenCalledTimes(1);
		expect(ipc.restoreVersion).toHaveBeenCalledWith(kept.id, { index: 2, revision: REVISION });
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A version being read is read at a position, and an edit is about to move
 * it. It closes when the edit lands, so no copy can take the value of the
 * version that moved into its place. */
it('closes a version being read when an edit lands, before anything is copied from it', async () => {
	const { component } = mounted(root);
	try {
		ipc.version.mockResolvedValue(
			entry({
				fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
			})
		);
		await openVersions();
		inPane('View')[0]?.click();
		await settled();
		const copyOld = () => pane()?.querySelector('[aria-label="Copy Password as it was"]');
		expect(copyOld()).not.toBeNull();

		ipc.save.mockReturnValueOnce(Promise.withResolvers<void>().promise);
		write(titleField() as HTMLInputElement, 'node-4');
		await settled();
		expect(copyOld()).toBeNull();
		expect(ipc.copyVersion).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * A press on a list the vault has changed since reaches Rust with a revision
 * that is not the vault's, and Rust refuses it and does nothing. The reader is
 * told in a sentence, the list is read again, and the next press carries the
 * revision of the list they can see.
 */
it('reads the versions again and says why when Rust finds the list out of date', async () => {
	const { component } = mounted(root);
	try {
		await openVersions();
		ipc.deleteVersion.mockRejectedValueOnce({
			code: 'versionsChanged',
			message: 'the versions changed after they were listed'
		});
		ipc.versions.mockImplementation(
			listing([version({ index: 0, modified: '2025-06-02T12:00:00Z' })], REVISION + 1)
		);

		trashes()[0]?.click();
		flushSync();
		pressed('Drop it', pane() ?? host);
		await settled();
		expect(ipc.deleteVersion).toHaveBeenCalledTimes(1);
		expect(toast()?.textContent).toContain(
			'The versions changed while you were choosing, so nothing was done.'
		);
		expect(ipc.save, 'a refusal is not a change').not.toHaveBeenCalled();
		expect(trashes()).toHaveLength(1);
		expect(paneReads()).toContain('2025');

		ipc.deleteVersion.mockImplementation(listing());
		trashes()[0]?.click();
		flushSync();
		pressed('Drop it', pane() ?? host);
		await settled();
		expect(ipc.deleteVersion).toHaveBeenLastCalledWith(kept.id, {
			index: 0,
			revision: REVISION + 1
		});
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Emptying the bin takes the entry in the pane with it when that entry was in
 * the bin. */
it('puts away an entry that went with the bin', async () => {
	const { component } = mounted(root);
	try {
		ipc.entry.mockImplementation((id: string) => Promise.resolve(titled(id, 'thrown away')));
		ipc.emptyRecycleBin.mockResolvedValue(
			group({ ...root, sections: [root.sections[0], { ...root.sections[1], entries: [] }] })
		);
		pressed('Recycle Bin');
		flushSync();
		pressed('thrown away');
		await settled();
		expect(pane()).not.toBeNull();

		pressed('Empty the bin');
		flushSync();
		pressed('Empty it');
		await settled();
		expect(pane()).toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Rust may answer a second later, behind a save, and an entry opened from
 * outside the bin in that second was never in it. It stays in the pane, whether
 * the bin emptied or only some of it did. */
it('leaves an entry opened meanwhile in the pane when the bin is emptied', async () => {
	for (const refused of [false, true]) {
		const { component } = mounted(root);
		try {
			const emptied = group({
				...root,
				sections: [root.sections[0], { ...root.sections[1], entries: [] }]
			});
			ipc.entry.mockImplementation((id: string) => Promise.resolve(titled(id, 'node-3')));
			ipc.tree.mockResolvedValue(emptied);
			const emptying = Promise.withResolvers<typeof emptied>();
			ipc.emptyRecycleBin.mockReturnValueOnce(emptying.promise);

			pressed('Recycle Bin');
			flushSync();
			pressed('Empty the bin');
			flushSync();
			pressed('Empty it');
			flushSync();
			pressed('All entries');
			flushSync();
			pressed('node-3');
			await settled();
			expect(titleField()?.value).toBe('node-3');

			if (refused) {
				emptying.reject({
					code: 'attachmentInHistory',
					message: 'earlier versions of an entry still hold that file in place'
				});
			} else {
				emptying.resolve(emptied);
			}
			await settled();
			expect(titleField()?.value, `refused: ${refused}`).toBe('node-3');
			expect(ipc.save).toHaveBeenCalledTimes(1);
		} finally {
			await unmount(component);
			vi.useRealTimers();
			ipc.save.mockClear();
		}
	}
});

/** Two logins whose entries have a password, which is what a Change is for. */
function withPasswords() {
	const logins = twoLogins();
	const read = (id: string) => {
		const from = id === logins.gmail.id ? logins.gmail : logins.drive;
		const title = readOf(from);
		return entry({
			...title,
			fields: [
				...title.fields,
				field({ name: 'Password', kind: 'password', value: null, empty: false, protected: true })
			]
		});
	};
	ipc.entry.mockImplementation((id: string) => Promise.resolve(read(id)));
	return { ...logins, read };
}

/** The field a new password is typed into, with something typed in it. */
function typeNewPassword(text: string): HTMLTextAreaElement {
	pressed('Change', pane() ?? host);
	flushSync();
	const typed = host.querySelector<HTMLTextAreaElement>('textarea[aria-label="New password"]');
	if (!typed) throw new Error('Change opened no field');
	typed.value = text;
	typed.dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
	return typed;
}

/** A button anywhere in the window whose words are exactly these. */
function exactly(label: string): HTMLButtonElement {
	const found = [...host.querySelectorAll('button')].find(
		(each) => each.textContent?.trim() === label
	);
	if (!found) throw new Error(`no button that says ${label}`);
	return found;
}

/**
 * The reader changes a site's password, pastes the new one into Change and,
 * by the habit every other field taught them, clicks the next entry. The pane
 * used to switch, the new password went with it, and a notice said it had not
 * been saved. Now nothing that would take the pane away does: the pane stays,
 * the field asks its question with the focus on Save, and nothing typed is let
 * go of in Rust either.
 */
it('keeps the pane and asks when a new password typed there would go with it', async () => {
	const { drive, tree } = withPasswords();
	ipc.createEntry.mockReset();
	ipc.deleteEntries.mockReset();
	const { component, onSettings } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();
		const typed = typeNewPassword('n3w-from-the-website');
		await settled();

		const attempts: [string, () => void][] = [
			['another row', () => pressed('Google Drive')],
			['Escape', () => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))],
			[
				'Close',
				() => host.querySelector<HTMLButtonElement>('[aria-label="Close this entry"]')?.click()
			],
			['a folder', () => pressed('All entries')],
			['+ Entry', () => exactly('Entry').click()],
			[
				'a kind from + Entry',
				() => {
					named('Other kinds of entry').click();
					flushSync();
					menuItem('Bank card').click();
				}
			],
			['Duplicate', () => exactly('Duplicate').click()],
			['Duplicate from the menu', () => run('duplicate')],
			['the bin', () => pressed('Move to Recycle Bin', pane() ?? host)],
			[
				'the settings',
				() => host.querySelector<HTMLButtonElement>('[aria-label="Settings"]')?.click()
			],
			['New Entry from the menu', () => run('newEntry')],
			['Settings… from the menu', () => run('settings')],
			['Move to Recycle Bin from the menu', () => run('moveToBin')]
		];
		for (const [what, attempt] of attempts) {
			attempt();
			await settled();
			expect(titleField()?.value, `${what} took the pane`).toBe('Gmail');
			expect(typed.value, `${what} threw the new password away`).toBe('n3w-from-the-website');
			expect(reads(), `${what} did not ask`).toContain('Save the new password?');
			expect(document.activeElement, `${what}: the question has not got the focus`).toBe(
				exactly('Save')
			);
		}
		expect(ipc.entry).not.toHaveBeenCalledWith(drive.id);
		expect(ipc.createEntry).not.toHaveBeenCalled();
		expect(ipc.duplicateEntry).not.toHaveBeenCalled();
		expect(ipc.deleteEntries).not.toHaveBeenCalled();
		expect(onSettings).not.toHaveBeenCalled();
		expect(ipc.setField).not.toHaveBeenCalled();
		expect(
			ipc.draft.mock.calls.map((call) => call[2]),
			'the new password was let go of in Rust'
		).not.toContain(null);

		exactly('Discard').click();
		await settled();
		pressed('Google Drive');
		await settled();
		expect(titleField()?.value).toBe('Google Drive');
		expect(ipc.entry).toHaveBeenCalledWith(drive.id);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * Escape pressed again because the pane did not close the first time. The
 * first one raised the question with the focus on Save, so the second lands
 * on the question - and its way out used to be Discard, which threw the new
 * password away with nothing said. The way out is back into the field now:
 * the question goes, the text stays, and nothing is let go of in Rust.
 */
it('takes a second Escape back into the field, and throws nothing away', async () => {
	const { tree } = withPasswords();
	const { component } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();
		const typed = typeNewPassword('n3w-from-the-website');
		await vi.advanceTimersByTimeAsync(1_000);
		expect(ipc.draft, 'the premise is a new password Rust has heard').toHaveBeenCalled();

		window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
		await settled();
		expect(document.activeElement).toBe(exactly('Save'));

		document.activeElement?.dispatchEvent(
			new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
		);
		await settled();

		expect(titleField()?.value, 'the second Escape took the pane').toBe('Gmail');
		expect(reads()).not.toContain('Save the new password?');
		expect(document.activeElement, 'the focus is not back in the field').toBe(typed);
		expect(typed.value, 'the second Escape threw the new password away').toBe(
			'n3w-from-the-website'
		);
		expect(
			ipc.draft.mock.calls.map((call) => call[2]),
			'the new password was let go of in Rust'
		).not.toContain(null);
		expect(ipc.setField).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Discard is an answer of its own, and the one that throws the new value
 * away. */
it('throws a new password away only when Discard is pressed', async () => {
	const { tree } = withPasswords();
	const { component } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();
		typeNewPassword('n3w-from-the-website');
		await vi.advanceTimersByTimeAsync(1_000);
		window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
		await settled();

		exactly('Discard').click();
		await settled();
		expect(host.querySelector('textarea[aria-label="New password"]')).toBeNull();
		expect(ipc.draft.mock.calls.map((call) => call[2])).toContain(null);
		expect(ipc.setField).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A new password on its way to the vault holds the pane until it is in: the
 * press that lands meanwhile does nothing, and asks nothing, since there is
 * nothing left to answer. */
it('keeps the pane while a new password is on its way, and lets it go once it is in', async () => {
	const { gmail, drive, tree, read } = withPasswords();
	const saving = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.setField.mockReset();
	ipc.setField.mockReturnValue(saving.promise);
	const { component } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();
		const typed = typeNewPassword('n3w');
		typed.dispatchEvent(
			new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })
		);
		await settled();

		pressed('Google Drive');
		await settled();
		expect(titleField()?.value).toBe('Gmail');
		expect(reads()).not.toContain('Save the new password?');
		expect(ipc.entry).not.toHaveBeenCalledWith(drive.id);

		saving.resolve(read(gmail.id));
		await settled();
		expect(host.querySelector('textarea[aria-label="New password"]')).toBeNull();
		pressed('Google Drive');
		await settled();
		expect(titleField()?.value).toBe('Google Drive');
	} finally {
		await unmount(component);
		vi.useRealTimers();
		ipc.setField.mockReset();
	}
});

/**
 * A value shown before a change is not what the entry holds after it. The old
 * password stayed on the screen under the label of the new one for the rest
 * of its half minute, and a part of it selected and copied was cut out of the
 * new value in Rust at positions counted on the old text. A new password
 * saved, one made and put in, and a version restored each take every value of
 * the entry off the screen.
 */
it('takes a revealed value off the screen when a new one lands, however it arrives', async () => {
	const { gmail, tree, read } = withPasswords();
	ipc.reveal.mockResolvedValue('the old one');
	ipc.setField.mockReset();
	ipc.setField.mockImplementation((id: string) => Promise.resolve(read(id)));
	ipc.generator.mockResolvedValue(drawing());
	ipc.generatePassword.mockResolvedValue(generated('Made-Password-123'));
	ipc.versions.mockImplementation(listing([version({ index: 0 })]));
	ipc.restoreVersion.mockImplementation((id: string) => Promise.resolve(read(id)));
	const shown = () => pane()?.querySelector('[data-value]')?.textContent ?? '';
	const reveal = async () => {
		pressed('Show', pane() ?? host);
		await settled();
		expect(shown()).toBe('the old one');
	};
	const { component } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();

		await reveal();
		typeNewPassword('the new one');
		exactly('Save').click();
		await settled();
		expect(ipc.setField).toHaveBeenCalledTimes(1);
		expect(shown(), 'the old password stayed up after a new one was saved').toBe('');

		await reveal();
		pressed('Make one', pane() ?? host);
		await settled();
		pressed('Put it in the field', pane() ?? host);
		await settled();
		expect(ipc.setField).toHaveBeenLastCalledWith(
			gmail.id,
			'Password',
			'Made-Password-123',
			true,
			expect.any(Number)
		);
		expect(shown(), 'the old password stayed up after a made one went in').toBe('');

		await reveal();
		pressed('Versions', pane() ?? host);
		flushSync();
		pressed('Restore', pane() ?? host);
		await settled();
		expect(ipc.restoreVersion).toHaveBeenCalledTimes(1);
		expect(shown(), 'the password stayed up after a version was restored').toBe('');
		expect(exactly('Show')).toBeTruthy();
	} finally {
		await unmount(component);
		vi.useRealTimers();
		ipc.setField.mockReset();
	}
});

/** A folder named through an input method is made by the reader's own Return,
 * not by the one that confirms a conversion, and renamed the same way. */
it('makes and renames no folder on the Return that ends a composition', () => {
	ipc.createGroup.mockReset();
	ipc.renameGroup.mockReset();
	const component = open();
	flushSync();
	const compositions = [{ isComposing: true }, { keyCode: 229 }];
	const returned = (into: HTMLElement, over: KeyboardEventInit) => {
		const event = new KeyboardEvent('keydown', {
			key: 'Enter',
			bubbles: true,
			cancelable: true,
			...over
		});
		into.dispatchEvent(event);
		return event;
	};

	host.querySelector<HTMLButtonElement>('[aria-label="New folder"]')?.click();
	flushSync();
	const name = host.querySelector('[aria-label="The name of the new folder"]') as HTMLInputElement;
	name.value = 'Clients';
	for (const composition of compositions) {
		expect(returned(name, composition).defaultPrevented).toBe(false);
	}
	expect(ipc.createGroup).not.toHaveBeenCalled();
	name.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
	flushSync();

	pressed('Work');
	flushSync();
	host.querySelector<HTMLButtonElement>('[aria-label="Rename this folder"]')?.click();
	flushSync();
	const rename = host.querySelector(
		'[aria-label="A new name for this folder"]'
	) as HTMLInputElement;
	rename.value = 'Work, renamed';
	for (const composition of compositions) {
		expect(returned(rename, composition).defaultPrevented).toBe(false);
	}
	expect(ipc.renameGroup).not.toHaveBeenCalled();

	return unmount(component);
});

/** Opens node-3 as Rust reads it, and waits until the pane is on it. */
async function reading(over: Parameters<typeof entry>[0] = {}) {
	ipc.entry.mockResolvedValue(entry({ id: kept.id, group: root.id, ...over }));
	const asked = ipc.entry.mock.calls.length;
	pressed('node-3');
	await vi.waitFor(() => expect(ipc.entry).toHaveBeenCalledTimes(asked + 1));
	await new Promise((resolve) => setTimeout(resolve, 0));
	flushSync();
}

/**
 * The page sees a key before AppKit looks in the menu bar, and a key it
 * answers is gone. So each key an item of the bar owns is left alone here -
 * not answered, not prevented - or it would happen twice, or never reach the
 * menu at all.
 */
it('leaves every key the menu owns to the menu, so none of them happens twice', async () => {
	const component = open();
	flushSync();
	await reading({
		fields: [
			field({ name: 'UserName', kind: 'username', value: 'deploy', empty: false }),
			field({ name: 'Password', kind: 'password', value: null, empty: false })
		]
	});
	const asked = ipc.entry.mock.calls.length;

	for (const [key, shiftKey] of [
		['f', false],
		['b', false],
		['n', false],
		['N', true],
		['n', true],
		['l', false],
		[',', false],
		['o', false],
		['d', false],
		['C', true],
		['c', true],
		['Backspace', false]
	] as const) {
		const pressed = new KeyboardEvent('keydown', {
			key,
			shiftKey,
			metaKey: true,
			bubbles: true,
			cancelable: true
		});
		window.dispatchEvent(pressed);
		flushSync();
		expect(pressed.defaultPrevented, `the page took ${shiftKey ? '⇧' : ''}⌘${key}`).toBe(false);
	}
	await tick();

	expect(ipc.copy).not.toHaveBeenCalled();
	expect(ipc.createEntry).not.toHaveBeenCalled();
	expect(ipc.duplicateEntry).not.toHaveBeenCalled();
	expect(ipc.createGroup).not.toHaveBeenCalled();
	expect(ipc.deleteEntries).not.toHaveBeenCalled();
	expect(ipc.entry).toHaveBeenCalledTimes(asked);
	expect(document.activeElement).not.toBe(search());
	expect(host.querySelector('[aria-label="The name of the new folder"]')).toBeNull();

	return unmount(component);
});

/** New Entry is the + Entry button: made in the folder being shown, and not
 * offered where the button is not drawn. */
it('makes an entry from the menu only where the + Entry button is drawn', async () => {
	ipc.createEntry.mockResolvedValue({ tree: root, entry: kept.id });
	ipc.entry.mockResolvedValue(entry({ id: kept.id, group: root.id }));
	const writable = open();
	flushSync();

	expect(applying()).toContain('newEntry');
	run('newEntry');
	flushSync();
	// "All entries" of a vault with a folder: the list asks where, with the
	// keys in its filter.
	expect(document.activeElement).toBe(picker());
	expect(ipc.createEntry).not.toHaveBeenCalled();
	inPicker('Enter');
	await vi.waitFor(() => expect(ipc.createEntry).toHaveBeenCalledWith(root.id, 'login'));

	pressed('Recycle Bin');
	flushSync();
	expect(applying(), 'the bin was offered a new entry').not.toContain('newEntry');
	expect(applying()).not.toContain('newFolder');
	run('newEntry');
	await tick();
	expect(ipc.createEntry).toHaveBeenCalledTimes(1);
	unmount(writable);

	const shut = open({ readOnly: true });
	flushSync();
	expect(applying(), 'a vault Coffer will not write was offered one').not.toContain('newEntry');
	run('newEntry');
	await tick();
	expect(ipc.createEntry).toHaveBeenCalledTimes(1);

	return unmount(shut);
});

/** The plus in the folders pane opens the name and closes it again. The menu
 * only ever opens it: a second New Folder that threw away a half-typed name
 * would be a key that undoes itself. */
it('opens the name of a new folder from the menu and never closes it again', async () => {
	const component = open();
	flushSync();

	run('newFolder');
	await tick();
	const name = host.querySelector<HTMLInputElement>('[aria-label="The name of the new folder"]');
	expect(name, 'the menu opened no name').not.toBeNull();
	expect(document.activeElement).toBe(name);
	if (name) name.value = 'Clie';

	expect(applying()).not.toContain('newFolder');
	run('newFolder');
	await tick();
	expect(host.querySelector('[aria-label="The name of the new folder"]')).toBe(name);
	expect(name?.value).toBe('Clie');
	expect(ipc.createGroup).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * The item says the Recycle Bin, so it is offered only where that is what
 * happens: not for an entry a deletion would erase, and not for one already
 * in the bin. Pressed twice before Rust answers, it moves the entry once.
 */
it('moves the open entry to the bin from the menu once, and only one that goes there', async () => {
	let answer: (tree: typeof root) => void = () => {};
	ipc.deleteEntries.mockReturnValue(new Promise((resolve) => (answer = resolve)));
	const component = open();
	flushSync();
	await reading({ deletion: 'bin' });

	expect(applying()).toContain('moveToBin');
	run('moveToBin');
	run('moveToBin');
	await tick();
	expect(ipc.deleteEntries).toHaveBeenCalledTimes(1);
	expect(ipc.deleteEntries).toHaveBeenCalledWith(
		[{ entry: kept.id, deletion: 'bin' }],
		expect.any(Number)
	);
	answer(root);
	await vi.waitFor(() => expect(ipc.save).toHaveBeenCalled());
	await new Promise((resolve) => setTimeout(resolve, 0));
	unmount(component);

	for (const over of [
		{ deletion: 'forever' as const },
		{ deletion: 'forever' as const, binned: { since: null, within: null, from: null } }
	]) {
		const again = open();
		flushSync();
		await reading(over);
		expect(applying(), JSON.stringify(over)).not.toContain('moveToBin');
		run('moveToBin');
		await tick();
		expect(ipc.deleteEntries).toHaveBeenCalledTimes(1);
		expect(reads()).not.toContain('Delete forever?');
		unmount(again);
	}
});

/** Cmd+Backspace in a field deletes to the start of the line. The item is
 * greyed out while a field has the focus, so the key is the field's alone. */
it('greys out Move to Recycle Bin while a field of the entry is being written', async () => {
	window.addEventListener('focusin', focused);
	window.addEventListener('focusout', focused);
	const component = open();
	try {
		flushSync();
		await reading({
			fields: [field({ name: 'Notes', kind: 'notes', value: 'a note', empty: false })]
		});
		expect(applying()).toContain('moveToBin');

		const notes = host.querySelector<HTMLTextAreaElement>('textarea[aria-label="Notes"]');
		notes?.focus();
		flushSync();
		expect(applying(), 'offered while the notes were being written').not.toContain('moveToBin');
		run('moveToBin');
		await tick();
		expect(ipc.deleteEntries).not.toHaveBeenCalled();

		notes?.blur();
		flushSync();
		expect(applying()).toContain('moveToBin');
	} finally {
		window.removeEventListener('focusin', focused);
		window.removeEventListener('focusout', focused);
		await unmount(component);
	}
});

/** The conflict dialog asks before anything else happens to the vault, and a
 * key cannot go round it. */
it('answers nothing from the menu while the conflict dialog asks its question', async () => {
	ipc.save.mockRejectedValue({ code: 'externalChange', message: 'the database changed on disk' });
	ipc.rival.mockResolvedValue({ modified: null, entries: 3 });
	ipc.createEntry.mockResolvedValue({ tree: root, entry: kept.id });
	ipc.entry.mockResolvedValue(entry({ id: kept.id, group: root.id }));
	const component = open();
	flushSync();

	run('newEntry');
	flushSync();
	inPicker('Enter');
	await vi.waitFor(() => expect(host.textContent).toContain('The file changed'));
	flushSync();

	for (const command of ['newEntry', 'newFolder', 'find', 'moveToBin'] as const) {
		expect(applying(), command).not.toContain(command);
		run(command);
	}
	await tick();
	expect(ipc.createEntry).toHaveBeenCalledTimes(1);
	expect(host.querySelector('[aria-label="The name of the new folder"]')).toBeNull();
	expect(document.activeElement).not.toBe(search());

	return unmount(component);
});

/** An entry with no password has nothing for Copy Password to copy, and the
 * item is grey rather than a press that says "copied" about nothing. */
it('copies nothing from the menu for an entry with no password, and says nothing', async () => {
	const component = open();
	flushSync();
	await reading({
		fields: [
			field({ name: 'UserName', kind: 'username', value: 'deploy', empty: false }),
			field({ name: 'Password', kind: 'password', value: null, empty: true })
		]
	});

	expect(applying()).toContain('copyLogin');
	expect(applying()).not.toContain('copyPassword');
	run('copyPassword');
	await tick();

	expect(ipc.copy).not.toHaveBeenCalled();
	expect(host.querySelector('[data-notice]')).toBeNull();

	return unmount(component);
});

/** Settings… is the status bar's button, guard and all, and is not offered
 * while the settings are already what is being read. */
it('opens the settings from the menu the way the status bar does', () => {
	const onSettings = vi.fn();
	const closed = open({ onSettings });
	flushSync();

	run('settings');
	expect(onSettings).toHaveBeenCalledTimes(1);
	unmount(closed);

	const shown = open({ onSettings, settings: sheet });
	flushSync();
	expect(applying()).not.toContain('settings');
	run('settings');
	expect(onSettings, 'the menu closed the settings it opens').toHaveBeenCalledTimes(1);

	return unmount(shown);
});

/** In a vault Coffer will not write back, Rust still says a deletion would go
 * to the bin - the bin decides that, not the file - so the screen is what
 * keeps the item grey: the pane draws no deletion there at all. */
it('offers no Move to Recycle Bin in a vault Coffer will not write', async () => {
	const component = open({ readOnly: true });
	flushSync();
	await reading({ deletion: 'bin' });

	expect(applying(), 'offered in a vault that will not be written').not.toContain('moveToBin');
	run('moveToBin');
	await tick();

	expect(ipc.deleteEntries).not.toHaveBeenCalled();
	return unmount(component);
});

/**
 * A press on + Entry takes the focus out of the field being written in, and
 * the field writes what was typed as it goes. New Entry from the menu takes no
 * focus, and the entry it opens takes the field off the screen - which WebKit
 * does without telling the field it lost the focus. So the menu leaves the
 * field first, as the press does, and what was typed is written before the
 * new entry is asked for.
 */
it('writes what is being typed before New Entry from the menu takes the pane', async () => {
	ipc.createEntry.mockResolvedValue({ tree: root, entry: other.id });
	ipc.setField.mockResolvedValue(entry({ id: kept.id, group: root.id }));
	const component = open();
	flushSync();
	await reading();

	const title = host.querySelector<HTMLInputElement>('h1 input');
	title?.focus();
	if (title) title.value = 'node-4';
	title?.dispatchEvent(new Event('input', { bubbles: true }));

	run('newEntry');
	flushSync();
	inPicker('Enter');
	await vi.waitFor(() => expect(ipc.createEntry).toHaveBeenCalledTimes(1));

	expect(ipc.setField).toHaveBeenCalledWith(kept.id, 'Title', 'node-4', false, expect.any(Number));
	expect(
		ipc.setField.mock.invocationCallOrder[0],
		'the new entry was asked for with the title still in the field'
	).toBeLessThan(ipc.createEntry.mock.invocationCallOrder[0]);

	return unmount(component);
});

/**
 * Cmd+B with the focus in the login field. The menu leaves the field, which
 * writes what was typed, and the copy is asked for only once that write has
 * arrived: Tauri answers the two side by side, and once a save lets go of the
 * session it goes to whichever asks first, so a copy sent beside the write put
 * the login as it was before the edit on the pasteboard, under a notice saying
 * the one on the screen was copied. The search field writes nothing when it is
 * left, and a copy from the menu leaves the reader typing in it.
 */
it('copies from the menu the login being typed, once it is written', async () => {
	let written: (value: unknown) => void = () => {};
	ipc.setField.mockReturnValue(new Promise((resolve) => (written = resolve)));
	const component = open();
	flushSync();
	await reading({
		fields: [field({ name: 'UserName', kind: 'username', value: 'alice', empty: false })]
	});

	const login = host.querySelector<HTMLInputElement>('input[aria-label="Login"]');
	login?.focus();
	if (login) login.value = 'bob';
	login?.dispatchEvent(new Event('input', { bubbles: true }));

	expect(applying()).toContain('copyLogin');
	run('copyLogin');
	expect(ipc.setField).toHaveBeenCalledWith(kept.id, 'UserName', 'bob', false, expect.any(Number));
	await new Promise((resolve) => setTimeout(resolve, 0));
	expect(
		ipc.copy,
		'the copy was asked for before the login it copies was written'
	).not.toHaveBeenCalled();

	written(
		entry({
			id: kept.id,
			group: root.id,
			fields: [field({ name: 'UserName', kind: 'username', value: 'bob', empty: false })]
		})
	);
	await vi.waitFor(() => expect(ipc.copy).toHaveBeenCalledWith(kept.id, 'UserName', null));
	expect(ipc.setField.mock.invocationCallOrder[0]).toBeLessThan(
		ipc.copy.mock.invocationCallOrder[0]
	);

	search().focus();
	run('copyLogin');
	expect(document.activeElement, 'the search field lost the focus to a copy').toBe(search());
	await vi.waitFor(() => expect(ipc.copy).toHaveBeenCalledTimes(2));

	return unmount(component);
});

/**
 * A vault whose rows name the folders they are in, the way Rust's do: node-3
 * at the top, Postgres in Work, Clients inside Work, and one thing in the bin.
 * Where a drag may land is read off the folder a row says it is in.
 */
const shelved = (() => {
	const clients = group({ name: 'Clients' });
	const work = group({ name: 'Work', sections: [clients] });
	const bin = group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever' });
	const tree = group({ name: 'Root', sections: [work, bin] });
	const atTop = row({ title: 'node-3', username: 'deploy', group: tree.id });
	const inWork = row({ title: 'Postgres', username: 'svc_app', group: work.id });
	const thrown = row({
		title: 'thrown away',
		group: bin.id,
		binned: { since: null, within: null, from: tree.id }
	});
	tree.entries.push(atTop);
	work.entries.push(inWork);
	bin.entries.push(thrown);
	return { tree, work, clients, bin, atTop, inWork, thrown };
})();
const { tree: vault, work, clients, atTop, inWork } = shelved;

/** What node-3 reads as, in whichever folder Rust now has it. */
function keptIn(folder: string) {
	return entry({
		id: atTop.id,
		group: folder,
		fields: [field({ name: 'Title', kind: 'title', value: 'node-3', empty: false })]
	});
}

/**
 * A window whose tree is the one the last change answered with, the way the
 * page above it hands each one back. A move's undo reads where things are off
 * the tree the window holds, so a window stuck on its first tree would be
 * undoing a move it never saw.
 */
function following(tree: Group, readOnly = false) {
	vi.useFakeTimers();
	ipc.tree.mockResolvedValue(tree);
	const props = reactive({
		database,
		root: tree,
		kinds: kinds(),
		readOnly,
		onSettings: vi.fn(),
		onTree: (next: Group) => {
			props.root = next;
		}
	});
	const component = mount(Vault, { target: host, props });
	flushSync();
	return { component, props };
}

/** The line above the open entry's title, which opens the folder list. */
const folderLine = () =>
	host.querySelector<HTMLButtonElement>('section header button[aria-haspopup="listbox"]');

/** Text typed into the open folder list's filter. */
function typeInPicker(text: string) {
	picker().value = text;
	picker().dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
}

/** Opens the folder list over the open entry and picks the folder named. */
async function moveOpenTo(name: string) {
	folderLine()?.click();
	flushSync();
	typeInPicker(name);
	inPicker('Enter');
	await settled();
}

/** A row of the list, or a line of the folders, pressed and carried with the
 * pointer to `target`, and let go there, with `released` run at the moment it
 * is let go: the click that follows. */
async function drag(
	from: Element | null | undefined,
	target: Element | null,
	released: () => void = () => {}
) {
	if (!from) throw new Error('nothing to drag');
	vi.spyOn(document, 'elementFromPoint').mockImplementation(() => target);
	from.dispatchEvent(
		new PointerEvent('pointerdown', { bubbles: true, clientX: 10, clientY: 10, button: 0 })
	);
	window.dispatchEvent(new PointerEvent('pointermove', { clientX: 20, clientY: 10 }));
	window.dispatchEvent(new PointerEvent('pointermove', { clientX: 40, clientY: 10 }));
	flushSync();
	window.dispatchEvent(new PointerEvent('pointerup', { clientX: 40, clientY: 10 }));
	released();
	flushSync();
	await settled();
}

/** The row of the list with these words, as a button. */
function listRow(title: string): HTMLButtonElement | undefined {
	return [...host.querySelectorAll<HTMLButtonElement>('button')].find(
		(each) => each.textContent?.includes(title) && !each.closest('aside')
	);
}

/** The place in the folders pane a drop under this key lands in. */
const place = (key: string) => host.querySelector<HTMLElement>(`[data-drop="${key}"]`);

it('moves the open entry from the line above its title and offers it back', async () => {
	const moved = [{ entry: atTop.id, from: vault.id }];
	ipc.entry.mockImplementation(() => Promise.resolve(keptIn(vault.id)));
	ipc.moveEntries.mockResolvedValue({ tree: vault, moved });
	ipc.moveEntriesBack.mockResolvedValue(vault);
	const { component } = following(vault);
	try {
		await openKept();
		expect(folderLine()?.textContent).toContain('Top of the vault');

		await moveOpenTo('work');

		expect(ipc.moveEntries).toHaveBeenCalledWith([atTop.id], work.id);
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(toast()?.textContent).toContain('Moved “⁨node-3⁩” to “⁨Work⁩”');
		expect(toast()?.querySelector('use')?.getAttribute('href')).toBe('#i-folder');
		expect(pane(), 'a move took the pane away').not.toBeNull();

		press('z');
		await settled();
		expect(ipc.moveEntriesBack).toHaveBeenCalledWith(moved, work.id);
		expect(ipc.save).toHaveBeenCalledTimes(2);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The pane stays on the entry wherever it goes, and the line above its title
 * names the folder it went to before the save is done, not a second later. */
it('reads the open entry again when it moves, before the save', async () => {
	ipc.entry.mockResolvedValueOnce(keptIn(vault.id)).mockResolvedValue(keptIn(work.id));
	ipc.moveEntries.mockResolvedValue({ tree: vault, moved: [{ entry: atTop.id, from: vault.id }] });
	const { component } = following(vault);
	try {
		await openKept();
		await moveOpenTo('Work');

		expect(ipc.entry).toHaveBeenCalledTimes(2);
		expect(ipc.entry.mock.invocationCallOrder[1]).toBeLessThan(
			ipc.save.mock.invocationCallOrder[0]
		);
		expect(folderLine()?.textContent).toContain('Work');
		expect(folderLine()?.textContent).not.toContain('Top of the vault');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('says why a move was refused and offers nothing', async () => {
	ipc.entry.mockImplementation(() => Promise.resolve(keptIn(vault.id)));
	ipc.moveEntries.mockRejectedValue({
		code: 'refused',
		message: 'what is in the recycle bin is put back, not moved'
	});
	const { component } = following(vault);
	try {
		await openKept();
		await moveOpenTo('Work');

		expect(toast()?.textContent).toContain('what is in the recycle bin is put back, not moved');
		expect(undo(), 'a refusal was offered back').toBeNull();
		expect(ipc.save).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The save's own failure is the one sentence that matters, and an offer over
 * it would push it off the screen. */
it('offers nothing back when the save after a move failed', async () => {
	ipc.entry.mockImplementation(() => Promise.resolve(keptIn(vault.id)));
	ipc.moveEntries.mockResolvedValue({ tree: vault, moved: [{ entry: atTop.id, from: vault.id }] });
	ipc.save.mockRejectedValue({ code: 'io', message: 'the disk is full' });
	const { component } = following(vault);
	try {
		await openKept();
		await moveOpenTo('Work');

		expect(toast()?.textContent).toContain('the disk is full');
		expect(undo()).toBeNull();
		expect(reads()).toContain('Not saved');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A tree on the screen that was behind the vault: Rust found the entry
 * already where it was sent, moved nothing, and there is nothing to write. */
it('saves nothing and offers nothing when Rust finds the entry there already', async () => {
	ipc.entry.mockImplementation(() => Promise.resolve(keptIn(vault.id)));
	ipc.moveEntries.mockResolvedValue({ tree: vault, moved: [] });
	const { component } = following(vault);
	try {
		await openKept();
		const asked = ipc.tree.mock.calls.length;
		await moveOpenTo('Work');

		expect(ipc.moveEntries).toHaveBeenCalledTimes(1);
		expect(ipc.save).not.toHaveBeenCalled();
		expect(undo()).toBeNull();
		expect(ipc.tree.mock.calls.length, 'the tree was not read again').toBeGreaterThan(asked);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** An undo pressed after the file says something moved since is refused by
 * Rust, and the window says so in the words every undo that came too late
 * has. */
it('says a move can no longer be taken back once something has moved since', async () => {
	ipc.entry.mockImplementation(() => Promise.resolve(keptIn(vault.id)));
	ipc.moveEntries.mockResolvedValue({ tree: vault, moved: [{ entry: atTop.id, from: vault.id }] });
	ipc.moveEntriesBack.mockRejectedValue({
		code: 'superseded',
		message: 'something has moved since, so that move can no longer be taken back'
	});
	const { component } = following(vault);
	try {
		await openKept();
		await moveOpenTo('Work');
		undo()?.click();
		await settled();

		expect(toast()?.textContent).toContain(
			'Something has changed since, so that can no longer be undone.'
		);
		expect(ipc.save).toHaveBeenCalledTimes(1);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A second press while the entry is on its way to the bin is a move Rust
 * would refuse for a choice the reader already made. */
it('drops a move of an entry whose deletion is on its way', async () => {
	ipc.entry.mockImplementation(() => Promise.resolve(keptIn(vault.id)));
	ipc.deleteEntries.mockReturnValue(new Promise(() => {}));
	const { component } = following(vault);
	try {
		await openKept();
		pressed('Move to Recycle Bin', pane() ?? host);
		await settled();
		expect(ipc.deleteEntries).toHaveBeenCalledTimes(1);

		await moveOpenTo('Work');
		expect(ipc.moveEntries).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('drops a move into a folder that is on its way to the bin', async () => {
	ipc.entry.mockImplementation(() => Promise.resolve(keptIn(vault.id)));
	ipc.deleteGroup.mockReturnValue(new Promise(() => {}));
	const { component } = following(vault);
	try {
		pressed('Work');
		flushSync();
		host.querySelector<HTMLButtonElement>('[aria-label="Delete this folder"]')?.click();
		flushSync();
		pressed('Move to Recycle Bin');
		await settled();
		expect(ipc.deleteGroup).toHaveBeenCalledTimes(1);

		pressed('All entries');
		flushSync();
		await openKept();
		await moveOpenTo('Work');
		folderLine()?.click();
		flushSync();
		typeInPicker('Clients');
		inPicker('Enter');
		await settled();
		expect(ipc.moveEntries, 'a move into a folder going to the bin').not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('drags a row onto a folder and moves it', async () => {
	ipc.moveEntries.mockResolvedValue({ tree: vault, moved: [{ entry: atTop.id, from: vault.id }] });
	const { component } = following(vault);
	try {
		// WebKit sends the click that follows to whatever holds both ends, and
		// here it is sent to the row the drag began on: it is the drag's all the
		// same, and opens nothing.
		await drag(listRow('node-3'), place(work.id)?.querySelector('span') ?? null, () =>
			listRow('node-3')?.click()
		);

		expect(ipc.moveEntries).toHaveBeenCalledWith([atTop.id], work.id);
		expect(toast()?.textContent).toContain('Moved “⁨node-3⁩” to “⁨Work⁩”');
		expect(ipc.entry, 'the drag opened the row it started on').not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** What the folder under the pointer looks like while a row is carried over
 * it, and that only a place that takes it lights. */
it('lights only the folder a drag would land in, and lets go on Escape', async () => {
	const { component } = following(vault);
	try {
		type('node');
		vi.spyOn(document, 'elementFromPoint').mockImplementation(() => place(work.id));
		listRow('node-3')?.dispatchEvent(
			new PointerEvent('pointerdown', { bubbles: true, clientX: 10, clientY: 10, button: 0 })
		);
		window.dispatchEvent(new PointerEvent('pointermove', { clientX: 40, clientY: 10 }));
		flushSync();

		expect(place(work.id)?.className).toContain('outline-accent');
		expect(place('all')?.className).not.toContain('outline-accent');
		expect(host.querySelector('.cursor-grabbing')).not.toBeNull();

		// node-3 is at the top of the vault already, which "All entries" and
		// "Not in a folder" both stand for: under the pointer, neither lights,
		// and Work goes dark again.
		for (const refusing of ['all', 'loose']) {
			vi.spyOn(document, 'elementFromPoint').mockImplementation(() => place(refusing));
			window.dispatchEvent(new PointerEvent('pointermove', { clientX: 50, clientY: 10 }));
			flushSync();
			expect(place(refusing)?.className, refusing).not.toContain('outline-accent');
			expect(place(work.id)?.className).not.toContain('outline-accent');
			expect(host.querySelector('.cursor-grabbing'), 'the drag was let go').not.toBeNull();
		}
		vi.spyOn(document, 'elementFromPoint').mockImplementation(() => place(work.id));
		window.dispatchEvent(new PointerEvent('pointermove', { clientX: 60, clientY: 10 }));
		flushSync();
		expect(place(work.id)?.className).toContain('outline-accent');

		const escape = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
		search().dispatchEvent(escape);
		flushSync();
		window.dispatchEvent(new PointerEvent('pointerup', { clientX: 40, clientY: 10 }));
		await settled();

		expect(place(work.id)?.className).not.toContain('outline-accent');
		expect(host.querySelector('.cursor-grabbing')).toBeNull();
		expect(ipc.moveEntries).not.toHaveBeenCalled();
		expect(search().value, 'the Escape that let go cleared the search as well').toBe('node');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('a drag onto the folder the entry is already in, or onto the bin, moves nothing', async () => {
	const { component } = following(vault);
	try {
		// node-3 is at the top of the vault, which both rows above the folders
		// stand for.
		await drag(listRow('node-3'), place('all'));
		await drag(listRow('node-3'), place('loose'));
		pressed('Work');
		flushSync();
		await drag(listRow('Postgres'), place(work.id));
		expect(ipc.moveEntries).not.toHaveBeenCalled();

		// The bin is no place to drop on at all: deleting is the way in. A row
		// let go over it lands nowhere, wherever the rule for places goes.
		const bin = [...host.querySelectorAll('aside button')].find((each) =>
			each.textContent?.includes('Recycle Bin')
		);
		expect(bin).toBeDefined();
		expect(bin?.closest('[data-drop]'), 'the bin takes a drop').toBeNull();
		await drag(listRow('Postgres'), bin ?? null);
		expect(ipc.moveEntries).not.toHaveBeenCalled();
		expect(ipc.deleteEntries, 'a drop on the bin deleted something').not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('drags a folder onto All entries to take it to the top of the vault', async () => {
	const lifted = group({ ...work, sections: [] });
	const after = group({ ...vault, sections: [lifted, clients, shelved.bin] });
	ipc.moveGroup.mockResolvedValueOnce(after);
	ipc.moveGroupBack.mockResolvedValueOnce(vault);
	const { component } = following(vault);
	try {
		host.querySelector<HTMLButtonElement>('[aria-label="Expand Work"]')?.click();
		flushSync();
		const line = [...(place(clients.id)?.querySelectorAll('button') ?? [])].at(-1);
		await drag(line, place('all'));

		expect(ipc.moveGroup).toHaveBeenCalledWith(clients.id, vault.id);
		expect(toast()?.textContent).toContain('Moved “⁨Clients⁩” to the top of the vault');
		expect(ipc.save).toHaveBeenCalledTimes(1);

		// Taken back by Rust, which checks the file still says so, from where
		// the move put it to where it came from.
		press('z');
		await settled();
		expect(ipc.moveGroupBack).toHaveBeenCalledWith(clients.id, work.id, vault.id);
		expect(ipc.moveGroup).toHaveBeenCalledTimes(1);
		expect(ipc.save).toHaveBeenCalledTimes(2);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('a drag of a folder onto a folder inside it, or onto its own, moves nothing', async () => {
	const { component } = following(vault);
	try {
		host.querySelector<HTMLButtonElement>('[aria-label="Expand Work"]')?.click();
		flushSync();
		const workLine = [...(place(work.id)?.querySelectorAll('button') ?? [])].at(-1);
		await drag(workLine, place(clients.id));
		await drag(workLine, place(work.id));
		await drag(workLine, place('all'));

		expect(ipc.moveGroup).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Shows the folder with this name and asks for it to go to the bin, where
 * Rust is never heard from again: a deletion on its way. */
async function binning(name: string) {
	ipc.deleteGroup.mockReturnValue(new Promise(() => {}));
	pressed(name);
	flushSync();
	host.querySelector<HTMLButtonElement>('[aria-label="Delete this folder"]')?.click();
	flushSync();
	pressed('Move to Recycle Bin');
	await settled();
}

/** The line of the folders pane that selects this folder: the one a drag of
 * the folder starts on. */
const folderOf = (id: string) => [...(place(id)?.querySelectorAll('button') ?? [])].at(-1);

/** "Move to Recycle Bin" on Work is waiting behind a save, and the reader
 * drags Postgres, in Work, out of it. Sent, the move would reach Rust before
 * the deletion, and Postgres would escape a deletion the reader already chose,
 * or after it, and be refused for being in the bin. */
it('drops a move of an entry out of a folder that is on its way to the bin', async () => {
	ipc.moveEntries.mockResolvedValue({ tree: vault, moved: [{ entry: inWork.id, from: work.id }] });
	const { component } = following(vault);
	try {
		await binning('Work');
		expect(ipc.deleteGroup).toHaveBeenCalledTimes(1);
		pressed('All entries');
		flushSync();

		await drag(listRow('Postgres'), place('all'));
		expect(ipc.moveEntries, 'a move out of a folder going to the bin').not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A folder move reaches Rust only while nothing it is about is on its way:
 * the folder, a folder above it, or the one it goes into. Each would have the
 * move refused, or undo a choice the reader already made, depending on which
 * of the two Rust took first. */
it('drops a folder move while it, a folder above it, or where it goes is on its way', async () => {
	const clientsHere = group({ name: 'Clients' });
	const vendors = group({ name: 'Vendors' });
	const workHere = group({ name: 'Work', sections: [clientsHere, vendors] });
	const personal = group({ name: 'Personal' });
	const archive = group({ name: 'Archive' });
	const spare = group({ name: 'Spare' });
	const bin = group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever' });
	const tree = group({ name: 'Root', sections: [personal, workHere, archive, spare, bin] });
	ipc.moveGroup.mockReturnValue(new Promise(() => {}));
	const { component } = following(tree);
	try {
		host.querySelector<HTMLButtonElement>('[aria-label="Expand Work"]')?.click();
		flushSync();

		await binning('Archive');
		await drag(folderOf(personal.id), place(archive.id));
		expect(ipc.moveGroup, 'a move into a folder going to the bin').not.toHaveBeenCalled();

		await binning('Clients');
		await drag(folderOf(clientsHere.id), place(personal.id));
		expect(ipc.moveGroup, 'a move of a folder going to the bin').not.toHaveBeenCalled();

		await binning('Work');
		await drag(folderOf(vendors.id), place(personal.id));
		expect(ipc.moveGroup, 'a move out of a folder going to the bin').not.toHaveBeenCalled();
		expect(ipc.deleteGroup).toHaveBeenCalledTimes(3);

		// The same drag of a folder nothing is happening to goes.
		await drag(folderOf(spare.id), place(personal.id));
		expect(ipc.moveGroup).toHaveBeenCalledWith(spare.id, personal.id);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('says why a folder move was refused and offers nothing', async () => {
	ipc.moveGroup.mockRejectedValue({
		code: 'refused',
		message: 'the recycle bin takes nothing but what is deleted'
	});
	const { component } = following(vault);
	try {
		host.querySelector<HTMLButtonElement>('[aria-label="Expand Work"]')?.click();
		flushSync();
		await drag(folderOf(clients.id), place('all'));

		expect(ipc.moveGroup).toHaveBeenCalledTimes(1);
		expect(toast()?.textContent).toContain('the recycle bin takes nothing but what is deleted');
		expect(undo(), 'a refusal was offered back').toBeNull();
		expect(ipc.save).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Rust refuses a folder's undo once the file no longer holds the move: the
 * folder moved on since, or a reload brought in the folder it came from in the
 * bin. The window says so in the words it uses for an entry's. */
it('says a folder move can no longer be taken back once the file has moved on', async () => {
	const after = group({
		...vault,
		sections: [group({ ...work, sections: [] }), clients, shelved.bin]
	});
	ipc.moveGroup.mockResolvedValue(after);
	ipc.moveGroupBack.mockRejectedValue({
		code: 'superseded',
		message: 'something has moved since, so that move can no longer be taken back'
	});
	const { component } = following(vault);
	try {
		host.querySelector<HTMLButtonElement>('[aria-label="Expand Work"]')?.click();
		flushSync();
		await drag(folderOf(clients.id), place('all'));
		press('z');
		await settled();

		expect(ipc.moveGroupBack).toHaveBeenCalledTimes(1);
		expect(toast()?.textContent).toContain(
			'Something has changed since, so that can no longer be undone.'
		);
		expect(ipc.save, 'a refused undo was written').toHaveBeenCalledTimes(1);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * The reader moves a folder, then moves it again while that second move waits
 * behind a save, and presses Cmd+Z. Sent, the undo and the second move would
 * reach Rust in no fixed order, and the undo could take back the reader's later
 * choice. It is dropped while anything it is about is on its way, for a folder
 * as for an entry.
 */
it('drops an undo while a later move of the same thing is on its way', async () => {
	const after = group({
		...vault,
		sections: [group({ ...work, sections: [] }), clients, shelved.bin]
	});
	ipc.moveGroup.mockResolvedValueOnce(after).mockReturnValueOnce(new Promise(() => {}));
	const { component } = following(vault);
	try {
		host.querySelector<HTMLButtonElement>('[aria-label="Expand Work"]')?.click();
		flushSync();
		await drag(folderOf(clients.id), place('all'));
		expect(undo()).not.toBeNull();

		await drag(folderOf(clients.id), place(work.id));
		expect(ipc.moveGroup).toHaveBeenLastCalledWith(clients.id, work.id);
		press('z');
		await settled();
		expect(ipc.moveGroupBack, 'the undo raced the later move').not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}

	// The folder it goes back to on its way to the bin is the same race.
	ipc.moveGroup.mockReset();
	ipc.moveGroup.mockResolvedValueOnce(after);
	const binned = following(vault);
	try {
		host.querySelector<HTMLButtonElement>('[aria-label="Expand Work"]')?.click();
		flushSync();
		await drag(folderOf(clients.id), place('all'));
		expect(undo()).not.toBeNull();
		await binning('Work');
		press('z');
		await settled();
		expect(ipc.moveGroupBack, 'the undo raced the deletion').not.toHaveBeenCalled();
	} finally {
		await unmount(binned.component);
		vi.useRealTimers();
	}

	const moved = [{ entry: atTop.id, from: vault.id }];
	ipc.entry.mockImplementation(() => Promise.resolve(keptIn(vault.id)));
	ipc.moveEntries
		.mockResolvedValueOnce({ tree: vault, moved })
		.mockReturnValueOnce(new Promise(() => {}));
	const again = following(vault);
	try {
		await openKept();
		await moveOpenTo('Work');
		expect(undo()).not.toBeNull();
		await moveOpenTo('Clients');
		expect(ipc.moveEntries).toHaveBeenCalledTimes(2);
		press('z');
		await settled();
		expect(ipc.moveEntriesBack, 'the undo raced the later move').not.toHaveBeenCalled();
	} finally {
		await unmount(again.component);
		vi.useRealTimers();
	}
});

/** A drag that ends over the empty part of the folders pane ends in a click
 * there, and that part puts the open entry away when it is pressed. */
it('the pane stays when a folder drag ends over the folders pane', async () => {
	ipc.entry.mockImplementation(() => Promise.resolve(keptIn(vault.id)));
	const { component } = following(vault);
	try {
		await openKept();
		const area = host.querySelector<HTMLElement>('aside [role="presentation"]');
		// The click comes with the release, before anything else runs.
		await drag([...(place(work.id)?.querySelectorAll('button') ?? [])].at(-1), area, () =>
			area?.click()
		);

		expect(pane(), 'the end of a drag closed the entry').not.toBeNull();
		area?.click();
		flushSync();
		expect(pane(), 'a press on the empty pane no longer closes the entry').toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('offers no drag and no folder list in a vault it cannot write', async () => {
	ipc.entry.mockImplementation(() => Promise.resolve(keptIn(vault.id)));
	const { component } = following(vault, true);
	try {
		await openKept();
		expect(folderLine()).toBeNull();
		await drag(listRow('node-3'), place(work.id));
		await drag([...(place(work.id)?.querySelectorAll('button') ?? [])].at(-1), place('all'));

		expect(ipc.moveEntries).not.toHaveBeenCalled();
		expect(ipc.moveGroup).not.toHaveBeenCalled();
		// Looking is not a change: the row for what is at the top stays.
		expect(place('loose')?.textContent).toContain('Not in a folder');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('offers no drag and no folder list for an entry in the bin', async () => {
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(
			titled(id, 'thrown away', {
				group: shelved.bin.id,
				binned: { since: null, within: null, from: vault.id },
				deletion: 'forever'
			})
		)
	);
	const { component } = following(vault);
	try {
		pressed('Recycle Bin');
		flushSync();
		listRow('thrown away')?.click();
		await settled();
		expect(pane()).not.toBeNull();
		expect(folderLine()).toBeNull();

		await drag(listRow('thrown away'), place(work.id));
		expect(ipc.moveEntries).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The button "+ Entry", whose list asks where. */
const plus = () => exactly('Entry');

/**
 * A press on a button with the pointer, the way WebKit makes one: a pressed
 * button takes no focus there, so unless the press is kept from moving it, the
 * focus leaves whatever had it for nothing at all - before the click. happy-dom
 * moves no focus on a click, which hid a list that could not be closed with the
 * button that opened it. Answers whether the press kept the focus where it was.
 */
function pressAsWebKit(button: HTMLElement): boolean {
	const down = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
	button.dispatchEvent(down);
	if (!down.defaultPrevented && document.activeElement instanceof HTMLElement) {
		document.activeElement.blur();
	}
	flushSync();
	button.click();
	flushSync();
	return down.defaultPrevented;
}

it('asks where a new entry goes from All entries, and Return makes it at the top when nothing was chosen before', async () => {
	ipc.createEntry.mockResolvedValue({ tree: vault, entry: atTop.id });
	ipc.entry.mockImplementation(() => Promise.resolve(keptIn(vault.id)));
	const { component } = following(vault);
	try {
		expect(plus().getAttribute('aria-haspopup')).toBe('listbox');
		plus().click();
		flushSync();

		expect(plus().getAttribute('aria-expanded')).toBe('true');
		expect(reads()).toContain('Put it in');
		expect(document.activeElement).toBe(picker());
		expect(ipc.createEntry, 'made before it was asked where').not.toHaveBeenCalled();

		inPicker('Enter');
		await settled();
		expect(ipc.createEntry).toHaveBeenCalledWith(vault.id, 'login');
		expect(host.querySelector('input[role="combobox"]')).toBeNull();

		// Pressed again while it is open, the button closes it, and nothing is
		// made. The press that opens it is an ordinary one, so a value being
		// typed into the pane is left, and written, first.
		expect(pressAsWebKit(plus()), 'the press that opens the list kept the focus').toBe(false);
		expect(picker()).toBeDefined();
		const stayed = pressAsWebKit(plus());
		expect(host.querySelector('input[role="combobox"]'), 'the press opened it again').toBeNull();
		expect(stayed).toBe(true);
		expect(plus().getAttribute('aria-expanded')).toBe('false');
		expect(ipc.createEntry).toHaveBeenCalledTimes(1);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('offers the folder the last new entry went into first', async () => {
	ipc.createEntry.mockResolvedValue({ tree: vault, entry: inWork.id });
	ipc.entry.mockImplementation((id: string) => Promise.resolve(titled(id, 'Postgres')));
	const { component } = following(vault);
	try {
		plus().click();
		flushSync();
		inPicker('ArrowDown');
		inPicker('Enter');
		await settled();
		expect(ipc.createEntry).toHaveBeenLastCalledWith(work.id, 'login');

		plus().click();
		flushSync();
		const active = picker().getAttribute('aria-activedescendant');
		expect(active && document.getElementById(active)?.textContent).toContain('Work');
		inPicker('Enter');
		await settled();
		expect(ipc.createEntry).toHaveBeenLastCalledWith(work.id, 'login');
		expect(ipc.createEntry).toHaveBeenCalledTimes(2);

		// Escape closes the list, makes nothing, and leaves the pane alone.
		plus().click();
		flushSync();
		inPicker('Escape');
		expect(host.querySelector('input[role="combobox"]')).toBeNull();
		expect(document.activeElement).toBe(plus());
		expect(pane(), 'the Escape that closed the list closed the pane too').not.toBeNull();
		expect(ipc.createEntry).toHaveBeenCalledTimes(2);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('makes the entry where it is asked when a folder is shown, with no question', async () => {
	ipc.createEntry.mockResolvedValue({ tree: vault, entry: inWork.id });
	ipc.entry.mockImplementation((id: string) => Promise.resolve(titled(id, 'Postgres')));
	const { component } = following(vault);
	try {
		pressed('Work');
		flushSync();
		expect(plus().getAttribute('aria-haspopup')).toBeNull();
		plus().click();
		flushSync();
		expect(host.querySelector('input[role="combobox"]')).toBeNull();
		await settled();
		expect(ipc.createEntry).toHaveBeenCalledWith(work.id, 'login');

		pressed('Not in a folder');
		flushSync();
		plus().click();
		await settled();
		expect(ipc.createEntry).toHaveBeenLastCalledWith(vault.id, 'login');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('makes the entry at the top with no question in a vault with no folders', async () => {
	const { tree, gmail } = twoLogins();
	ipc.createEntry.mockResolvedValue({ tree, entry: gmail.id });
	ipc.entry.mockImplementation(() => Promise.resolve(readOf(gmail)));
	const { component } = following(tree);
	try {
		expect(reads(), 'a vault with no folders has a row for being in none').not.toContain(
			'Not in a folder'
		);
		plus().click();
		flushSync();
		expect(host.querySelector('input[role="combobox"]')).toBeNull();
		await settled();
		expect(ipc.createEntry).toHaveBeenCalledWith(tree.id, 'login');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A list that opened over a new password being changed would take the pane
 * the password is in with the entry it makes. */
it('asks nothing while a new password is being changed', async () => {
	const { gmail, drive } = withPasswords();
	const tree = group({
		name: 'Root',
		entries: [gmail, drive],
		sections: [
			group({ name: 'Work' }),
			group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever' })
		]
	});
	const { component } = following(tree);
	try {
		pressed('Gmail');
		await settled();
		typeNewPassword('n3w-from-the-website');
		await settled();

		plus().click();
		await settled();
		run('newEntry');
		await settled();

		expect(host.querySelector('input[role="combobox"]'), 'the list opened').toBeNull();
		expect(reads()).toContain('Save the new password?');
		expect(ipc.createEntry).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('lists the entries at the top of the vault on their own, and searches only them', async () => {
	const { component } = following(vault);
	try {
		expect(place('loose')?.textContent).toContain('Not in a folder');
		expect(place('loose')?.textContent).toContain('1');

		pressed('Not in a folder');
		flushSync();
		expect(listRow('node-3')).toBeDefined();
		expect(listRow('Postgres'), 'an entry in a folder is not in no folder').toBeUndefined();
		expect(reads()).toContain('1 entry here');

		type('postgres');
		expect(listRow('Postgres')).toBeUndefined();
		expect(reads()).toContain('Nothing matches');
		type('node');
		expect(listRow('node-3')).toBeDefined();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

it('keeps Not in a folder while it is chosen, saying every entry is in a folder', async () => {
	const { component, props } = following(vault);
	try {
		pressed('Not in a folder');
		flushSync();
		props.root = group({ ...vault, entries: [] });
		flushSync();

		expect(place('loose')?.textContent).toContain('0');
		expect(reads()).toContain('Every entry is in a folder');
		expect(reads()).toContain(
			'Entries at the top of the vault, outside every folder, show up here.'
		);

		pressed('All entries');
		flushSync();
		expect(place('loose'), 'the row stayed for nothing once left').toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The mockup's empty folder says both ways an entry gets in, and both are
 * there now. A vault Coffer will not write back offers neither. */
it('says an empty folder takes entries made in it or dragged in, where it can', async () => {
	const empty = group({ name: 'Clients' });
	const tree = group({
		name: 'Root',
		sections: [empty, group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever' })]
	});
	for (const [readOnly, says, offers] of [
		[false, 'Entries can be made here or dragged in from other folders.', true],
		[true, 'Entries live in folders. This one has none of its own.', false]
	] as const) {
		const { component } = following(tree, readOnly);
		try {
			pressed('Clients');
			flushSync();
			expect(reads()).toContain('There is nothing in “⁨Clients⁩” yet');
			expect(reads(), String(readOnly)).toContain(says);
			expect(reads().includes('Add an entry'), String(readOnly)).toBe(offers);
		} finally {
			await unmount(component);
			vi.useRealTimers();
		}
	}
});

it('offers no rename and no deletion on Not in a folder', async () => {
	const { component } = following(vault);
	try {
		pressed('Not in a folder');
		flushSync();

		expect(host.querySelector('[aria-label="Rename this folder"]')).toBeNull();
		expect(host.querySelector('[aria-label="Delete this folder"]')).toBeNull();
		expect(host.querySelector('[aria-label="New folder"]')).not.toBeNull();

		pressed('Work');
		flushSync();
		expect(host.querySelector('[aria-label="Rename this folder"]')).not.toBeNull();
		expect(host.querySelector('[aria-label="Delete this folder"]')).not.toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * A vault to choose rows in: a router at the top, four logins in Work, an
 * empty Banking, and a recycle bin. Built afresh for each test, because the
 * trees a test answers with are made from it.
 */
function chest() {
	const work = group({ name: 'Work' });
	const banking = group({ name: 'Banking' });
	const bin = group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever' });
	const tree = group({ name: 'Root', deletion: 'forever' });
	const router = row({ title: 'Router', group: tree.id });
	const logins = ['Gmail', 'Drive', 'Slack', 'Jira'].map((title) =>
		row({ title, username: `${title.toLowerCase()}@example.com`, group: work.id })
	);
	tree.entries = [router];
	tree.sections = [{ ...work, entries: logins }, banking, bin];
	return { tree, work, banking, bin, router, logins };
}

/** The tree with every row named taken out of wherever it is. */
function without(tree: Group, ids: ReadonlySet<string>): Group {
	return {
		...tree,
		entries: tree.entries.filter((each) => !ids.has(each.id)),
		sections: tree.sections.map((section) => without(section, ids))
	};
}

/** The tree after `rows` went to the bin from the folders they were in. */
function binnedOf(tree: Group, rows: EntryRow[]): Group {
	const into = (here: Group): Group =>
		here.isRecycleBin
			? {
					...here,
					entries: [
						...here.entries,
						...rows.map((each) => ({
							...each,
							group: here.id,
							deletion: 'forever' as const,
							binned: { since: null, within: null, from: each.group }
						}))
					]
				}
			: { ...here, sections: here.sections.map(into) };
	return into(without(tree, new Set(rows.map((each) => each.id))));
}

/** The tree after `rows` moved into the folder `into`. */
function movedOf(tree: Group, rows: EntryRow[], into: string): Group {
	const put = (here: Group): Group => ({
		...here,
		entries:
			here.id === into
				? [...here.entries, ...rows.map((each) => ({ ...each, group: into }))]
				: here.entries,
		sections: here.sections.map(put)
	});
	return put(without(tree, new Set(rows.map((each) => each.id))));
}

/** The tree with `tag` on every row named. */
function taggedOf(tree: Group, ids: readonly string[], tag: string): Group {
	return {
		...tree,
		entries: tree.entries.map((each) =>
			ids.includes(each.id) ? { ...each, tags: [...each.tags, tag] } : each
		),
		sections: tree.sections.map((section) => taggedOf(section, ids, tag))
	};
}

/** A press on the row with this title, Cmd held unless other keys are said. */
function choose(title: string, keys: MouseEventInit = { metaKey: true }) {
	const target = listRow(title);
	if (!target) throw new Error(`no row for ${title}`);
	target.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, ...keys }));
	flushSync();
}

/** The bar over the list while rows are chosen. */
const selectionBar = () => host.querySelector<HTMLElement>('[aria-label="The selected entries"]');

/** How many the bar says are chosen, or nothing when there is no bar. */
function count(): string | null {
	return selectionBar()?.querySelector('span')?.textContent?.trim() ?? null;
}

/** A button of the bar, by its words or its label. */
function inBar(label: string): HTMLButtonElement {
	const found = [...(selectionBar()?.querySelectorAll<HTMLButtonElement>('button') ?? [])].find(
		(each) => (each.getAttribute('aria-label') ?? each.textContent?.trim()) === label
	);
	if (!found) throw new Error(`the bar has no ${label}`);
	return found;
}

/** The titles of the rows drawn chosen, in the order they are drawn. */
function lit(): string[] {
	return [...host.querySelectorAll<HTMLElement>('.bg-selection')].map(
		(each) => each.textContent?.replace(', selected', '').match(/\S+/)?.[0] ?? ''
	);
}

/** Cmd+A, pressed with the focus on `target`. */
function selectAll(target: EventTarget = document.body): KeyboardEvent {
	const pressed = press('a', target);
	flushSync();
	return pressed;
}

/** What each entry a deletion sent was sent with. */
function deleting(call = 0): { entry: string; deletion: string }[] {
	return ipc.deleteEntries.mock.calls[call]?.[0] ?? [];
}

/** The bar's Add tag, with `name` typed into its field and Return pressed. */
function addTag(name: string) {
	inBar('Add tag').click();
	flushSync();
	const field = selectionBar()?.querySelector('input');
	if (!field) throw new Error('the tag field did not open');
	field.value = name;
	field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
}

/** The tree with every row in it saying what `deletion` it would get. */
function deletingAs(tree: Group, deletion: EntryRow['deletion']): Group {
	return {
		...tree,
		entries: tree.entries.map((each) => ({ ...each, deletion })),
		sections: tree.sections.map((section) => deletingAs(section, deletion))
	};
}

/** The entry Rust reads for a row in the bin: read only, and deleted for good
 * from there. */
function inBinOf(read: ReturnType<typeof entry>, from: string) {
	return { ...read, deletion: 'forever' as const, binned: { since: null, within: null, from } };
}

/**
 * Cmd-click chooses a row and opens nothing, Shift-click reaches from the
 * last one pressed in the order the list draws them, and the chosen rows are
 * drawn on the selection plane under a bar that says how many.
 */
it('chooses rows with Cmd and Shift and opens nothing', async () => {
	const { tree } = chest();
	const { component } = following(tree);
	try {
		expect(selectionBar()).toBeNull();
		choose('Gmail');
		expect(count()).toBe('1 selected');
		choose('Slack', { shiftKey: true });
		expect(count()).toBe('3 selected');
		expect(lit()).toEqual(['Gmail', 'Drive', 'Slack']);
		expect(host.textContent, 'the column names are still drawn').not.toContain('Changed');

		choose('Drive');
		expect(lit()).toEqual(['Gmail', 'Slack']);
		await settled();
		expect(ipc.entry, 'a press that chose a row opened it').not.toHaveBeenCalled();
		expect(pane()).toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Cmd+A is every row the list draws: after a search only what it found, and
 * in the bin never the deleted folders above the entries. */
it('chooses every row the search left on Cmd+A, and nothing it hid or the bin’s folders', async () => {
	const { tree } = chest();
	const { component } = following(tree);
	try {
		type('i');
		const all = selectAll();
		expect(all.defaultPrevented, 'the key went on to the system’s Select All').toBe(true);
		expect(lit()).toEqual(['Gmail', 'Drive', 'Jira']);
		expect(count()).toBe('3 selected');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}

	const vault = binnedVault();
	const shown = mounted(vault.tree);
	try {
		pressed('Recycle Bin');
		flushSync();
		selectAll();
		expect(count()).toBe('1 selected');
		expect(lit()).toEqual(['Old']);
		expect(inBar('Put back')).toBeTruthy();
	} finally {
		await unmount(shown.component);
		vi.useRealTimers();
	}
});

/** Select All is the system's in a field, in the entry pane and wherever
 * nothing can be chosen: the key is left alone there. */
it('leaves Cmd+A to a field being typed in, to the entry pane and to a vault it cannot write', async () => {
	const { tree, logins } = chest();
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(logins.find((each) => each.id === id) ?? logins[0]))
	);
	const { component } = following(tree);
	try {
		expect(selectAll(search()).defaultPrevented).toBe(false);
		pressed('Gmail');
		await settled();
		const inside = pane()?.querySelector('h1') ?? null;
		expect(inside).not.toBeNull();
		expect(selectAll(inside ?? host).defaultPrevented).toBe(false);
		expect(selectionBar()).toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}

	const shut = following(tree, true);
	try {
		expect(selectAll().defaultPrevented).toBe(false);
		choose('Gmail');
		await settled();
		expect(selectionBar(), 'a vault Coffer will not write offered a choice').toBeNull();
		expect(ipc.entry, 'a Cmd-click in a vault it will not write opened nothing').toHaveBeenCalled();
	} finally {
		await unmount(shut.component);
		vi.useRealTimers();
	}
});

/** A row a search hides leaves the choice, and clearing the search does not
 * bring it back: nothing the bar does reaches a row out of sight. */
it('lets go of rows a search hides, for good', async () => {
	const { tree } = chest();
	const { component } = following(tree);
	try {
		choose('Gmail');
		choose('Slack');
		type('slack');
		expect(count()).toBe('1 selected');
		type('');
		expect(count()).toBe('1 selected');
		expect(lit()).toEqual(['Slack']);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Another folder is another list, and a file read again is another vault:
 * either lets go of the choice. */
it('lets go of the choice when another folder is chosen or the file is read again', async () => {
	const { tree, logins } = chest();
	const { component } = following(tree);
	try {
		choose('Gmail');
		choose('Drive');
		pressed('Work');
		flushSync();
		expect(selectionBar()).toBeNull();
		pressed('All entries');
		flushSync();
		expect(selectionBar(), 'the choice came back with the folder').toBeNull();

		ipc.tagEntries.mockResolvedValue({ tree, changed: [logins[0].id] });
		ipc.save.mockRejectedValue({ code: 'externalChange', message: 'the database changed on disk' });
		ipc.rival.mockResolvedValue({ modified: null, entries: 5 });
		ipc.reload.mockResolvedValue(tree);
		choose('Gmail');
		choose('Drive');
		inBar('Add tag').click();
		flushSync();
		const field = selectionBar()?.querySelector('input');
		if (!field) throw new Error('the tag field did not open');
		field.value = 'work';
		field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
		await settled();
		exactly('Take the file on disk').click();
		await settled();
		expect(ipc.reload).toHaveBeenCalledTimes(1);
		expect(selectionBar(), 'the choice outlived the vault it was made in').toBeNull();
	} finally {
		ipc.save.mockReset();
		ipc.save.mockResolvedValue(undefined);
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The tree coming back keeps what is chosen, less what it took out of the
 * list. */
it('keeps the choice when the tree comes back, less what left the list', async () => {
	const { tree, logins } = chest();
	const { component, props } = following(tree);
	try {
		choose('Gmail');
		choose('Drive');
		choose('Slack');
		props.root = without(tree, new Set([logins[1].id]));
		flushSync();
		expect(lit()).toEqual(['Gmail', 'Slack']);
		props.root = tree;
		flushSync();
		expect(lit(), 'a row that left came back chosen').toEqual(['Gmail', 'Slack']);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * Three entries to the bin are one call, one save and one notice, whose one
 * undo puts all three back in one call. The pane on one of them goes with it,
 * and nothing is opened again when several came back.
 */
it('moves every chosen entry to the bin in one call and one save, and takes them all back with one undo', async () => {
	const { tree, logins } = chest();
	const going = logins.slice(0, 3);
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(logins.find((each) => each.id === id) ?? logins[0]))
	);
	ipc.deleteEntries.mockResolvedValue(binnedOf(tree, going));
	ipc.putBackEntries.mockResolvedValue(tree);
	const { component } = following(tree);
	try {
		pressed('Gmail');
		await settled();
		choose('Slack', { shiftKey: true });
		expect(count()).toBe('3 selected');
		inBar('Delete').click();
		await settled();

		expect(ipc.deleteEntries).toHaveBeenCalledTimes(1);
		expect(deleting()).toEqual(going.map((each) => ({ entry: each.id, deletion: 'bin' })));
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(pane(), 'the pane stayed on an entry in the bin').toBeNull();
		expect(selectionBar()).toBeNull();
		expect(toast()?.textContent).toContain('Moved 3 entries to the Recycle Bin');

		ipc.entry.mockClear();
		expect(press('z').defaultPrevented).toBe(true);
		await settled();
		expect(ipc.putBackEntries).toHaveBeenCalledTimes(1);
		expect(ipc.putBackEntries).toHaveBeenCalledWith(going.map((each) => each.id));
		expect(ipc.save).toHaveBeenCalledTimes(2);
		expect(ipc.entry, 'an undo of three opened one of them').not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** In the bin, deleting is for good: the bar asks, and the notice says they
 * went forever and offers nothing back. */
it('asks before deleting chosen entries for good, and says they went forever', async () => {
	const { tree, logins } = chest();
	const binned = binnedOf(tree, logins.slice(0, 2));
	ipc.deleteEntries.mockResolvedValue(without(binned, new Set(logins.map((each) => each.id))));
	const { component } = following(binned);
	try {
		pressed('Recycle Bin');
		flushSync();
		choose('Gmail');
		choose('Drive');
		inBar('Delete forever…').click();
		flushSync();
		expect(ipc.deleteEntries).not.toHaveBeenCalled();
		expect(reads()).toContain('Delete 2 entries forever? This can’t be undone.');

		exactly('Delete forever').click();
		await settled();
		expect(deleting()).toEqual(
			logins.slice(0, 2).map((each) => ({ entry: each.id, deletion: 'forever' }))
		);
		expect(toast()?.textContent).toContain('Deleted 2 entries forever');
		expect(undo()).toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** One of them would no longer do what the rows showed: Rust refuses the lot,
 * nothing is deleted, and the window says so and reads everything again. */
it('says nothing was deleted when Rust refuses the batch because one of them changed, and reads again', async () => {
	const { tree } = chest();
	ipc.deleteEntries.mockRejectedValue({
		code: 'deletionChanged',
		message: 'that deletion would no longer do what was shown, so nothing was deleted'
	});
	const { component } = following(tree);
	try {
		choose('Gmail');
		choose('Drive');
		const reading = ipc.tree.mock.calls.length;
		inBar('Delete').click();
		await settled();
		expect(toast()?.textContent).toContain('so nothing was deleted');
		expect(ipc.tree.mock.calls.length).toBeGreaterThan(reading);
		expect(ipc.save).not.toHaveBeenCalled();
		expect(undo()).toBeNull();
	} finally {
		ipc.deleteEntries.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A new password typed into the open entry is the reader's until they say
 * otherwise. A batch holding that entry asks first, and deletes nothing. */
it('keeps the pane and asks when a chosen entry holding a new password would go', async () => {
	const { gmail, tree } = withPasswords();
	const { component } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();
		const typed = typeNewPassword('n3w-from-the-website');
		await settled();
		choose('Google Drive');
		expect(count()).toBe('2 selected');

		inBar('Delete').click();
		await settled();
		expect(ipc.deleteEntries, 'the bar deleted the entry holding it').not.toHaveBeenCalled();
		expect(titleField()?.value).toBe('Gmail');
		expect(typed.value).toBe('n3w-from-the-website');
		expect(reads()).toContain('Save the new password?');

		// From the menu bar too, once the field is left and the item is
		// offered at all.
		typed.blur();
		flushSync();
		expect(applying()).toContain('moveToBin');
		run('moveToBin');
		await settled();
		expect(ipc.deleteEntries, 'the menu deleted the entry holding it').not.toHaveBeenCalled();
		expect(titleField()?.value).toBe('Gmail');
		expect(typed.value).toBe('n3w-from-the-website');
		expect(count()).toBe('2 selected');
		expect(ipc.entry).toHaveBeenCalledWith(gmail.id);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A batch lets go of what was typed into each entry it names, with a number
 * newer than any of it, so a draft still on its way is dropped when it lands. */
it('deletes chosen entries with a number newer than anything typed into them', async () => {
	const { gmail, drive, tree } = twoLogins();
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(id === gmail.id ? gmail : drive))
	);
	ipc.deleteEntries.mockResolvedValue(binnedOf(tree, [gmail, drive]));
	const { component } = mounted(tree);
	try {
		pressed('Gmail');
		await settled();
		const field = titleField();
		if (!field) throw new Error('there is no title field');
		field.value = 'Gmail, renamed';
		field.dispatchEvent(new Event('input', { bubbles: true }));
		await vi.advanceTimersByTimeAsync(1_000);
		const drafted = Math.max(...ipc.draft.mock.calls.map((call) => call[5] as number));

		choose('Google Drive');
		inBar('Delete').click();
		await settled();
		expect(ipc.deleteEntries.mock.lastCall?.[1]).toBeGreaterThan(drafted);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A second press of Delete while the batch is on its way asks for nothing
 * new, and is dropped rather than refused. */
it('drops a second press of Delete while the first batch is on its way', async () => {
	const { tree, logins } = chest();
	const answered = Promise.withResolvers<Group>();
	ipc.deleteEntries.mockReturnValue(answered.promise);
	const { component } = following(tree);
	try {
		choose('Gmail');
		choose('Drive');
		inBar('Delete').click();
		inBar('Delete').click();
		run('moveToBin');
		await settled();
		expect(ipc.deleteEntries).toHaveBeenCalledTimes(1);

		answered.resolve(binnedOf(tree, logins.slice(0, 2)));
		await settled();
		expect(ipc.save).toHaveBeenCalledTimes(1);
	} finally {
		ipc.deleteEntries.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** An entry whose folder is on its way to the bin goes with the folder. Sent
 * on its own as well, it would reach Rust after the folder and be refused. */
it('does not send an entry whose folder is on its way to the bin', async () => {
	const { tree, work, router, logins } = chest();
	const folderGoing = Promise.withResolvers<Group>();
	ipc.deleteGroup.mockReturnValue(folderGoing.promise);
	ipc.deleteEntries.mockResolvedValue(binnedOf(tree, [router]));
	const { component } = following(tree);
	try {
		pressed('Work');
		flushSync();
		host.querySelector<HTMLButtonElement>('[aria-label="Delete this folder"]')?.click();
		flushSync();
		exactly('Move to Recycle Bin').click();
		flushSync();
		expect(ipc.deleteGroup).toHaveBeenCalledWith(work.id, 'bin');

		pressed('All entries');
		flushSync();
		choose('Router');
		choose('Gmail');
		inBar('Delete').click();
		await settled();
		expect(deleting()).toEqual([{ entry: router.id, deletion: 'bin' }]);
		expect(deleting().map((each) => each.entry)).not.toContain(logins[0].id);
		folderGoing.resolve(tree);
		await settled();
	} finally {
		ipc.deleteGroup.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * A tag goes on in one call, and the notice offers it back. The open entry
 * among them is read again, since its chips are part of it, and the undo
 * takes the tag off only the entries Rust says it went on.
 */
it('tags every chosen entry once, reads the open one again, and takes the tag off only where it was added', async () => {
	const { tree, logins } = chest();
	const [gmail, drive, slack] = logins;
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(logins.find((each) => each.id === id) ?? gmail))
	);
	const after = taggedOf(tree, [gmail.id, slack.id], 'work');
	ipc.tagEntries.mockResolvedValue({ tree: after, changed: [gmail.id, slack.id] });
	ipc.untagEntries.mockResolvedValue(tree);
	const { component } = following(tree);
	try {
		pressed('Gmail');
		await settled();
		choose('Drive');
		choose('Slack');
		ipc.entry.mockClear();
		inBar('Add tag').click();
		flushSync();
		const field = selectionBar()?.querySelector('input');
		if (!field) throw new Error('the tag field did not open');
		field.value = ' work ';
		field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
		await settled();

		expect(ipc.tagEntries).toHaveBeenCalledTimes(1);
		expect(ipc.tagEntries).toHaveBeenCalledWith([gmail.id, drive.id, slack.id], 'work');
		expect(ipc.entry, 'the open entry was not read again').toHaveBeenCalledWith(gmail.id);
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(toast()?.textContent).toContain('Added “\u2068work\u2069” to 2 entries');
		expect(toast()?.querySelector('use')?.getAttribute('href')).toBe('#i-check');

		undo()?.click();
		await settled();
		expect(ipc.untagEntries).toHaveBeenCalledWith([gmail.id, slack.id], 'work');
		expect(ipc.save).toHaveBeenCalledTimes(2);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Nothing to do is nothing written: no save, no undo, and a sentence that
 * says why nothing happened. */
it('says so and saves nothing when every chosen entry already has the tag', async () => {
	const { tree } = chest();
	ipc.tagEntries.mockResolvedValue({ tree, changed: [] });
	const { component } = following(tree);
	try {
		choose('Gmail');
		choose('Drive');
		inBar('Add tag').click();
		flushSync();
		const field = selectionBar()?.querySelector('input');
		if (!field) throw new Error('the tag field did not open');
		field.value = 'work';
		field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
		await settled();
		expect(toast()?.textContent).toContain('2 entries already have “\u2068work\u2069”');
		expect(undo()).toBeNull();
		expect(ipc.save).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A tag the format would split is Rust's to refuse, and its sentence is the
 * one shown, as on one entry's chip. */
it('shows Rust’s refusal of a tag the file would split', async () => {
	const { tree } = chest();
	ipc.tagEntries.mockRejectedValue({
		code: 'refused',
		message:
			'a tag cannot be empty, hold a semicolon, a comma or a tab, or begin or end with a space'
	});
	const { component } = following(tree);
	try {
		choose('Gmail');
		choose('Drive');
		inBar('Add tag').click();
		flushSync();
		const field = selectionBar()?.querySelector('input');
		if (!field) throw new Error('the tag field did not open');
		field.value = 'a;b';
		field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
		await settled();
		expect(toast()?.textContent).toContain('a tag cannot be empty, hold a semicolon');
		expect(ipc.save).not.toHaveBeenCalled();
	} finally {
		ipc.tagEntries.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The bar's Move to… is the move every other way of moving makes: one call,
 * one notice, and an undo that takes back exactly what Rust says it moved. */
it('moves the chosen entries into a folder and moves them back with one undo', async () => {
	const { tree, work, banking, logins } = chest();
	const going = logins.slice(0, 2);
	const moved = going.map((each) => ({ entry: each.id, from: work.id }));
	ipc.moveEntries.mockResolvedValue({ tree: movedOf(tree, going, banking.id), moved });
	ipc.moveEntriesBack.mockResolvedValue(tree);
	const { component } = following(tree);
	try {
		choose('Gmail');
		choose('Drive');
		inBar('Move to…').click();
		flushSync();
		typeInPicker('Banking');
		inPicker('Enter');
		await settled();

		expect(ipc.moveEntries).toHaveBeenCalledWith(
			going.map((each) => each.id),
			banking.id
		);
		expect(toast()?.textContent).toContain('Moved 2 entries to “\u2068Banking\u2069”');
		press('z');
		await settled();
		expect(ipc.moveEntriesBack).toHaveBeenCalledWith(moved, banking.id);
		expect(ipc.save).toHaveBeenCalledTimes(2);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** An undo whose entries the vault has moved on from - put back by another
 * way, gone out of the file - is refused whole by Rust, and the window says
 * it can no longer be done and reads the tree again. */
it('says something changed since when an undo of a batch is refused', async () => {
	const { tree, logins } = chest();
	ipc.deleteEntries.mockResolvedValue(binnedOf(tree, logins.slice(0, 2)));
	ipc.putBackEntries.mockRejectedValue({
		code: 'refused',
		message: 'that is not in the recycle bin'
	});
	const { component } = following(tree);
	try {
		choose('Gmail');
		choose('Drive');
		inBar('Delete').click();
		await settled();
		const reading = ipc.tree.mock.calls.length;
		undo()?.click();
		await settled();
		expect(toast()?.textContent).toContain(
			'Something has changed since, so that can no longer be undone.'
		);
		expect(toast()?.textContent).not.toContain('not in the recycle bin');
		expect(ipc.tree.mock.calls.length).toBeGreaterThan(reading);
		expect(ipc.save).toHaveBeenCalledTimes(1);
	} finally {
		ipc.putBackEntries.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The bin's own way out, for several: one call, and an undo that sends them
 * back to the bin, as the bin. */
it('puts chosen entries back out of the bin, and sends them back on undo', async () => {
	const { tree, logins } = chest();
	const going = logins.slice(0, 2);
	const binned = binnedOf(tree, going);
	ipc.putBackEntries.mockResolvedValue(tree);
	ipc.deleteEntries.mockResolvedValue(binned);
	const { component } = following(binned);
	try {
		pressed('Recycle Bin');
		flushSync();
		selectAll();
		inBar('Put back').click();
		await settled();
		expect(ipc.putBackEntries).toHaveBeenCalledWith(going.map((each) => each.id));
		expect(toast()?.textContent).toContain('Put back 2 entries');

		undo()?.click();
		await settled();
		expect(deleting()).toEqual(going.map((each) => ({ entry: each.id, deletion: 'bin' })));
		expect(ipc.save).toHaveBeenCalledTimes(2);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A batch whose save failed is in memory and not in the file. The failure is
 * the sentence that matters, and an offer would push it off the screen. */
it('offers nothing back over a batch whose save failed', async () => {
	const { tree, logins } = chest();
	ipc.deleteEntries.mockResolvedValue(binnedOf(tree, logins.slice(0, 2)));
	ipc.save.mockRejectedValue({ code: 'io', message: 'No space left on device' });
	const { component } = following(tree);
	try {
		choose('Gmail');
		choose('Drive');
		inBar('Delete').click();
		await settled();
		expect(toast()?.textContent).toContain('No space left on device');
		expect(undo()).toBeNull();
	} finally {
		ipc.save.mockReset();
		ipc.save.mockResolvedValue(undefined);
		await unmount(component);
		vi.useRealTimers();
	}
});

/** One entry is named by its title, set apart from the sentence so that a
 * title that turns text around turns nothing else, and markup in it is text. */
it('names one chosen entry by its title, isolated, and many by how many', async () => {
	const evil = row({ title: 'evil\u202E<b>slip</b>' });
	const plain = row({ title: 'plain' });
	const tree = group({
		name: 'Root',
		entries: [evil, plain],
		sections: [group({ name: 'Recycle Bin', isRecycleBin: true, deletion: 'forever' })]
	});
	ipc.deleteEntries.mockResolvedValue(binnedOf(tree, [evil]));
	const { component } = following(tree);
	try {
		choose('evil');
		expect(count()).toBe('1 selected');
		inBar('Delete').click();
		await settled();
		expect(toast()?.textContent).toContain(
			'Moved “\u2068evil\u202E<b>slip</b>\u2069” to the Recycle Bin'
		);
		expect(host.querySelector('[data-notice] b')).toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The question is about the rows chosen when it was asked. Choosing again
 * closes it, so an answer never lands on rows nobody was asked about. */
it('closes the question when the choice changes under it', async () => {
	const { tree, logins } = chest();
	const binned = binnedOf(tree, logins);
	const { component } = following(binned);
	try {
		pressed('Recycle Bin');
		flushSync();
		choose('Gmail');
		choose('Drive');
		inBar('Delete forever…').click();
		flushSync();
		expect(host.querySelector('[data-confirm]')).not.toBeNull();
		choose('Slack');
		expect(host.querySelector('[data-confirm]')).toBeNull();
		expect(count()).toBe('3 selected');
		expect(ipc.deleteEntries).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Move to Recycle Bin in the menu bar acts on the rows chosen, when every one
 * of them goes to the bin, and on the open entry when nothing is chosen. In
 * the bin, where they would go for good, it is grey: only the bar's question
 * erases. */
it('acts on the choice from the menu’s Move to Recycle Bin, and on the open entry without one', async () => {
	const { tree, logins } = chest();
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(logins.find((each) => each.id === id) ?? logins[0]))
	);
	ipc.deleteEntries.mockResolvedValue(tree);
	const { component } = following(tree);
	try {
		pressed('Gmail');
		await settled();
		expect(applying()).toContain('moveToBin');
		choose('Drive');
		run('moveToBin');
		await settled();
		expect(deleting(0).map((each) => each.entry)).toEqual([logins[0].id, logins[1].id]);

		pressed('Slack');
		await settled();
		expect(selectionBar()).toBeNull();
		run('moveToBin');
		await settled();
		expect(deleting(1)).toEqual([{ entry: logins[2].id, deletion: 'bin' }]);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}

	const binned = following(binnedOf(tree, logins));
	try {
		pressed('Recycle Bin');
		flushSync();
		choose('Gmail');
		choose('Drive');
		expect(applying(), 'offered for rows that would go for good').not.toContain('moveToBin');
	} finally {
		await unmount(binned.component);
		vi.useRealTimers();
	}
});

/** Escape lets go of the choice first, back to the entry being read, and only
 * then puts the pane away. */
it('lets Escape go of the choice before it puts the pane away', async () => {
	const { tree, logins } = chest();
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(logins.find((each) => each.id === id) ?? logins[0]))
	);
	const { component } = following(tree);
	try {
		pressed('Gmail');
		await settled();
		choose('Drive');
		expect(count()).toBe('2 selected');

		window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
		flushSync();
		expect(selectionBar()).toBeNull();
		expect(titleField()?.value).toBe('Gmail');

		window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
		flushSync();
		expect(pane()).toBeNull();
		expect(selectionBar(), 'the entry put away stayed chosen on its own').toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A drag that starts on a chosen row carries every chosen row; one that
 * starts on a row not chosen carries that row alone, and leaves the choice. */
it('drags every chosen row onto a folder, and a row not chosen alone', async () => {
	const { tree, banking, logins } = chest();
	ipc.moveEntries.mockResolvedValue({ tree, moved: [] });
	const { component } = following(tree);
	try {
		choose('Gmail');
		choose('Drive');
		await drag(listRow('Drive'), place(banking.id));
		expect(ipc.moveEntries).toHaveBeenLastCalledWith([logins[0].id, logins[1].id], banking.id);

		await drag(listRow('Jira'), place(banking.id));
		expect(ipc.moveEntries).toHaveBeenLastCalledWith([logins[3].id], banking.id);
		expect(count(), 'a drag of another row lost the choice').toBe('2 selected');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The choice is read out as it changes, from a region that stays in the
 * document while the bar comes and goes: Cmd+A chooses with nothing under the
 * focus that would say so. */
it('says how many are chosen in a region a screen reader reads out', async () => {
	const { tree } = chest();
	const { component } = following(tree);
	try {
		const region = host.querySelector('[data-chosen]');
		expect(region?.getAttribute('role')).toBe('status');
		expect(region?.getAttribute('aria-live')).toBe('polite');
		expect(region?.textContent?.trim()).toBe('');

		selectAll();
		expect(region?.textContent?.trim()).toBe('5 selected');
		choose('Gmail');
		expect(region?.textContent?.trim()).toBe('4 selected');
		window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
		flushSync();
		expect(region?.textContent?.trim()).toBe('');
		expect(host.querySelector('[data-chosen]'), 'the region was drawn afresh').toBe(region);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A choice of several is the reader's, and putting the pane away - its close,
 * or the entry in it deleted from it - lets go only of that entry. */
it('keeps a choice of several when the open entry is put away or deleted from its pane', async () => {
	const { tree, logins } = chest();
	const jira = logins[3];
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(logins.find((each) => each.id === id) ?? logins[0]))
	);
	ipc.deleteEntries.mockResolvedValue(binnedOf(tree, [jira]));
	const { component } = following(tree);
	try {
		pressed('Gmail');
		await settled();
		choose('Drive');
		choose('Slack');
		expect(count()).toBe('3 selected');
		host.querySelector<HTMLButtonElement>('[aria-label="Close this entry"]')?.click();
		flushSync();
		expect(pane()).toBeNull();
		expect(count(), 'putting the pane away let go of the choice').toBe('3 selected');

		pressed('Jira');
		await settled();
		expect(selectionBar(), 'a plain press is a choice of one').toBeNull();
		choose('Drive');
		choose('Slack');
		pressed('Move to Recycle Bin', pane() ?? host);
		await settled();
		expect(deleting()).toEqual([{ entry: jira.id, deletion: 'bin' }]);
		expect(pane()).toBeNull();
		expect(lit(), 'deleting the open entry let go of the choice').toEqual(['Drive', 'Slack']);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Cmd+A is the system's Select All wherever the list is not the reader's to
 * choose from - under the settings or the question about the file on disk,
 * over a list with nothing in it - and with any other key held beside Cmd. */
it('leaves Cmd+A to the system under the settings, over an empty list and with another key held', async () => {
	const covered = open({ settings: sheet });
	flushSync();
	expect(press('a', document.body).defaultPrevented, 'under the settings').toBe(false);
	await unmount(covered);

	const { tree, logins } = chest();
	const asked = following(tree);
	ipc.tagEntries.mockResolvedValue({ tree, changed: [logins[0].id] });
	ipc.save.mockRejectedValue({ code: 'externalChange', message: 'the database changed on disk' });
	ipc.rival.mockResolvedValue({ modified: null, entries: 5 });
	try {
		choose('Gmail');
		addTag('work');
		await settled();
		expect(exactly('Take the file on disk')).toBeTruthy();
		expect(selectAll().defaultPrevented, 'under the question about the file').toBe(false);
	} finally {
		ipc.save.mockReset();
		ipc.save.mockResolvedValue(undefined);
		await unmount(asked.component);
		vi.useRealTimers();
	}

	const { component } = following(tree);
	try {
		type('nothing is called this');
		expect(selectAll().defaultPrevented, 'over a search that found nothing').toBe(false);
		type('');
		for (const held of [{ shiftKey: true }, { altKey: true }, { ctrlKey: true }]) {
			const event = new KeyboardEvent('keydown', {
				key: held.shiftKey ? 'A' : 'a',
				metaKey: true,
				bubbles: true,
				cancelable: true,
				...held
			});
			document.body.dispatchEvent(event);
			flushSync();
			expect(event.defaultPrevented, JSON.stringify(held)).toBe(false);
		}
		expect(selectionBar()).toBeNull();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}

	const empty = following(group({ name: 'Root' }));
	try {
		expect(selectAll().defaultPrevented, 'over an empty vault').toBe(false);
	} finally {
		await unmount(empty.component);
		vi.useRealTimers();
	}
});

/** Typing a new password into the open entry while Rust makes another keeps
 * the pane, and the new entry is neither opened nor chosen: chosen beside a
 * pane on another entry, it would be a choice the reader never made, with a
 * bar offering to delete it. */
it('neither opens nor chooses a new entry while a password typed during its making waits', async () => {
	const { gmail, tree } = withPasswords();
	const made = row({ title: 'Made', group: tree.id });
	const making = Promise.withResolvers<{ tree: Group; entry: string }>();
	ipc.createEntry.mockReturnValue(making.promise);
	const { component } = following(tree);
	try {
		pressed('Gmail');
		await settled();
		plus().click();
		await settled();
		expect(ipc.createEntry).toHaveBeenCalledTimes(1);
		const typed = typeNewPassword('n3w-from-the-website');
		await settled();

		making.resolve({ tree: { ...tree, entries: [...tree.entries, made] }, entry: made.id });
		await settled();
		expect(titleField()?.value).toBe('Gmail');
		expect(typed.value).toBe('n3w-from-the-website');
		expect(selectionBar(), 'the new entry was chosen beside the pane').toBeNull();
		expect(lit()).toEqual([]);
		expect(ipc.entry).not.toHaveBeenCalledWith(made.id);
		expect(ipc.entry).toHaveBeenCalledWith(gmail.id);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/**
 * The open entry, put back with others, is an entry like any other again, and
 * a new password can be typed into it. The undo would send it back to the bin,
 * where the field is not drawn, and the typed password would be nowhere: the
 * undo asks the pane first, sends nothing, and the field puts its question.
 */
it('keeps a new password typed into an entry put back with others when its undo would bin it again', async () => {
	const { gmail, drive, tree, read } = withPasswords();
	const binned = binnedOf(tree, [gmail, drive]);
	ipc.entry.mockImplementation((id: string) => Promise.resolve(inBinOf(read(id), tree.id)));
	ipc.putBackEntries.mockResolvedValue(tree);
	ipc.deleteEntries.mockResolvedValue(binned);
	const { component } = following(binned);
	try {
		pressed('Recycle Bin');
		flushSync();
		pressed('Gmail');
		await settled();
		choose('Google Drive');
		expect(count()).toBe('2 selected');

		ipc.entry.mockImplementation((id: string) => Promise.resolve(read(id)));
		inBar('Put back').click();
		await settled();
		expect(toast()?.textContent).toContain('Put back 2 entries');
		const typed = typeNewPassword('n3w-from-the-website');
		await settled();

		undo()?.click();
		await settled();
		expect(ipc.deleteEntries, 'the undo binned the entry holding it').not.toHaveBeenCalled();
		expect(typed.isConnected).toBe(true);
		expect(typed.value).toBe('n3w-from-the-website');
		expect(reads()).toContain('Save the new password?');
		expect(titleField()?.value).toBe('Gmail');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A file whose bin was switched off after it filled still lists what is in
 * it, and puts it back; but nothing deleted there goes to a bin, so an undo
 * that sends them back to one could only be refused, and none is offered. */
it('offers nothing back over entries put back into a vault that keeps no bin', async () => {
	const { tree, logins } = chest();
	const going = logins.slice(0, 2);
	ipc.putBackEntries.mockResolvedValue(deletingAs(tree, 'forever'));
	const { component } = following(binnedOf(tree, going));
	try {
		pressed('Recycle Bin');
		flushSync();
		selectAll();
		inBar('Put back').click();
		await settled();
		expect(ipc.putBackEntries).toHaveBeenCalledWith(going.map((each) => each.id));
		expect(toast()?.textContent).toContain('Put back 2 entries');
		expect(undo()).toBeNull();
		press('z');
		await settled();
		expect(ipc.deleteEntries).not.toHaveBeenCalled();
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** An undo of a put back that would no longer send one of them to the bin -
 * it went into a folder that went to the bin since - is refused whole by
 * Rust, and is too late, not broken. */
it('says something changed since when the undo of a put back finds one no longer goes to the bin', async () => {
	const { tree, logins } = chest();
	ipc.putBackEntries.mockResolvedValue(tree);
	ipc.deleteEntries.mockRejectedValue({
		code: 'deletionChanged',
		message: 'that deletion would no longer do what was shown, so nothing was deleted'
	});
	const { component } = following(binnedOf(tree, logins.slice(0, 2)));
	try {
		pressed('Recycle Bin');
		flushSync();
		selectAll();
		inBar('Put back').click();
		await settled();
		const reading = ipc.tree.mock.calls.length;
		undo()?.click();
		await settled();
		expect(ipc.deleteEntries).toHaveBeenCalledTimes(1);
		expect(toast()?.textContent).toContain(
			'Something has changed since, so that can no longer be undone.'
		);
		expect(ipc.tree.mock.calls.length).toBeGreaterThan(reading);
		expect(ipc.save).toHaveBeenCalledTimes(1);
	} finally {
		ipc.deleteEntries.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** An entry a tag went on, erased from the file since, refuses the tag's
 * undo whole: too late, and said so. */
it('says something changed since when a tag’s undo finds an entry gone', async () => {
	const { tree, logins } = chest();
	const [gmail, drive] = logins;
	ipc.tagEntries.mockResolvedValue({
		tree: taggedOf(tree, [gmail.id, drive.id], 'work'),
		changed: [gmail.id, drive.id]
	});
	ipc.untagEntries.mockRejectedValue({
		code: 'noSuchEntry',
		message: 'there is no such entry in this database'
	});
	const { component } = following(tree);
	try {
		choose('Gmail');
		choose('Drive');
		addTag('work');
		await settled();
		const reading = ipc.tree.mock.calls.length;
		undo()?.click();
		await settled();
		expect(toast()?.textContent).toContain(
			'Something has changed since, so that can no longer be undone.'
		);
		expect(toast()?.textContent).not.toContain('no such entry');
		expect(ipc.tree.mock.calls.length).toBeGreaterThan(reading);
		expect(ipc.save).toHaveBeenCalledTimes(1);
	} finally {
		ipc.untagEntries.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** An undo that fails for any other reason - the disk, a vault gone read
 * only - did not come too late. The window shows Rust's own sentence, and
 * reads nothing again as though the vault had moved on. */
it('shows the failure itself when an undo of a batch fails for another reason', async () => {
	const { tree, logins } = chest();
	ipc.deleteEntries.mockResolvedValue(binnedOf(tree, logins.slice(0, 2)));
	ipc.putBackEntries.mockRejectedValue({ code: 'io', message: 'the disk would not answer' });
	const { component } = following(tree);
	try {
		choose('Gmail');
		choose('Drive');
		inBar('Delete').click();
		await settled();
		const reading = ipc.tree.mock.calls.length;
		undo()?.click();
		await settled();
		expect(toast()?.textContent).toContain('the disk would not answer');
		expect(toast()?.textContent).not.toContain('changed since');
		expect(ipc.tree.mock.calls.length, 'the tree was read again').toBe(reading);
	} finally {
		ipc.putBackEntries.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A tag is a change to the open entry among them: a value revealed before it
 * is taken off the screen, as for any other change to the entry. */
it('takes revealed values off the open entry a tag lands on', async () => {
	const { gmail, drive, tree } = withPasswords();
	ipc.reveal.mockResolvedValue('the old one');
	ipc.tagEntries.mockResolvedValue({
		tree: taggedOf(tree, [gmail.id, drive.id], 'work'),
		changed: [gmail.id, drive.id]
	});
	const shown = () => pane()?.querySelector('[data-value]')?.textContent ?? '';
	const { component } = following(tree);
	try {
		pressed('Gmail');
		await settled();
		pressed('Show', pane() ?? host);
		await settled();
		expect(shown()).toBe('the old one');

		choose('Google Drive');
		addTag('work');
		await settled();
		expect(ipc.tagEntries).toHaveBeenCalledTimes(1);
		expect(shown(), 'the revealed password stayed up after the tag').toBe('');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The undo takes the tag off the open entry, whose chips are part of it, so
 * the pane reads it again. */
it('reads the open entry again when the tag’s undo takes it off', async () => {
	const { tree, logins } = chest();
	const [gmail, drive] = logins;
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(logins.find((each) => each.id === id) ?? gmail))
	);
	ipc.tagEntries.mockResolvedValue({
		tree: taggedOf(tree, [gmail.id, drive.id], 'work'),
		changed: [gmail.id, drive.id]
	});
	ipc.untagEntries.mockResolvedValue(tree);
	const { component } = following(tree);
	try {
		pressed('Gmail');
		await settled();
		choose('Drive');
		addTag('work');
		await settled();
		ipc.entry.mockClear();
		undo()?.click();
		await settled();
		expect(ipc.untagEntries).toHaveBeenCalledWith([gmail.id, drive.id], 'work');
		expect(ipc.entry, 'the open entry kept the chip the undo took off').toHaveBeenCalledWith(
			gmail.id
		);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A tag whose save failed is in memory and not in the file. The failure is
 * the sentence that matters, and an offer would push it off the screen. */
it('offers nothing back over a tag whose save failed', async () => {
	const { tree, logins } = chest();
	ipc.tagEntries.mockResolvedValue({
		tree: taggedOf(tree, [logins[0].id, logins[1].id], 'work'),
		changed: [logins[0].id, logins[1].id]
	});
	ipc.save.mockRejectedValue({ code: 'io', message: 'No space left on device' });
	const { component } = following(tree);
	try {
		choose('Gmail');
		choose('Drive');
		addTag('work');
		await settled();
		expect(toast()?.textContent).toContain('No space left on device');
		expect(undo()).toBeNull();
	} finally {
		ipc.save.mockReset();
		ipc.save.mockResolvedValue(undefined);
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The open entry put back with others is read again before the save, so the
 * pane does not go on offering Put back, and a deletion for good, on an entry
 * that has already left the bin for the length of the save. */
it('reads the open entry again when it is put back with others', async () => {
	const { tree, logins } = chest();
	const going = logins.slice(0, 2);
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(readOf(logins.find((each) => each.id === id) ?? logins[0]))
	);
	ipc.putBackEntries.mockResolvedValue(tree);
	const { component } = following(binnedOf(tree, going));
	try {
		pressed('Recycle Bin');
		flushSync();
		pressed('Gmail');
		await settled();
		choose('Drive');
		ipc.entry.mockClear();
		inBar('Put back').click();
		await settled();
		expect(ipc.entry).toHaveBeenCalledWith(going[0].id);
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(
			ipc.entry.mock.invocationCallOrder[0],
			'the entry was read again only after the save'
		).toBeLessThan(ipc.save.mock.invocationCallOrder[0]);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A put back whose save failed is offered back to nobody: the failure is the
 * sentence that matters. */
it('offers nothing back over a put back whose save failed', async () => {
	const { tree, logins } = chest();
	ipc.putBackEntries.mockResolvedValue(tree);
	ipc.save.mockRejectedValue({ code: 'io', message: 'No space left on device' });
	const { component } = following(binnedOf(tree, logins.slice(0, 2)));
	try {
		pressed('Recycle Bin');
		flushSync();
		selectAll();
		inBar('Put back').click();
		await settled();
		expect(toast()?.textContent).toContain('No space left on device');
		expect(undo()).toBeNull();
	} finally {
		ipc.save.mockReset();
		ipc.save.mockResolvedValue(undefined);
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Rust refuses the whole batch when one of them is no longer in the bin, and
 * its sentence is the one shown: nothing was put back, and nothing is
 * written. */
it('shows Rust’s refusal of a batch put back', async () => {
	const { tree, logins } = chest();
	ipc.putBackEntries.mockRejectedValue({
		code: 'refused',
		message: 'that is not in the recycle bin'
	});
	const { component } = following(binnedOf(tree, logins.slice(0, 2)));
	try {
		pressed('Recycle Bin');
		flushSync();
		selectAll();
		inBar('Put back').click();
		await settled();
		expect(toast()?.textContent).toContain('that is not in the recycle bin');
		expect(undo()).toBeNull();
		expect(ipc.save).not.toHaveBeenCalled();
	} finally {
		ipc.putBackEntries.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A second press of Put back while the first is on its way asks for nothing
 * new, and is dropped rather than refused for entries already out of the
 * bin. */
it('drops a second Put back while the first is on its way', async () => {
	const { tree, logins } = chest();
	const answered = Promise.withResolvers<Group>();
	ipc.putBackEntries.mockReturnValue(answered.promise);
	const { component } = following(binnedOf(tree, logins.slice(0, 2)));
	try {
		pressed('Recycle Bin');
		flushSync();
		selectAll();
		inBar('Put back').click();
		inBar('Put back').click();
		await settled();
		expect(ipc.putBackEntries).toHaveBeenCalledTimes(1);

		answered.resolve(tree);
		await settled();
		expect(ipc.save).toHaveBeenCalledTimes(1);
	} finally {
		ipc.putBackEntries.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The chevron beside "+ Entry", which lists the other kinds and the vault's
 * templates. */
const chevron = () => named('Other kinds of entry');

/** A line of that list, by what it says. */
function menuItem(label: string): HTMLButtonElement {
	const found = [...host.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find(
		(each) => each.textContent?.trim() === label
	);
	if (!found) throw new Error(`the list has no ${label}`);
	return found;
}

/** Whether the field holds the focus with every character of it selected,
 * so that the first key replaces it. */
function selectedWhole(field: HTMLInputElement | null): boolean {
	return (
		field !== null &&
		document.activeElement === field &&
		field.selectionStart === 0 &&
		field.selectionEnd === field.value.length
	);
}

/** A vault with one folder, and what Rust answers once an entry is made in it. */
function oneFolder() {
	const folder = group({ name: 'Work' });
	const tree = group({ name: 'Root', sections: [folder] });
	const made = row({ title: '', username: '', group: folder.id });
	const grown = group({ ...tree, sections: [group({ ...folder, entries: [made] })] });
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(
			entry({
				id,
				group: folder.id,
				fields: [
					field({ name: 'Title', kind: 'title', value: '', empty: true }),
					field({ name: 'Licence key', kind: 'custom', value: null, protected: true }),
					field({ name: 'Licensed to', kind: 'custom', value: '' })
				]
			})
		)
	);
	return { folder, tree, made, grown };
}

/** A kind chosen from the list is made in the folder being shown, opened, and
 * its name takes the focus to be typed. A field the kind writes in lines is a
 * text area before anything is in it. */
it('makes the kind chosen from "+ Entry" in the folder shown, ready to be named', async () => {
	const { folder, tree, made, grown } = oneFolder();
	ipc.createEntry.mockResolvedValue({ tree: grown, entry: made.id });
	const { component } = following(tree);
	try {
		pressed('Work');
		flushSync();
		chevron().click();
		await settled();
		expect(chevron().getAttribute('aria-expanded')).toBe('true');
		menuItem('Software licence').click();
		await settled();

		expect(ipc.createEntry).toHaveBeenCalledWith(folder.id, 'licence');
		expect(host.querySelector('[role="menu"]'), 'the list stayed open').toBeNull();
		expect(selectedWhole(titleField()), 'the name did not take the focus').toBe(true);
		expect(host.querySelector('textarea[aria-label="Licence key"]')?.getAttribute('rows')).toBe(
			'4'
		);
		expect(host.querySelector('textarea[aria-label="Licensed to"]')?.getAttribute('rows')).toBe(
			'1'
		);
		expect(ipc.save).toHaveBeenCalledTimes(1);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Two presses during the second a save holds Rust make one entry. */
it('makes one entry for a second press while the first is on its way', async () => {
	const { tree, made, grown } = oneFolder();
	const making = Promise.withResolvers<{ tree: Group; entry: string }>();
	ipc.createEntry.mockReturnValue(making.promise);
	const { component } = following(tree);
	try {
		pressed('Work');
		flushSync();
		plus().click();
		chevron().click();
		await settled();
		menuItem('Bank card').click();
		await settled();
		expect(ipc.createEntry).toHaveBeenCalledTimes(1);

		making.resolve({ tree: grown, entry: made.id });
		await settled();
		expect(ipc.createEntry).toHaveBeenCalledTimes(1);
	} finally {
		ipc.createEntry.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A vault with nothing in it offers every kind at once, and makes the one
 * pressed at the top of the vault. A vault Coffer will not write offers none. */
it('offers every kind in an empty vault, and makes the one pressed', async () => {
	const empty = group({ name: 'Root' });
	const made = row({ title: '', group: empty.id });
	ipc.createEntry.mockResolvedValue({ tree: group({ ...empty, entries: [made] }), entry: made.id });
	ipc.entry.mockResolvedValue(entry({ id: made.id, group: empty.id }));
	for (const readOnly of [true, false]) {
		const { component } = following(empty, readOnly);
		try {
			const chips = host.querySelector('[role="group"][aria-label="Add an entry"]');
			if (readOnly) {
				expect(chips, 'a read-only vault offered a kind').toBeNull();
				continue;
			}
			expect(
				[...(chips?.querySelectorAll('button') ?? [])].map((each) => each.textContent?.trim())
			).toEqual(kinds().offered.map((offer) => offer.name));
			pressed('Wi-Fi', chips ?? host);
			await settled();
			expect(ipc.createEntry).toHaveBeenCalledWith(empty.id, 'wifi');
		} finally {
			await unmount(component);
			vi.useRealTimers();
		}
	}
});

/** A vault with its own templates lists them after the kinds, and one chosen
 * in "All entries" asks where first, as a kind does. It is made by Rust from
 * the template's id, with every value and file copied there. */
it('makes an entry from one of the vault’s templates, asking where in All entries', async () => {
	const template = row({ title: 'Card template' });
	const hidden = row({ title: null });
	const templates = group({ name: 'Templates', isTemplates: true, entries: [template, hidden] });
	const folder = group({ name: 'Work' });
	const tree = group({ name: 'Root', sections: [templates, folder] });
	const made = row({ title: 'Card template', group: folder.id });
	ipc.createFromTemplate.mockResolvedValue({
		tree: group({ ...tree, sections: [templates, group({ ...folder, entries: [made] })] }),
		entry: made.id
	});
	ipc.entry.mockResolvedValue(titled(made.id, 'Card template', { group: folder.id }));
	const { component } = following(tree);
	try {
		chevron().click();
		await settled();
		const menu = host.querySelector('[role="menu"]');
		expect(menu?.textContent).toContain('Templates in this vault');
		expect(
			named('A template whose name is hidden', menu ?? host).querySelector('svg'),
			'a protected name was not drawn as the mask'
		).not.toBeNull();

		menuItem('Card template').click();
		await settled();
		expect(reads()).toContain('Put it in');
		expect(ipc.createFromTemplate).not.toHaveBeenCalled();
		typeInPicker('Work');
		inPicker('Enter');
		await settled();

		expect(ipc.createFromTemplate).toHaveBeenCalledWith(folder.id, template.id);
		expect(ipc.createEntry).not.toHaveBeenCalled();
		expect(titleField()?.value).toBe('Card template');
		expect(selectedWhole(titleField()), 'the name did not take the focus').toBe(true);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A vault keeping one template, with a "Work" folder holding one entry. */
function templating() {
	const template = row({ title: 'Card template' });
	const templates = group({ name: 'Templates', isTemplates: true, entries: [template] });
	const kept = row({ title: 'Kept login' });
	const folder = group({ name: 'Work', entries: [] });
	const filled = group({ ...folder, entries: [{ ...kept, group: folder.id }] });
	const tree = group({ name: 'Root', sections: [templates, filled] });
	return { template, templates, kept: { ...kept, group: folder.id }, folder: filled, tree };
}

/** A template chosen while a folder is shown is made there with no question,
 * the way a kind is: the folder is where the list is. */
it('makes an entry from a template in the folder shown without asking where', async () => {
	const { template, templates, folder, tree } = templating();
	const made = row({ title: 'Card template', group: folder.id });
	ipc.createFromTemplate.mockResolvedValue({
		tree: group({ ...tree, sections: [templates, group({ ...folder, entries: [made] })] }),
		entry: made.id
	});
	ipc.entry.mockResolvedValue(titled(made.id, 'Card template', { group: folder.id }));
	const { component } = following(tree);
	try {
		pressed('Work', host.querySelector('aside') ?? host);
		flushSync();
		chevron().click();
		await settled();
		menuItem('Card template').click();
		await settled();

		expect(host.querySelector('input[role="combobox"]'), 'it asked where').toBeNull();
		expect(ipc.createFromTemplate).toHaveBeenCalledWith(folder.id, template.id);
		expect(ipc.createEntry).not.toHaveBeenCalled();
		expect(titleField()?.value).toBe('Card template');
		expect(ipc.save).toHaveBeenCalledTimes(1);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A template that is no longer one when it is chosen - its folder went to the
 * bin, or the file was read again, after the list was drawn - is refused by
 * Rust, said in Rust's words, and nothing else happens: the entry being read
 * stays, and nothing is written. */
it('says why a template was refused and leaves the pane', async () => {
	const { template, folder, kept, tree } = templating();
	ipc.createFromTemplate.mockRejectedValue({
		code: 'refused',
		message: 'that entry is not one of the templates this vault keeps'
	});
	ipc.entry.mockResolvedValue(titled(kept.id, 'Kept login', { group: folder.id }));
	const { component, props } = following(tree);
	try {
		pressed('Work', host.querySelector('aside') ?? host);
		flushSync();
		pressed('Kept login');
		await settled();
		chevron().click();
		await settled();
		menuItem('Card template').click();
		await settled();

		expect(ipc.createFromTemplate).toHaveBeenCalledWith(folder.id, template.id);
		expect(notice()).toContain('that entry is not one of the templates this vault keeps');
		expect(titleField()?.value).toBe('Kept login');
		expect(props.root).toEqual(tree);
		expect(ipc.save).not.toHaveBeenCalled();
	} finally {
		ipc.createFromTemplate.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A vault with two logins, the first open, and what Rust answers for a copy
 * of it. */
function copying() {
	const { gmail, drive, tree } = twoLogins();
	const copy = row({ title: 'Gmail copy', username: gmail.username });
	const grown = group({ ...tree, entries: [gmail, copy, drive] });
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(id === copy.id ? readOf(copy) : id === gmail.id ? readOf(gmail) : readOf(drive))
	);
	return { gmail, drive, tree, copy, grown };
}

/** Duplicate makes a copy beside the entry and opens it with its name
 * selected: "Gmail copy" is there to be typed over. */
it('opens a copy of the entry with its name selected', async () => {
	const { gmail, tree, copy, grown } = copying();
	ipc.duplicateEntry.mockResolvedValue({ tree: grown, entry: copy.id });
	const { component, props } = following(tree);
	try {
		pressed('Gmail');
		await settled();
		exactly('Duplicate').click();
		await settled();

		expect(ipc.duplicateEntry).toHaveBeenCalledWith(gmail.id);
		expect(props.root).toEqual(grown);
		expect(titleField()?.value).toBe('Gmail copy');
		expect(selectedWhole(titleField())).toBe(true);
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(notice(), 'a copy is said in a notice as well as in the pane').toBe('');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});

/** What is being typed into the entry is part of it: the copy is asked for
 * once the field is written and the write has arrived, from the pill and from
 * the menu bar alike. The press on the pill leaves the field, as WebKit's does;
 * the menu bar's item leaves it on its own. Tauri answers the two side by
 * side, and a copy that got there first held the login as it was. */
it('copies the entry once what is being typed into it has been written', async () => {
	const { gmail, tree, copy, grown } = copying();
	ipc.duplicateEntry.mockResolvedValue({ tree: grown, entry: copy.id });
	for (const [how, copied] of [
		[
			'the pill',
			(field: HTMLElement | null) => {
				field?.blur();
				exactly('Duplicate').click();
			}
		],
		['the menu', () => run('duplicate')]
	] as const) {
		const writing = Promise.withResolvers<Entry>();
		ipc.setField.mockReturnValue(writing.promise);
		const { component } = following(tree);
		try {
			pressed('Gmail');
			await settled();
			const login = host.querySelector<HTMLInputElement>('input[aria-label="Login"]');
			login?.focus();
			if (login) login.value = 'me@work.example';
			login?.dispatchEvent(new Event('input', { bubbles: true }));

			copied(login);
			await settled();
			expect(ipc.setField, how).toHaveBeenCalledWith(
				gmail.id,
				'UserName',
				'me@work.example',
				false,
				expect.any(Number)
			);
			expect(ipc.duplicateEntry, `${how}: copied before the login arrived`).not.toHaveBeenCalled();

			writing.resolve(readOf(gmail));
			await settled();
			expect(ipc.duplicateEntry, how).toHaveBeenCalledTimes(1);
		} finally {
			ipc.setField.mockReset();
			ipc.duplicateEntry.mockClear();
			await unmount(component);
			vi.useRealTimers();
		}
	}
});

/** A tag and a field's new name are written when they are left, like a value,
 * and the copy waits for them the same way: chosen from the menu bar with the
 * focus in either, it holds what was typed there. */
it('copies the entry once a tag or a field name being typed has been written', async () => {
	const { gmail, tree, copy, grown } = copying();
	const owned = entry({
		id: gmail.id,
		group: gmail.group,
		fields: [
			field({ name: 'Title', kind: 'title', value: 'Gmail', empty: false }),
			field({ name: 'Recovery', kind: 'custom', value: 'phone', empty: false })
		]
	});
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(id === gmail.id ? owned : readOf(copy))
	);
	ipc.duplicateEntry.mockResolvedValue({ tree: grown, entry: copy.id });
	for (const [what, typed] of [
		[
			'a tag',
			async () => {
				exactly('+ tag').click();
				flushSync();
				await settled();
				const tag = host.querySelector<HTMLInputElement>('input[aria-label="A new tag"]');
				expect(document.activeElement).toBe(tag);
				if (tag) tag.value = 'work';
				return ipc.setTags;
			}
		],
		[
			'a field’s name',
			async () => {
				named('Rename Recovery').click();
				flushSync();
				await settled();
				const name = host.querySelector<HTMLInputElement>(
					'input[aria-label="New name for Recovery"]'
				);
				expect(document.activeElement).toBe(name);
				if (name) name.value = 'Recovery phone';
				return ipc.renameField;
			}
		]
	] as const) {
		const writing = Promise.withResolvers<Entry>();
		ipc.setTags.mockReturnValue(writing.promise);
		ipc.renameField.mockReturnValue(writing.promise);
		const { component } = following(tree);
		try {
			pressed('Gmail');
			await settled();
			const sent = await typed();

			run('duplicate');
			await settled();
			expect(sent, what).toHaveBeenCalledTimes(1);
			expect(ipc.duplicateEntry, `${what}: copied before it arrived`).not.toHaveBeenCalled();

			writing.resolve(owned);
			await settled();
			expect(ipc.duplicateEntry, what).toHaveBeenCalledTimes(1);
		} finally {
			ipc.setTags.mockReset();
			ipc.renameField.mockReset();
			ipc.duplicateEntry.mockClear();
			await unmount(component);
			vi.useRealTimers();
		}
	}
});

/** A copy Rust answers after the reader chose another entry is in the list
 * and in the file, and the pane stays with the later choice. */
it('leaves the pane on an entry chosen while a copy was being made', async () => {
	const { drive, tree, copy, grown } = copying();
	const copying_ = Promise.withResolvers<{ tree: Group; entry: string }>();
	ipc.duplicateEntry.mockReturnValue(copying_.promise);
	const { component } = following(tree);
	try {
		pressed('Gmail');
		await settled();
		exactly('Duplicate').click();
		exactly('Duplicate').click();
		await settled();
		pressed('Google Drive');
		await settled();

		copying_.resolve({ tree: grown, entry: copy.id });
		await settled();
		expect(ipc.duplicateEntry, 'a second press made a second copy').toHaveBeenCalledTimes(1);
		expect(titleField()?.value).toBe(drive.title);
		expect(ipc.entry).not.toHaveBeenCalledWith(copy.id);
		expect(listRow('Gmail copy')).toBeDefined();
	} finally {
		ipc.duplicateEntry.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** The same for an entry chosen while the copy waited for what was typed
 * into the original to arrive: the pane is taken as it was at the press, and
 * a second press during the wait makes no second copy. */
it('leaves the pane on an entry chosen while the copy waited for a write', async () => {
	const { gmail, drive, tree, copy, grown } = copying();
	const writing = Promise.withResolvers<Entry>();
	ipc.setField.mockReturnValue(writing.promise);
	// From the copy on, the vault holds it, and the window reading the tree
	// again after the write - as it does - reads it there.
	ipc.duplicateEntry.mockImplementation(() => {
		ipc.tree.mockResolvedValue(grown);
		return Promise.resolve({ tree: grown, entry: copy.id });
	});
	const { component } = following(tree);
	try {
		pressed('Gmail');
		await settled();
		const login = host.querySelector<HTMLInputElement>('input[aria-label="Login"]');
		login?.focus();
		if (login) login.value = 'me@work.example';
		login?.dispatchEvent(new Event('input', { bubbles: true }));
		run('duplicate');
		run('duplicate');
		await settled();
		expect(ipc.setField).toHaveBeenCalledTimes(1);
		expect(ipc.duplicateEntry).not.toHaveBeenCalled();

		pressed('Google Drive');
		await settled();
		expect(titleField()?.value).toBe(drive.title);

		writing.resolve(readOf(gmail));
		await settled();
		expect(ipc.duplicateEntry, 'a second press made a second copy').toHaveBeenCalledTimes(1);
		expect(ipc.duplicateEntry).toHaveBeenCalledWith(gmail.id);
		expect(titleField()?.value, 'the copy took the pane from the later choice').toBe(drive.title);
		expect(ipc.entry).not.toHaveBeenCalledWith(copy.id);
		expect(listRow('Gmail copy')).toBeDefined();
		expect(ipc.save).toHaveBeenCalled();
	} finally {
		ipc.setField.mockReset();
		ipc.duplicateEntry.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** A refusal is said in Rust's own words, and nothing else happens. */
it('says why a copy was refused', async () => {
	const { tree } = copying();
	ipc.duplicateEntry.mockRejectedValue({
		code: 'refused',
		message: 'the recycle bin takes nothing but what is deleted'
	});
	const { component } = following(tree);
	try {
		pressed('Gmail');
		await settled();
		exactly('Duplicate').click();
		await settled();
		expect(notice()).toContain('the recycle bin takes nothing but what is deleted');
		expect(titleField()?.value).toBe('Gmail');
		expect(ipc.save).not.toHaveBeenCalled();
	} finally {
		ipc.duplicateEntry.mockReset();
		await unmount(component);
		vi.useRealTimers();
	}
});

/** Duplicate in the menu bar is offered on the condition the pill is drawn
 * on: an entry read, in a vault Coffer writes, out of the bin. */
it('offers a copy only of an entry that can be copied', async () => {
	const { tree } = copying();
	const { component } = following(tree);
	try {
		expect(applying(), 'a copy of nothing was offered').not.toContain('duplicate');
		pressed('Gmail');
		await settled();
		expect(applying()).toContain('duplicate');
		expect(exactly('Duplicate').disabled).toBe(false);

		// Rows chosen: the menu bar's verbs act on the choice, and the entry in
		// the pane need not be one of it. The pane's own pill names its entry.
		choose('Google Drive');
		expect(applying(), 'a copy of the pane was offered over a choice').not.toContain('duplicate');
		run('duplicate');
		await settled();
		expect(ipc.duplicateEntry).not.toHaveBeenCalled();
		expect(exactly('Duplicate').disabled).toBe(false);
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}

	const readOnly = following(tree, true);
	try {
		pressed('Gmail');
		await settled();
		expect(applying(), 'offered in a vault Coffer will not write').not.toContain('duplicate');
		expect(() => exactly('Duplicate')).toThrow();
		expect(host.querySelector('[aria-label="Other kinds of entry"]')).toBeNull();
		run('duplicate');
		await settled();
		expect(ipc.duplicateEntry).not.toHaveBeenCalled();
	} finally {
		await unmount(readOnly.component);
		vi.useRealTimers();
	}

	const binned = following(vault);
	try {
		pressed('Recycle Bin');
		flushSync();
		ipc.entry.mockResolvedValue(
			titled(shelved.thrown.id, 'thrown away', {
				group: shelved.bin.id,
				binned: shelved.thrown.binned,
				deletion: 'forever'
			})
		);
		pressed('thrown away');
		await settled();
		expect(applying(), 'a copy out of the bin was offered').not.toContain('duplicate');
		expect(() => exactly('Duplicate')).toThrow();
	} finally {
		await unmount(binned.component);
		vi.useRealTimers();
	}
});

/** A copy is an entry made in this window like any other, and "+ Entry" in
 * "All entries" offers the folder it went into first. */
it('offers the folder a copy went into first, as for any entry made', async () => {
	const copy = row({ title: 'Postgres copy', group: work.id });
	const grown = group({
		...vault,
		sections: [group({ ...work, entries: [inWork, copy] }), ...vault.sections.slice(1)]
	});
	ipc.duplicateEntry.mockResolvedValue({ tree: grown, entry: copy.id });
	ipc.entry.mockImplementation((id: string) =>
		Promise.resolve(titled(id, id === copy.id ? 'Postgres copy' : 'Postgres', { group: work.id }))
	);
	const { component } = following(vault);
	try {
		pressed('Postgres');
		await settled();
		exactly('Duplicate').click();
		await settled();
		expect(titleField()?.value).toBe('Postgres copy');

		plus().click();
		flushSync();
		const active = picker().getAttribute('aria-activedescendant');
		expect(active && document.getElementById(active)?.textContent).toContain('Work');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});
