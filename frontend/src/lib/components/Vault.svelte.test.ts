import { createRawSnippet, flushSync, mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { entry, field, group, row } from '$lib/fixtures';
import Vault from './Vault.svelte';

const ipc = vi.hoisted(() => ({
	entry: vi.fn(),
	tree: vi.fn(),
	setField: vi.fn(),
	copy: vi.fn(),
	createEntry: vi.fn(),
	createGroup: vi.fn(),
	deleteEntry: vi.fn(),
	deleteGroup: vi.fn(),
	renameGroup: vi.fn(),
	emptyRecycleBin: vi.fn(),
	versions: vi.fn(),
	version: vi.fn(),
	revealVersion: vi.fn(),
	restoreVersion: vi.fn(),
	deleteVersion: vi.fn(),
	clearHistory: vi.fn(),
	reveal: vi.fn(),
	openUrl: vi.fn(),
	removeField: vi.fn(),
	setTags: vi.fn(),
	addAttachment: vi.fn(),
	exportAttachment: vi.fn(),
	removeAttachment: vi.fn(),
	generatePassword: vi.fn(),
	save: vi.fn(),
	saveOver: vi.fn(),
	saveCopy: vi.fn(),
	reload: vi.fn(),
	rival: vi.fn(),
	// The same reading the real one does: a command rejects with the value Rust
	// serialised, and anything else is not one.
	asFailure: (thrown: unknown) =>
		thrown && typeof (thrown as { message?: unknown }).message === 'string'
			? (thrown as { code: string; message: string })
			: { code: 'other', message: 'Coffer could not finish that.' }
}));
vi.mock('$lib/ipc', () => ipc);

const database = { path: '/Users/someone/personal.kdbx', name: 'personal' };

const kept = row({ title: 'node-3', username: 'deploy', tags: ['prod'] });
const other = row({ title: 'Postgres', username: 'svc_app' });
const deleted = row({ title: 'thrown away' });

const root = group({
	name: 'Root',
	entries: [kept],
	sections: [
		group({ name: 'Work', entries: [other] }),
		group({ name: 'Recycle Bin', isRecycleBin: true, entries: [deleted] })
	]
});

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.copy.mockResolvedValue(60);
	ipc.versions.mockResolvedValue([]);
	ipc.save.mockResolvedValue(undefined);
	ipc.tree.mockResolvedValue(root);
});

afterEach(() => {
	host.remove();
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
 * open, and they never touch the value themselves. */
it('copies the open entry through Rust on the keyboard', async () => {
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
	await vi.waitFor(() => expect(ipc.copy).toHaveBeenCalledWith(kept.id, 'Password'));

	window.dispatchEvent(new KeyboardEvent('keydown', { key: 'b', metaKey: true }));
	await vi.waitFor(() => expect(ipc.copy).toHaveBeenCalledWith(kept.id, 'UserName'));

	await tick();
	expect(reads()).toContain('Copied. The clipboard clears in 1 minute.');

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
	await vi.waitFor(() => expect(ipc.copy).toHaveBeenCalledWith(kept.id, 'Password'));

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
		ipc.versions.mockResolvedValue([]);
		ipc.save.mockResolvedValue(undefined);
		window.dispatchEvent(new KeyboardEvent('keydown', { key: 'c', metaKey: true }));
		await vi.advanceTimersByTimeAsync(0);
		flushSync();
		expect(toast()?.textContent).toContain('Copied. The clipboard clears in 1 minute.');
		expect(toast()?.querySelector('use[href="#i-copy"]')).not.toBeNull();

		await vi.advanceTimersByTimeAsync(4_000);
		flushSync();
		expect(toast()?.textContent).toContain('Copied. The clipboard clears in 1 minute.');

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

it('puts the reader in the search field on the shortcut the field advertises', async () => {
	const component = open();
	flushSync();

	expect(document.activeElement).not.toBe(search());
	window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', metaKey: true }));
	await tick();

	expect(document.activeElement).toBe(search());

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
	title.value = 'node-4';
	title.dispatchEvent(new Event('blur'));

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
	ipc.versions.mockResolvedValue([]);

	const onTree = vi.fn();
	const component = open({ onTree });
	flushSync();

	// Making an entry is a change like any other, and it saves.
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Entry')
		?.click();
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

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Entry')
		?.click();
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
	ipc.versions.mockResolvedValue([{ index: 0, modified: '2021-06-02T12:00:00Z' }]);
	ipc.deleteVersion.mockResolvedValue([]);

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
	title.value = 'node-4';
	title.dispatchEvent(new Event('blur'));

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

	window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f', metaKey: true, bubbles: true }));
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
