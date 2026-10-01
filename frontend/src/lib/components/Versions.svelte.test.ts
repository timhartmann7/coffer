import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { entry, field, version } from '$lib/fixtures';
import type { Version } from '$lib/model';
import { reactive } from '$lib/props.svelte';
import Versions from './Versions.svelte';

const ipc = vi.hoisted(() => ({
	version: vi.fn(),
	revealVersion: vi.fn(),
	restoreVersion: vi.fn(),
	deleteVersion: vi.fn(),
	clearHistory: vi.fn()
}));
vi.mock('$lib/ipc', () => ipc);

const OLD = 'version 1 password';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.revealVersion.mockResolvedValue(OLD);
});

afterEach(() => {
	host.remove();
	// A selection one test made is still standing in the next one otherwise,
	// and a stand-in for the selection has to go before it can be cleared.
	vi.restoreAllMocks();
	document.getSelection()?.removeAllRanges();
});

const listed = [
	version({ index: 0, modified: '2021-06-02T12:00:00Z' }),
	version({ index: 1, modified: '2022-06-03T12:00:00Z' }),
	version({ index: 2, modified: '2023-06-04T12:00:00Z' })
];

/** A list of versions the way the window holds one: with the entry it is of. */
function of(versions: Version[], entry = 'an-entry') {
	return { entry, versions };
}

function show(props: Record<string, unknown> = {}) {
	return mount(Versions, {
		target: host,
		props: {
			entry: 'an-entry',
			history: of(listed),
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy: vi.fn(),
			onVersions: vi.fn(),
			onChanged: vi.fn(),
			onFailure: vi.fn(),
			...props
		}
	});
}

function button(label: string): HTMLButtonElement {
	const found = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === label
	);
	if (!found) throw new Error(`no button labelled ${label}`);
	return found;
}

function open() {
	const found = [...host.querySelectorAll('button')].find((candidate) =>
		candidate.textContent?.includes('Versions')
	);
	found?.click();
	flushSync();
}

/** The block is closed until it is asked for, which is what the mockup says and
 * what keeps an entry with a hundred versions from being a wall of dates. */
it('stays closed until it is opened', () => {
	const component = show();
	flushSync();

	expect(host.textContent).toContain('Versions');
	expect(host.textContent).not.toContain('2021');

	open();
	expect(host.textContent).toContain('2021');

	return unmount(component);
});

/** Newest first: what a reader is looking for is what changed last. The order
 * comes from the engine, which sorts by date rather than by position. */
it('lists the newest version first', () => {
	const component = show();
	open();

	const years = (host.textContent ?? '').match(/20\d\d/g);
	expect(years).toEqual(['2023', '2022', '2021']);

	return unmount(component);
});

it('says so rather than drawing an empty list', () => {
	const component = show({ history: of([]) });
	open();

	expect(host.textContent).toContain('Nothing yet');

	return unmount(component);
});

/** A version is addressed by its position, never by its date: two versions
 * written in the same second read as the same moment. */
it('views, restores and deletes a version by its position', async () => {
	const onChanged = vi.fn();
	const onVersions = vi.fn();
	ipc.version.mockResolvedValue(entry({ fields: [field({ name: 'Title', value: 'was' })] }));
	ipc.restoreVersion.mockResolvedValue(entry());
	ipc.deleteVersion.mockResolvedValue(of([]));

	const component = show({ onChanged, onVersions });
	open();

	// The first row is the newest version, which is index 2.
	[...host.querySelectorAll('button')]
		.filter((each) => each.textContent?.trim() === 'View')[0]
		?.click();
	await vi.waitFor(() => expect(ipc.version).toHaveBeenCalledWith('an-entry', 2));

	[...host.querySelectorAll('button')]
		.filter((each) => each.textContent?.trim() === 'Restore')[1]
		?.click();
	await vi.waitFor(() => expect(ipc.restoreVersion).toHaveBeenCalledWith('an-entry', 1));
	expect(onChanged).toHaveBeenCalled();

	host.querySelectorAll<HTMLButtonElement>('[aria-label="Delete this version"]')[2]?.click();
	flushSync();
	button('Drop it').click();
	await vi.waitFor(() => expect(ipc.deleteVersion).toHaveBeenCalledWith('an-entry', 0));
	expect(onVersions).toHaveBeenCalledWith(of([]));

	return unmount(component);
});

/** A value a version protects is a secret like any other: it comes one field at
 * a time, on an explicit reveal, and it is never in the payload that drew the
 * version. */
it("asks for a version's protected value one field at a time", async () => {
	ipc.version.mockResolvedValue(
		entry({
			fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
		})
	);

	const component = show();
	open();
	button('View').click();
	await vi.waitFor(() => expect(host.querySelector('[data-value]')).not.toBeNull());
	flushSync();

	expect(host.textContent).not.toContain(OLD);
	host.querySelector<HTMLButtonElement>('[aria-label="Show Password as it was"]')?.click();
	await vi.waitFor(() => expect(host.textContent).toContain(OLD));
	expect(ipc.revealVersion).toHaveBeenCalledWith('an-entry', 2, 'Password');

	await unmount(component);
	expect(document.body.textContent).not.toContain(OLD);
});

/** Clearing every version is the one action here that cannot be undone, so it
 * asks first and the destructive half is the red one. */
it('asks before it drops every version', async () => {
	const onVersions = vi.fn();
	ipc.clearHistory.mockResolvedValue(of([]));

	const component = show({ onVersions });
	open();

	button('Clear the history').click();
	flushSync();
	expect(ipc.clearHistory).not.toHaveBeenCalled();
	expect(host.textContent).toContain('Drop all 3 versions?');

	button('Keep them').click();
	flushSync();
	expect(ipc.clearHistory).not.toHaveBeenCalled();

	button('Clear the history').click();
	flushSync();
	button('Clear the history').click();
	await vi.waitFor(() => expect(ipc.clearHistory).toHaveBeenCalledWith('an-entry'));
	expect(onVersions).toHaveBeenCalledWith(of([]));

	return unmount(component);
});

/**
 * Every protected value in an expanded version holds its own node. One node
 * shared between the rows would put the value of the field that was asked for
 * into the row of the field that was not - the reader clicks the eye beside
 * "Password" and reads it under "API token".
 */
it('puts a revealed value in the row it was asked for', async () => {
	ipc.version.mockResolvedValue(
		entry({
			fields: [
				field({ name: 'API token', kind: 'custom', value: null, empty: false }),
				field({ name: 'Password', kind: 'password', value: null, empty: false })
			]
		})
	);
	ipc.revealVersion.mockImplementation((_entry: string, _index: number, name: string) =>
		Promise.resolve(`the ${name}`)
	);

	const component = show();
	open();
	button('View').click();
	await vi.waitFor(() => expect(host.querySelectorAll('[data-value]')).toHaveLength(2));
	flushSync();

	const rows = [...host.querySelectorAll('[data-value]')];
	host.querySelector<HTMLButtonElement>('[aria-label="Show Password as it was"]')?.click();
	await vi.waitFor(() => expect(host.textContent).toContain('the Password'));
	flushSync();

	expect(rows[0].textContent).toBe('');
	expect(rows[1].textContent).toBe('the Password');

	return unmount(component);
});

/**
 * A value read in a previous version is copied the way the entry's own is:
 * through Rust, by the version's position and the field's name, and the part
 * the reader selected rather than the system's plain pasteboard write. The
 * eye's row answers Cmd+C, and the copy beside it the pointer.
 */
it("copies a version's value through Rust, whole or the part selected", async () => {
	ipc.version.mockResolvedValue(
		entry({
			fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
		})
	);
	const onCopy = vi.fn();

	const component = show({ onCopy });
	open();
	button('View').click();
	await vi.waitFor(() => expect(host.querySelector('[data-value]')).not.toBeNull());
	flushSync();

	const eye = host.querySelector<HTMLButtonElement>('[aria-label="Show Password as it was"]');
	eye?.click();
	await vi.waitFor(() => expect(host.textContent).toContain(OLD));
	flushSync();

	host.querySelector<HTMLButtonElement>('[aria-label="Copy Password as it was"]')?.click();
	expect(onCopy).toHaveBeenLastCalledWith(2, 'Password', null);

	const node = host.querySelector('[data-value]') as HTMLElement;
	const range = document.createRange();
	range.setStart(node.firstChild as Text, 8);
	range.setEnd(node.firstChild as Text, 16);
	document.getSelection()?.removeAllRanges();
	document.getSelection()?.addRange(range);
	const copied = new ClipboardEvent('copy', { bubbles: true, cancelable: true });
	node.dispatchEvent(copied);
	expect(copied.defaultPrevented, 'the system copied an old password').toBe(true);
	expect(onCopy).toHaveBeenLastCalledWith(2, 'Password', { from: 8, to: 16 });
	document.getSelection()?.removeAllRanges();

	const pressed = new KeyboardEvent('keydown', {
		key: 'c',
		metaKey: true,
		bubbles: true,
		cancelable: true
	});
	host.querySelector('[aria-label="Hide Password as it was"]')?.dispatchEvent(pressed);
	expect(pressed.defaultPrevented).toBe(true);
	expect(onCopy).toHaveBeenLastCalledWith(2, 'Password', null);
	expect(onCopy).toHaveBeenCalledTimes(3);

	return unmount(component);
});

/**
 * A version is addressed by its position, and dropping one moves every position
 * after it. A panel left open on a number that now names a different version
 * would show one version's fields and reveal another's values.
 */
it('closes an open version when the list underneath it changes', async () => {
	ipc.version.mockResolvedValue(entry({ fields: [field({ name: 'Title', value: 'was' })] }));
	ipc.deleteVersion.mockResolvedValue(of([listed[0], listed[1]]));

	const component = show();
	open();

	button('View').click();
	await vi.waitFor(() => expect(host.textContent).toContain('A version is read only'));
	flushSync();

	host.querySelectorAll<HTMLButtonElement>('[aria-label="Delete this version"]')[2]?.click();
	flushSync();
	button('Drop it').click();
	await vi.waitFor(() => expect(ipc.deleteVersion).toHaveBeenCalled());
	flushSync();

	expect(host.textContent).not.toContain('A version is read only');

	return unmount(component);
});

/** A database Coffer will not write back has versions to read and nothing to
 * do to them. */
it('offers nothing but a look on a database it cannot write', () => {
	const component = show({ readOnly: true });
	open();

	const named = [...host.querySelectorAll('button')].map((each) => each.textContent?.trim());
	expect(named).toContain('View');
	expect(named).not.toContain('Restore');
	expect(named).not.toContain('Clear the history');
	expect(host.querySelector('[aria-label="Delete this version"]')).toBeNull();

	return unmount(component);
});

/**
 * A version is the one record of what an entry held before, and dropping one
 * is written to the file at once. It asks first, the way clearing all of them
 * does, and it asks about the version whose trash was pressed.
 */
it('asks before it drops one version', async () => {
	const onVersions = vi.fn();
	ipc.deleteVersion.mockResolvedValue(of([listed[0], listed[2]]));

	const component = show({ onVersions });
	open();

	const trash = () =>
		host.querySelectorAll<HTMLButtonElement>('[aria-label="Delete this version"]');
	// Newest first, so the second row is the version at index 1.
	trash()[1]?.click();
	flushSync();
	expect(ipc.deleteVersion).not.toHaveBeenCalled();
	expect(host.textContent).toContain('Drop this version?');
	expect(host.querySelectorAll('[data-confirm]')).toHaveLength(1);

	button('Keep it').click();
	flushSync();
	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(ipc.deleteVersion).not.toHaveBeenCalled();

	trash()[1]?.click();
	flushSync();
	button('Drop it').click();
	await vi.waitFor(() => expect(ipc.deleteVersion).toHaveBeenCalledWith('an-entry', 1));
	expect(ipc.deleteVersion).toHaveBeenCalledTimes(1);
	expect(onVersions).toHaveBeenCalledWith(of([listed[0], listed[2]]));

	return unmount(component);
});

/**
 * A question about a version names it by its position, and a list that changed
 * under it may have put another version in that place. The answer would then
 * drop a version nobody asked about, so the question goes with the list.
 */
it('takes a question about a version away when the list underneath it changes', () => {
	const props = reactive({
		entry: 'an-entry',
		history: of(listed),
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onVersions: vi.fn(),
		onChanged: vi.fn(),
		onFailure: vi.fn()
	});
	const component = mount(Versions, { target: host, props });
	open();

	host.querySelectorAll<HTMLButtonElement>('[aria-label="Delete this version"]')[0]?.click();
	flushSync();
	expect(host.textContent).toContain('Drop this version?');

	props.history = of([listed[1], listed[2]]);
	flushSync();
	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(ipc.deleteVersion).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * A position means something only in the history of the entry it was read
 * from. The window once left one entry's list under the next entry's name, and
 * Restore and Delete went to the entry on the screen with a position from the
 * list that was not its own. A list of another entry's is not drawn at all, so
 * there is nothing on the screen to press.
 */
it("draws no list that is not its entry's, so nothing can act on one", async () => {
	ipc.restoreVersion.mockResolvedValue(entry());
	const props = reactive({
		entry: 'the next entry',
		history: of(listed, 'the last entry'),
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onVersions: vi.fn(),
		onChanged: vi.fn(),
		onFailure: vi.fn()
	});
	const component = mount(Versions, { target: host, props });
	open();

	expect(host.textContent).not.toMatch(/\d/);
	expect(host.textContent).not.toContain('Nothing yet');
	const named = [...host.querySelectorAll('button')].map((each) => each.textContent?.trim());
	expect(named).toEqual(['Versions']);

	// Its own list arrives, and every action on it names its own entry.
	props.history = of([listed[0]], 'the next entry');
	flushSync();
	expect(host.textContent).toContain('2021');
	expect(host.textContent).not.toContain('2023');
	button('Restore').click();
	await vi.waitFor(() => expect(ipc.restoreVersion).toHaveBeenCalledWith('the next entry', 0));
	expect(ipc.restoreVersion).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/**
 * Reading a version waits its turn in Rust, behind a save that may prune the
 * history it was read from. A version that arrives after the list changed was
 * read at a position that may name another version now, and drawn it would
 * show one version's fields and copy another's values.
 */
it('drops a version that arrives after the list under it changed', async () => {
	const reading = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.version.mockReturnValue(reading.promise);
	const props = reactive({
		entry: 'an-entry',
		history: of(listed),
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onVersions: vi.fn(),
		onChanged: vi.fn(),
		onFailure: vi.fn()
	});
	const component = mount(Versions, { target: host, props });
	open();

	button('View').click();
	await vi.waitFor(() => expect(ipc.version).toHaveBeenCalledWith('an-entry', 2));
	props.history = of([listed[1], listed[2]]);
	flushSync();

	reading.resolve(entry({ fields: [field({ name: 'Title', value: 'what version 2 held' })] }));
	await reading.promise;
	await Promise.resolve();
	flushSync();

	expect(host.textContent).not.toContain('what version 2 held');
	expect(host.textContent).not.toContain('A version is read only');

	return unmount(component);
});
