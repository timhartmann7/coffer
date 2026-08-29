import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { entry, field, version } from '$lib/fixtures';
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

afterEach(() => host.remove());

const listed = [
	version({ index: 0, modified: '2021-06-02T12:00:00Z' }),
	version({ index: 1, modified: '2022-06-03T12:00:00Z' }),
	version({ index: 2, modified: '2023-06-04T12:00:00Z' })
];

function show(props: Record<string, unknown> = {}) {
	return mount(Versions, {
		target: host,
		props: {
			entry: 'an-entry',
			versions: listed,
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
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
	const component = show({ versions: [] });
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
	ipc.deleteVersion.mockResolvedValue([]);

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
	await vi.waitFor(() => expect(ipc.deleteVersion).toHaveBeenCalledWith('an-entry', 0));
	expect(onVersions).toHaveBeenCalledWith([]);

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
	ipc.clearHistory.mockResolvedValue([]);

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
	expect(onVersions).toHaveBeenCalledWith([]);

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
 * A version is addressed by its position, and dropping one moves every position
 * after it. A panel left open on a number that now names a different version
 * would show one version's fields and reveal another's values.
 */
it('closes an open version when the list underneath it changes', async () => {
	ipc.version.mockResolvedValue(entry({ fields: [field({ name: 'Title', value: 'was' })] }));
	ipc.deleteVersion.mockResolvedValue([listed[0], listed[1]]);

	const component = show();
	open();

	button('View').click();
	await vi.waitFor(() => expect(host.textContent).toContain('A version is read only'));
	flushSync();

	host.querySelectorAll<HTMLButtonElement>('[aria-label="Delete this version"]')[2]?.click();
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
