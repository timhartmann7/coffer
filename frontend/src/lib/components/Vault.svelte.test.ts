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
	putBackEntry: vi.fn(),
	putBackGroup: vi.fn(),
	renameGroup: vi.fn(),
	emptyRecycleBin: vi.fn(),
	versions: vi.fn(),
	version: vi.fn(),
	revealVersion: vi.fn(),
	restoreVersion: vi.fn(),
	beforeRemoval: vi.fn(),
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
		// A folder with a folder in it, so the tree draws the chevron that expands
		// without selecting. That press is the one the dismiss guard is about.
		group({ name: 'Work', entries: [other], sections: [group({ name: 'Clients' })] }),
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
	await vi.waitFor(() => expect(host.textContent).toContain('not there any more'));
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Keep it elsewhere')
		?.click();
	await vi.waitFor(() => expect(ipc.saveCopy).toHaveBeenCalledTimes(1));
	flushSync();

	expect(ipc.reload, 'it read back a file that is not there').not.toHaveBeenCalled();
	expect(host.textContent).toContain('Kept as rescued');

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
	title.value = 'node-4';
	title.dispatchEvent(new Event('blur'));

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

/**
 * Opens node-3, takes its PIN off, and waits for whatever the window then says.
 * `answer` is what Rust says about the version the removal wrote.
 */
async function removePin(answer: number | null) {
	ipc.entry.mockImplementation((id: string) => Promise.resolve(withPin(id)));
	ipc.removeField.mockResolvedValue(
		entry({
			id: kept.id,
			group: root.id,
			fields: withPin(kept.id).fields.filter((each) => each.name !== 'PIN')
		})
	);
	ipc.restoreVersion.mockResolvedValue(withPin(kept.id));
	ipc.beforeRemoval.mockReset();
	ipc.beforeRemoval.mockResolvedValue(answer);

	const component = open();
	flushSync();
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('node-3'))
		?.click();
	await settled();

	host.querySelector<HTMLButtonElement>('[aria-label="Remove the field PIN"]')?.click();
	await settled();
	return component;
}

/**
 * A field of the reader's own used to go on one press with nothing said, and the
 * way back was a version nobody was told about. The notice says what went and
 * offers it back, and taking it back is a restore of exactly the version Rust
 * names - asked once the save is done, because the save is what can prune it,
 * and asked again at the press, because a position is only an answer for the
 * history as it stood.
 */
it('offers a removed field back and restores the version the removal wrote', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin(3);

		expect(toast()?.textContent).toContain('Field “PIN” removed');
		expect(undo()?.textContent).toContain('Undo');
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(ipc.beforeRemoval).toHaveBeenCalledWith(kept.id, 'PIN');
		expect(
			ipc.save.mock.invocationCallOrder[0],
			'Rust was asked before the save that can prune the version'
		).toBeLessThan(ipc.beforeRemoval.mock.invocationCallOrder[0]);

		// Heard as well as seen, in a region that was there before the words.
		const region = host.querySelector('[role="status"]');
		expect(region?.getAttribute('aria-live')).toBe('polite');
		expect(region?.textContent).toContain('Field “PIN” removed');

		undo()?.click();
		await settled();

		expect(ipc.beforeRemoval).toHaveBeenCalledTimes(2);
		expect(ipc.restoreVersion).toHaveBeenCalledTimes(1);
		expect(ipc.restoreVersion).toHaveBeenCalledWith(kept.id, 3);
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
		const component = await removePin(3);

		await vi.advanceTimersByTimeAsync(7_900);
		flushSync();
		expect(undo(), 'the offer went early').not.toBeNull();

		await vi.advanceTimersByTimeAsync(100);
		flushSync();
		expect(press('z').defaultPrevented).toBe(false);
		await settled();
		expect(ipc.restoreVersion).not.toHaveBeenCalled();

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
		const component = await removePin(3);

		const typed = press('z', search());
		await settled();
		expect(typed.defaultPrevented, 'the search field lost its own undo').toBe(false);
		expect(ipc.restoreVersion).not.toHaveBeenCalled();

		// Redo is not undo.
		press('z', window, true);
		await settled();
		expect(ipc.restoreVersion).not.toHaveBeenCalled();

		expect(press('z').defaultPrevented).toBe(true);
		await settled();
		expect(ipc.restoreVersion).toHaveBeenCalledWith(kept.id, 3);

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** The button and the key, and the button twice, are one undo. A second would
 * restore the version the first one just wrote. */
it('runs an undo once however many ways it is asked for', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin(3);

		const button = undo();
		button?.click();
		press('z');
		button?.click();
		await settled();
		press('z');
		await settled();

		expect(ipc.restoreVersion).toHaveBeenCalledTimes(1);

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/**
 * With no versions kept, or a size limit the version does not fit, the save
 * after the removal prunes the version it wrote. Whatever is newest after that
 * is older, and restoring it would take back more than the field. The removal
 * is still reported; nothing is offered.
 */
it('offers nothing back when the save pruned the version', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin(null);

		expect(toast()?.textContent).toContain('Field “PIN” removed');
		expect(undo()).toBeNull();

		expect(press('z').defaultPrevented).toBe(false);
		await settled();
		expect(ipc.restoreVersion).not.toHaveBeenCalled();
		expect(ipc.beforeRemoval).toHaveBeenCalledTimes(1);

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** An undo is of something on the screen. Another entry in the pane is a
 * reader who has moved on, and Cmd+Z there would reach back into one they left. */
it('withdraws the offer when another entry is opened', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin(3);

		[...host.querySelectorAll('button')]
			.find((each) => each.textContent?.includes('Postgres'))
			?.click();
		await settled();
		expect(ipc.entry).toHaveBeenLastCalledWith(other.id);

		await vi.advanceTimersByTimeAsync(1_000);
		flushSync();
		expect(toast()).toBeNull();
		press('z');
		await settled();
		expect(ipc.restoreVersion).not.toHaveBeenCalled();

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** Putting the pane away is moving on as well. */
it('withdraws the offer when the entry is put away', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin(3);

		window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
		await settled();
		press('z');
		await settled();
		expect(ipc.restoreVersion).not.toHaveBeenCalled();

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
		const component = await removePin(3);

		press('c');
		await settled();
		expect(toast()?.textContent).toContain('Copied.');
		expect(undo()).toBeNull();

		press('z');
		await settled();
		expect(ipc.restoreVersion).not.toHaveBeenCalled();

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** Another change that reaches the file is newer than the removal. Restoring
 * the removal's version after it would take that change back as well. */
it('withdraws the offer when another change reaches the file', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin(3);
		ipc.setField.mockResolvedValue(withPin(kept.id));

		const title = host.querySelector('h1 input') as HTMLInputElement;
		title.value = 'node-4';
		title.dispatchEvent(new Event('blur'));
		await settled();
		expect(ipc.save).toHaveBeenCalledTimes(2);

		press('z');
		await settled();
		expect(ipc.restoreVersion).not.toHaveBeenCalled();

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
		const component = await removePin(3);

		expect(toast()?.textContent).toContain('No space left on device');
		expect(undo()).toBeNull();
		expect(ipc.beforeRemoval).not.toHaveBeenCalled();
		expect(reads()).toContain('Not saved');

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** Rust is asked again at the press. A history that moved in between gets a
 * sentence, and no restore of whatever now sits at the old position. */
it('restores nothing when the history moved before the undo reached it', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin(3);
		ipc.beforeRemoval.mockResolvedValue(null);

		undo()?.click();
		await settled();

		expect(ipc.restoreVersion).not.toHaveBeenCalled();
		expect(toast()?.textContent).toContain('can no longer be undone');

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** A lock takes the window down, and the offer and its clock with it. */
it('leaves nothing to undo once the window is gone', async () => {
	vi.useFakeTimers();
	try {
		const component = await removePin(3);
		await unmount(component);

		press('z');
		await vi.advanceTimersByTimeAsync(10_000);
		expect(ipc.restoreVersion).not.toHaveBeenCalled();
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
			{ ...work, binned: { since: '2026-08-29T14:00:00Z', from: root.id }, deletion: 'forever' }
		]
	});
	ipc.deleteGroup.mockResolvedValue(group({ ...root, sections: [binned] }));
	ipc.putBackGroup.mockResolvedValue(root);

	const component = open();
	flushSync();
	[...host.querySelectorAll('button')].find((each) => each.textContent?.includes('Work'))?.click();
	flushSync();

	const trash = () => host.querySelector<HTMLButtonElement>('[aria-label="Delete this folder"]');
	trash()?.click();
	flushSync();
	expect(host.querySelector('[data-confirm]')?.textContent).toContain(
		'Move “Work” and everything in it to the Recycle Bin?'
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
	await vi.waitFor(() => expect(ipc.deleteGroup).toHaveBeenCalledWith(work.id));
	await vi.waitFor(() => expect(ipc.save).toHaveBeenCalledTimes(1));
	await vi.waitFor(() => expect(toast()?.textContent).toContain('Moved “Work” to the Recycle Bin'));

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
	const card = row({
		title: 'Visa',
		binned: { since: '2026-08-26T09:00:00Z', from: null }
	});
	const banking = group({
		name: 'Banking',
		binned: { since: '2026-08-26T09:00:00Z', from: personal.id },
		deletion: 'forever',
		entries: [card]
	});
	const mail = row({
		title: 'Old mail',
		binned: { since: '2026-08-28T10:00:00Z', from: work.id }
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
	const component = mount(Vault, {
		target: host,
		props: { database, root: tree, readOnly, onSettings: vi.fn(), onTree }
	});
	flushSync();
	return { component, onTree };
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
		expect(folders[0].textContent).toContain('Deleted 3 days ago · from “Personal”');
		expect(reads()).toContain('Old mail');
		expect(reads()).toContain('Deleted yesterday · from “Work”');
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
			'In the Recycle Bin since 26 Aug · was in “Personal”'
		);
		expect(reads()).toContain('Visa');
		expect(reads()).toContain('Deleted 3 days ago');
		expect(reads(), 'emptying is for the whole bin, not a folder in it').not.toContain(
			'Empty the bin'
		);
		expect(host.querySelector('[aria-label="Delete this folder"]')).toBeNull();
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
				entries: [{ ...bank, binned: { since: '2026-08-29T14:30:00Z', from: before.id } }]
			})
		]
	});
	ipc.entry.mockResolvedValue(titled(bank.id, 'node-3'));
	ipc.deleteEntry.mockResolvedValue(after);
	ipc.putBackEntry.mockResolvedValue(before);
	ipc.save.mockResolvedValue(undefined);

	const { component, onTree } = mounted(before);
	try {
		pressed('node-3');
		await settled();
		ipc.entry.mockClear();

		pressed('Move to Recycle Bin');
		await settled();

		expect(ipc.deleteEntry).toHaveBeenCalledWith(bank.id);
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(onTree).toHaveBeenLastCalledWith(after);
		expect(host.querySelector('h1'), 'the pane stayed open on a deleted entry').toBeNull();
		expect(toast()?.textContent).toContain('Moved “node-3” to the Recycle Bin');
		expect(undo()?.textContent).toContain('Undo');

		expect(press('z').defaultPrevented).toBe(true);
		press('z');
		undo()?.click();
		await settled();

		expect(ipc.putBackEntry).toHaveBeenCalledTimes(1);
		expect(ipc.putBackEntry).toHaveBeenCalledWith(bank.id);
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
		entries: [{ ...bank, binned: { since: null, from: before.id } }]
	});
	ipc.entry.mockResolvedValue(titled(bank.id, 'node-3'));
	ipc.deleteEntry.mockResolvedValue(group({ ...before, entries: [], sections: [bin] }));

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
		expect(ipc.putBackEntry).not.toHaveBeenCalled();
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
	ipc.deleteEntry.mockResolvedValue(group({ ...before, entries: [] }));

	const { component } = mounted(before);
	try {
		pressed('node-3');
		await settled();
		pressed('Move to Recycle Bin');
		await settled();

		expect(toast()?.textContent).toContain('Deleted “node-3” forever');
		expect(undo()).toBeNull();
		expect(press('z').defaultPrevented).toBe(false);
		await settled();
		expect(ipc.putBackEntry).not.toHaveBeenCalled();
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
		entries: [{ ...bank, binned: { since: null, from: before.id } }]
	});
	ipc.entry.mockResolvedValue(titled(bank.id, 'node-3'));
	ipc.deleteEntry.mockResolvedValue(group({ ...before, entries: [], sections: [bin] }));
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
	ipc.putBackEntry.mockResolvedValue(vault.tree);

	const { component } = mounted(vault.tree);
	try {
		pressed('Recycle Bin');
		flushSync();
		pressed('Old mail');
		await settled();

		expect(entryCard()?.textContent).toContain('In the Recycle Bin since 28 Aug · was in “Work”');
		expect(host.querySelector('section h1 input'), 'the title can be written into').toBeNull();

		ipc.entry.mockResolvedValue(home);
		pressed('Put back', entryCard() ?? host);
		await settled();
		expect(ipc.putBackEntry).toHaveBeenCalledWith(vault.mail.id);
		expect(ipc.save).toHaveBeenCalledTimes(1);
		expect(entryCard(), 'the pane still says the entry is in the bin').toBeNull();
		expect(host.querySelector('section h1 input')).not.toBeNull();

		// And the other way out, from the bin again.
		ipc.entry.mockResolvedValue(inBin);
		pressed('Old mail');
		await settled();
		ipc.deleteEntry.mockResolvedValue(
			group({
				...vault.tree,
				sections: [vault.personal, vault.work, { ...vault.bin, entries: [] }]
			})
		);
		pressed('Delete forever…', entryCard() ?? host);
		flushSync();
		expect(host.querySelector('[data-confirm]')?.textContent).toContain(
			'Delete “Old mail” forever? This can’t be undone.'
		);
		expect(ipc.deleteEntry).not.toHaveBeenCalled();
		pressed('Delete forever', host.querySelector('[data-confirm]') ?? host);
		await settled();

		expect(ipc.deleteEntry).toHaveBeenCalledWith(vault.mail.id);
		expect(toast()?.textContent).toContain('Deleted “Old mail” forever');
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
			'Delete “Banking” and everything in it forever? This can’t be undone.'
		);
		pressed('Delete forever', host.querySelector('[data-confirm]') ?? host);
		await settled();

		expect(ipc.deleteGroup).toHaveBeenCalledWith(vault.banking.id);
		expect(toast()?.textContent).toContain('Deleted “Banking” forever');
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
			'Delete “Work” and everything in it forever? This can’t be undone.'
		);
		pressed('Delete forever', host.querySelector('[data-confirm]') ?? host);
		await settled();

		expect(ipc.deleteGroup).toHaveBeenCalledWith(work.id);
		expect(toast()?.textContent).toContain('Deleted “Work” forever');
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

		expect(folderCard()?.textContent).toContain('was in “Personal”');
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
		binned: { since: null, from: from.id },
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
		expect(host.querySelector('[data-folder]')?.textContent).toContain(`Deleted from “${hostile}”`);
		(host.querySelector('[data-folder]') as HTMLElement).click();
		flushSync();
		expect(folderCard()?.textContent).toContain(`In the Recycle Bin · was in “${hostile}”`);
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
		entries: [{ ...bank, binned: { since: null, from: before.id } }]
	});
	ipc.entry.mockResolvedValue(titled(bank.id, 'node-3'));
	ipc.deleteEntry.mockResolvedValue(group({ ...before, entries: [], sections: [bin] }));

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

		expect(ipc.deleteEntry).toHaveBeenCalledTimes(1);
		expect(toast()?.textContent).toContain('Moved “node-3” to the Recycle Bin');
	} finally {
		await unmount(component);
		vi.useRealTimers();
	}
});
