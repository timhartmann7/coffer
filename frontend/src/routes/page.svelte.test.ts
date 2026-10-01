import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { typed } from '$lib/drafts';
import { entry, group, row } from '$lib/fixtures';
import { writing } from '$lib/focus.svelte';
import { applying } from '$lib/menu.svelte';
import type { Action, Command, Settings, Status } from '$lib/model';
import type { Stubbed } from '$lib/stubbed';
import Page from './+page.svelte';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('$lib/ipc'), async (real) =>
	Object.assign(ipc, (await import('$lib/stubbed')).stubbed(await real()))
);

const database = { path: '/Users/someone/personal.kdbx', name: 'personal' };

const locked: Status = {
	database,
	found: null,
	keyFile: null,
	unlocked: false,
	entries: 0,
	readOnly: false,
	rescue: null,
	lost: false,
	file: { there: true, written: null },
	copy: null,
	typed: false,
	typedBeside: false,
	lockedBy: null,
	locksIn: null
};

const chosen: Settings = {
	idleSeconds: 300,
	clipboardSeconds: 60,
	lockOnSleep: true,
	lockOnScreenLock: true,
	theme: 'system',
	idleChoices: [300],
	clipboardChoices: [60],
	themeChoices: ['system', 'dark', 'light']
};

const kept = row({ title: 'node-3', username: 'deploy' });
const root = group({ name: 'Root', entries: [kept] });

let host: HTMLElement;
let page: ReturnType<typeof mount> | null = null;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.status.mockResolvedValue(locked);
	ipc.settings.mockResolvedValue(chosen);
	ipc.listen.mockResolvedValue(undefined);
	ipc.menuState.mockResolvedValue(undefined);
	ipc.snapshots.mockResolvedValue([]);
	ipc.stirred.mockResolvedValue(null);
	ipc.closeWindow.mockResolvedValue(undefined);
});

afterEach(() => {
	if (page) void unmount(page);
	page = null;
	host.remove();
});

/** The window as it boots, once it has handed Rust its way in. */
async function boot(): Promise<(action: Action) => void> {
	page = mount(Page, { target: host });
	flushSync();
	await vi.waitFor(() => expect(ipc.listen).toHaveBeenCalledTimes(1));
	return ipc.listen.mock.calls[0][0] as (action: Action) => void;
}

/** What the menu bar was told last. */
function told(): Command[] {
	return ipc.menuState.mock.lastCall?.[0] ?? [];
}

/** The bar is told what applies only once Rust can deliver a choice to the
 * page: before that, an item it enabled would be a choice that waits. And on
 * the unlock screen there is nothing to lock. */
it('tells the menu bar what the unlock screen offers once it listens, and never Lock Vault', async () => {
	await boot();

	await vi.waitFor(() => expect(ipc.menuState).toHaveBeenCalled());
	expect(ipc.listen.mock.invocationCallOrder[0]).toBeLessThan(
		ipc.menuState.mock.invocationCallOrder[0]
	);
	expect(told()).toEqual(['settings', 'openVault', 'shortcuts']);
	expect(told()).not.toContain('lock');
});

/** What Rust hands the page is routed: an item of the menu bar is run by the
 * screen that answers it, and the close button closes the window - after what
 * is typed, and without running anything a screen answers. */
it('runs what Rust hands it: Keyboard Shortcuts opens the sheet, the close button closes', async () => {
	const heard = await boot();

	heard({ action: 'command', command: 'shortcuts' });
	flushSync();
	expect(host.querySelector('[role="dialog"]')?.textContent).toContain('Keyboard shortcuts');
	expect(host.querySelector('[inert]'), 'the screens under the sheet can still be used').not.toBe(
		null
	);

	document.activeElement?.dispatchEvent(
		new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
	);
	flushSync();
	expect(host.querySelector('[role="dialog"]')).toBeNull();
	expect(host.querySelector('[inert]')).toBeNull();

	heard({ action: 'closing' });
	await vi.waitFor(() => expect(ipc.closeWindow).toHaveBeenCalledTimes(1));
	expect(host.querySelector('[role="dialog"]'), 'the close ran a menu item').toBeNull();
});

/** With a vault open, Lock Vault applies, and Move to Recycle Bin for an open
 * entry that goes to the bin - until the focus goes into a field, where
 * Cmd+Backspace is the field's. The window's own focus events are what say so. */
it('greys out Move to Recycle Bin while the focus is in a field, from the window’s own focus', async () => {
	ipc.status.mockResolvedValue({ ...locked, unlocked: true, entries: 1 });
	ipc.tree.mockResolvedValue(root);
	ipc.entry.mockResolvedValue(entry({ id: kept.id, group: root.id, deletion: 'bin' }));
	ipc.versions.mockResolvedValue({ entry: kept.id, revision: 1, versions: [] });
	await boot();

	await vi.waitFor(() => expect(host.textContent).toContain('node-3'));
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('node-3'))
		?.click();
	await vi.waitFor(() => expect(told()).toContain('moveToBin'));
	expect(told()).toContain('lock');
	expect(told()).not.toContain('openVault');

	const search = host.querySelector<HTMLInputElement>('input');
	search?.focus();
	flushSync();
	expect(writing()).toBe(true);
	await vi.waitFor(() => expect(told()).not.toContain('moveToBin'));

	search?.blur();
	flushSync();
	expect(writing()).toBe(false);
	await vi.waitFor(() => expect(told()).toContain('moveToBin'));
});

/** The window as it boots with a vault open, its list drawn. */
async function unlocked(): Promise<(action: Action) => void> {
	ipc.status.mockResolvedValue({ ...locked, unlocked: true, entries: 1 });
	ipc.tree.mockResolvedValue(root);
	const heard = await boot();
	await vi.waitFor(() => expect(host.textContent).toContain('node-3'));
	return heard;
}

/** Lock Vault in the menu bar is the title bar's Lock button: what is being
 * typed reaches Rust before the lock is asked for, and the vault locks once.
 * With nothing open there is nothing to lock, and the choice does nothing -
 * a Cmd+L the reader believes locked a vault has to be one that did. */
it('locks from the menu the way the Lock button does, after what is typed', async () => {
	ipc.draft.mockResolvedValue(undefined);
	ipc.lock.mockResolvedValue(undefined);
	const heard = await unlocked();
	typed({ entry: kept.id, field: 'Notes', protect: false }, () => 'written before the lock');

	heard({ action: 'command', command: 'lock' });

	await vi.waitFor(() => expect(ipc.lock).toHaveBeenCalledTimes(1));
	expect(ipc.draft).toHaveBeenCalledWith(
		kept.id,
		'Notes',
		'written before the lock',
		false,
		false,
		expect.any(Number)
	);
	expect(ipc.draft.mock.invocationCallOrder[0]).toBeLessThan(ipc.lock.mock.invocationCallOrder[0]);
	await vi.waitFor(() => expect(host.textContent).not.toContain('node-3'));
	expect(applying()).not.toContain('lock');
});

it('locks nothing from the menu with no vault open', async () => {
	const heard = await boot();

	heard({ action: 'command', command: 'lock' });
	await new Promise((resolve) => setTimeout(resolve, 0));

	expect(ipc.lock).not.toHaveBeenCalled();
	expect(ipc.draft).not.toHaveBeenCalled();
});

/** Settings… with nothing open is the title bar's button. The menu only ever
 * opens the settings: a second choice that closed them would be a key that
 * undoes itself. */
it('opens the settings from the menu with no vault open, and keeps them open on a second', async () => {
	const heard = await boot();
	expect(applying()).toContain('settings');

	heard({ action: 'command', command: 'settings' });
	flushSync();
	expect(host.textContent).toContain('Open another');
	expect(applying()).not.toContain('settings');

	heard({ action: 'command', command: 'settings' });
	flushSync();
	expect(host.textContent, 'the second choice closed the settings').toContain('Open another');
});

/**
 * The reader closed the window and went to the menu bar for it. The choice
 * waited in Rust for the page the window was built for, and Rust hands it over
 * the moment that page listens - so the page listens only once its screens are
 * drawn, or the choice would arrive with nothing to answer it, and the window
 * the reader asked for would come up doing nothing. Run once, and only once.
 */
it('runs a choice that waited for the window once its screens are drawn', async () => {
	ipc.chooseDatabase.mockResolvedValue(null);
	ipc.listen.mockImplementation(async (heard: (action: Action) => void) => {
		heard({ action: 'command', command: 'openVault' });
	});

	await boot();

	await vi.waitFor(() => expect(ipc.chooseDatabase).toHaveBeenCalledTimes(1));
	await new Promise((resolve) => setTimeout(resolve, 0));
	expect(ipc.chooseDatabase, 'the choice was made twice').toHaveBeenCalledTimes(1);
});

it('opens the settings chosen with no window once the window is back', async () => {
	ipc.listen.mockImplementation(async (heard: (action: Action) => void) => {
		heard({ action: 'command', command: 'settings' });
	});

	await boot();

	await vi.waitFor(() => expect(host.textContent).toContain('Open another'));
});

/** A sheet over the window keeps every key to itself, and keys are how a
 * keyboard reader is there. Heard on their way in, before the sheet stops
 * them, they put the idle lock off as a press does - whether the key is aimed
 * inside the card or, after a press on the title bar, at nothing. */
it('counts a key pressed while the sheet of keys is up as the reader being there', async () => {
	const heard = await boot();
	heard({ action: 'command', command: 'shortcuts' });
	flushSync();
	const close = host.querySelector<HTMLElement>('[role="dialog"] button');
	expect(document.activeElement).toBe(close);
	const asked = ipc.stirred.mock.calls.length;
	const now = Date.now();
	const clock = vi.spyOn(Date, 'now');

	// Apart by more than the throttle on telling Rust, so each is told.
	clock.mockReturnValue(now + 20_000);
	close?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }));
	clock.mockReturnValue(now + 40_000);
	document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }));

	expect(ipc.stirred).toHaveBeenCalledTimes(asked + 2);
	expect(host.querySelector('[role="dialog"]'), 'a key put the sheet away').not.toBeNull();
});
