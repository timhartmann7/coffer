import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { typed } from '$lib/drafts';
import { entry, group, kinds, row } from '$lib/fixtures';
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
	readOnlyBecause: null,
	copyable: false,
	elsewhere: null,
	rescue: null,
	lost: false,
	file: { there: true, written: null },
	copy: null,
	snapshot: null,
	typed: false,
	typedBeside: false,
	rekeyed: false,
	adopted: null,
	backupGone: false,
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
	ipc.kinds.mockResolvedValue(kinds());
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
 * the unlock screen there is nothing to lock, and nothing open to copy; its
 * file can still be shown, to be carried somewhere by hand. */
it('tells the menu bar what the unlock screen offers once it listens, and never Lock Vault', async () => {
	await boot();

	await vi.waitFor(() => expect(ipc.menuState).toHaveBeenCalled());
	expect(ipc.listen.mock.invocationCallOrder[0]).toBeLessThan(
		ipc.menuState.mock.invocationCallOrder[0]
	);
	expect(told()).toEqual(['settings', 'openVault', 'showInFinder', 'shortcuts']);
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

/** Opens the settings the way Settings… in the menu bar does, and answers
 * whether they offer to change the master password. */
async function offersRekey(heard: (action: Action) => void): Promise<boolean> {
	heard({ action: 'command', command: 'settings' });
	flushSync();
	await vi.waitFor(() => expect(host.textContent).toContain('Automatic backups'));
	return [...host.querySelectorAll('button')].some(
		(each) => each.textContent?.trim() === 'Change…'
	);
}

/**
 * The row is the vault screen's to hand and the settings' to draw, and the
 * page is what joins the two: a page that stopped passing on what the vault
 * screen handed would leave the settings with no row and every other test
 * green.
 */
it('offers to change the master password in the settings of a vault it can write', async () => {
	const heard = await unlocked();

	expect(await offersRekey(heard), 'the row did not reach the settings').toBe(true);
	expect(host.textContent).toContain('Master password');
});

it('offers no new master password for a vault Coffer does not write', async () => {
	ipc.status.mockResolvedValue({
		...locked,
		unlocked: true,
		entries: 1,
		readOnly: true,
		readOnlyBecause: 'kdb'
	});
	ipc.tree.mockResolvedValue(root);
	const heard = await boot();
	await vi.waitFor(() => expect(host.textContent).toContain('node-3'));

	expect(await offersRekey(heard)).toBe(false);
	expect(host.textContent).not.toContain('Master password');
});

it('offers no new master password with no vault open', async () => {
	const heard = await boot();

	expect(await offersRekey(heard)).toBe(false);
	expect(host.textContent).not.toContain('Master password');
});

/** The copy a lock left beside the vault opens with the vault's password, and
 * would bring it back if it were made the vault after a change. Rust says it
 * is there, and the row says what to do with it instead of offering a change
 * Rust would refuse. */
it('says why a new master password waits while a lock’s copy sits beside the vault', async () => {
	ipc.status.mockResolvedValue({
		...locked,
		unlocked: true,
		entries: 1,
		rescue: { name: 'personal.kdbx.unsaved.kdbx', written: null }
	});
	ipc.tree.mockResolvedValue(root);
	const heard = await boot();
	await vi.waitFor(() => expect(host.textContent).toContain('node-3'));

	expect(await offersRekey(heard)).toBe(false);
	expect(host.textContent).toContain('Master password');
	expect(host.textContent).toContain('Not while the copy a lock left sits beside this vault');
});

/** The settings that said the password changed went with the lock, and the
 * window the lock builds again knows nothing of it. What Rust kept reaches the
 * unlock screen through the page. */
it('tells the unlock screen the master password changed before the lock', async () => {
	ipc.status.mockResolvedValue({ ...locked, rekeyed: true });
	await boot();

	await vi.waitFor(() =>
		expect(host.textContent).toContain('The master password was changed before locking')
	);
});

/** A backup made the vault, and a lock took the window before its notice
 * could say what became of the file it replaced. The unlock screen says it
 * instead, with the password the vault opens with now. */
it('tells the unlock screen a backup was made the vault before the lock', async () => {
	ipc.status.mockResolvedValue({
		...locked,
		adopted: {
			database,
			keptAs: null,
			setAside: 'personal.kdbx.replaced-2026-10-01.kdbx'
		}
	});
	await boot();

	await vi.waitFor(() =>
		expect(host.textContent).toContain(
			'A backup was made your vault before locking, and it opens with the password that backup opened with.'
		)
	);
	expect(host.textContent).toContain('personal.kdbx.replaced-2026-10-01.kdbx');
});

/** "Open to look" from the settings, and the lock's own save pushed that
 * backup out on the way. The window comes back on the vault's unlock screen,
 * and says why it is not the backup's. */
it('tells the unlock screen the backup asked for was pushed out on the way', async () => {
	ipc.status.mockResolvedValue({ ...locked, backupGone: true });
	await boot();

	await vi.waitFor(() =>
		expect(host.textContent).toContain('The backup you asked to look at is not there any more')
	);
	expect(host.textContent).toContain('This is your vault.');
});

/** A lock's copy can be written, and is not the vault: a new password there
 * would leave the vault and its snapshots opening with the old one. The banner
 * that says it is the copy is up, and is the way to make it the vault. */
it('offers no new master password inside a lock’s copy', async () => {
	ipc.status.mockResolvedValue({
		...locked,
		unlocked: true,
		entries: 1,
		copy: {
			vault: 'personal.kdbx',
			saved: null,
			keptAs: 'personal.kdbx.1.bak',
			vaultFile: { there: true, written: null }
		}
	});
	ipc.tree.mockResolvedValue(root);
	const heard = await boot();
	await vi.waitFor(() => expect(host.textContent).toContain('Make this my vault'));

	expect(await offersRekey(heard)).toBe(false);
	expect(host.textContent).not.toContain('Master password');
	expect(host.textContent, 'the way to make it the vault went').toContain('Make this my vault');
});

/** A backup open, as Rust says it: read only for being a backup, copyable, and
 * the vault beside it as it stands. */
const looking: Status = {
	...locked,
	database: { path: '/Users/someone/personal.kdbx.2.bak', name: 'personal.kdbx.2' },
	unlocked: true,
	entries: 1,
	readOnly: true,
	readOnlyBecause: 'snapshot',
	copyable: true,
	snapshot: {
		vault: 'personal.kdbx',
		taken: '2026-08-27T18:40:00Z',
		keptAs: 'personal.kdbx.1.bak',
		vaultFile: { there: true, written: null },
		because: 'unopened'
	}
};

/**
 * A backup open in place of a vault that would not open. The strip over it
 * says so, and is the way to make it the vault; afterwards the strip goes, the
 * status bar no longer says read only, and the notice says what became of the
 * file it replaced - read from Rust, which made the decision.
 */
it('puts the strip over an open backup, and says what became of the file it replaced', async () => {
	ipc.status.mockResolvedValue(looking);
	ipc.tree.mockResolvedValue(root);
	await boot();
	await vi.waitFor(() =>
		expect(host.textContent).toContain('Your vault file could not be opened.')
	);
	expect(host.querySelector('button[aria-controls="read-only-note"]')).not.toBeNull();

	ipc.adoptSnapshot.mockResolvedValue({
		database,
		keptAs: null,
		setAside: 'personal.kdbx.replaced-2026-10-01.kdbx'
	});
	ipc.status.mockResolvedValue({ ...locked, unlocked: true, entries: 1 });
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Use this copy as my vault')
		?.click();

	await vi.waitFor(() =>
		expect(host.querySelector('[data-notice]')?.textContent).toContain(
			'did not open with this password and is kept as'
		)
	);
	expect(ipc.adoptSnapshot).toHaveBeenCalledTimes(1);
	expect(host.textContent).not.toContain('Use this copy as my vault');
	expect(host.querySelector('button[aria-controls="read-only-note"]')).toBeNull();
});

/** A press Rust refused is said on the strip, which reads the vault's file
 * again first: the window never decides on its own that the backup won. */
it('keeps the strip and reads the vault file again when making a backup the vault is refused', async () => {
	ipc.status.mockResolvedValue(looking);
	ipc.tree.mockResolvedValue(root);
	await boot();
	await vi.waitFor(() => expect(host.textContent).toContain('Use this copy as my vault'));
	const asked = ipc.status.mock.calls.length;

	ipc.adoptSnapshot.mockRejectedValue({
		code: 'externalChange',
		message: 'the vault file is not as it stood when it was shown'
	});
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Use this copy as my vault')
		?.click();

	await vi.waitFor(() => expect(host.textContent).toContain('so nothing was replaced'));
	expect(ipc.status.mock.calls.length).toBeGreaterThan(asked);
	expect(host.querySelector('[data-notice]')).toBeNull();
});

/**
 * "Open to look" in the settings of an open vault. What is still being typed
 * reaches Rust before the backup is asked for - Rust locks the vault on the
 * way, and that lock writes it - and the screen is the backup's unlock screen
 * once Rust has answered.
 */
it('opens a backup from the settings after what is typed, and shows its unlock screen', async () => {
	ipc.draft.mockResolvedValue(undefined);
	ipc.snapshots.mockResolvedValue([
		{ name: 'personal.kdbx.1.bak', index: 1, taken: '2026-08-27T18:40:00Z' },
		{ name: 'personal.kdbx.2.bak', index: 2, taken: '2026-08-27T18:31:00Z' }
	]);
	ipc.chooseSnapshot.mockResolvedValue(looking.database);
	const heard = await unlocked();
	typed({ entry: kept.id, field: 'Notes', protect: false }, () => 'written before the look');

	heard({ action: 'command', command: 'settings' });
	flushSync();
	await vi.waitFor(() => expect(host.textContent).toContain('2 copies'));
	[...host.querySelectorAll('button')].find((each) => each.textContent?.trim() === 'Show')?.click();
	flushSync();

	ipc.status.mockResolvedValue({
		...locked,
		database: looking.database,
		snapshot: looking.snapshot
	});
	host.querySelector<HTMLButtonElement>('[aria-label="Open to look personal.kdbx.2.bak"]')?.click();

	await vi.waitFor(() => expect(ipc.chooseSnapshot).toHaveBeenCalledWith(2));
	expect(ipc.draft.mock.invocationCallOrder[0]).toBeLessThan(
		ipc.chooseSnapshot.mock.invocationCallOrder[0]
	);
	await vi.waitFor(() => expect(host.textContent).toContain('A backup, not your vault'));
	expect(host.textContent).not.toContain('node-3');
});

/**
 * What Rust knows of copies on another disk reaches the vault's status bar and
 * the settings through the page, and a copy saved from either is what both say
 * next: the reminder goes, and the row says when and where. The page is what
 * joins the three, and a page that dropped Rust's answer would leave the
 * reminder up after the copy it asked for.
 */
it('carries what is known of copies between the status bar and the settings', async () => {
	const due = { otherDisk: null, sameDiskAt: null, overdue: 34 };
	ipc.status.mockResolvedValue({ ...locked, unlocked: true, entries: 1, elsewhere: due });
	ipc.tree.mockResolvedValue(root);
	ipc.copyVault.mockResolvedValue({
		sameDisk: false,
		volume: 'Stick',
		elsewhere: {
			otherDisk: { at: new Date().toISOString(), volume: 'Stick' },
			sameDiskAt: null,
			overdue: null
		}
	});
	const heard = await boot();
	await vi.waitFor(() => expect(host.textContent).toContain('node-3'));
	expect(host.textContent).toContain('No copy on another disk for 34 days');

	heard({ action: 'command', command: 'settings' });
	flushSync();
	await vi.waitFor(() => expect(host.textContent).toContain('Copy on another disk'));
	expect(host.textContent).toContain('Last made: never');

	heard({ action: 'command', command: 'saveCopy' });
	await vi.waitFor(() =>
		expect(host.textContent).toContain('Last made: today, on “\u2068Stick\u2069”')
	);
	expect(ipc.copyVault).toHaveBeenCalledTimes(1);
	expect(host.textContent).not.toContain('No copy on another disk');
});

/** A copy asked for from the vault's screen before the settings opened is
 * still on its way when they are drawn over it. The page hands the settings
 * the vault screen's answer, so their button says so and takes no press, and
 * the menu bar's item is grey, rather than both offering a copy that would do
 * nothing. */
it('tells the settings a copy asked for before they opened is on its way', async () => {
	const due = { otherDisk: null, sameDiskAt: null, overdue: 34 };
	ipc.status.mockResolvedValue({ ...locked, unlocked: true, entries: 1, elsewhere: due });
	ipc.tree.mockResolvedValue(root);
	let finish: (made: null) => void = () => {};
	ipc.copyVault.mockReturnValue(new Promise((resolve) => (finish = resolve)));
	const heard = await boot();
	await vi.waitFor(() => expect(host.textContent).toContain('node-3'));

	heard({ action: 'command', command: 'saveCopy' });
	await vi.waitFor(() => expect(ipc.copyVault).toHaveBeenCalledTimes(1));
	heard({ action: 'command', command: 'settings' });
	flushSync();
	await vi.waitFor(() => expect(host.textContent).toContain('Copy on another disk'));

	const button = () =>
		[...host.querySelectorAll('button')].find((each) => each.textContent?.includes('a copy'));
	expect(button()?.textContent?.trim()).toBe('Saving a copy…');
	expect(button()?.disabled).toBe(true);
	await vi.waitFor(() => expect(told()).not.toContain('saveCopy'));

	finish(null);
	await vi.waitFor(() => expect(button()?.textContent?.trim()).toBe('Save a copy…'));
	await vi.waitFor(() => expect(told()).toContain('saveCopy'));
	expect(ipc.copyVault).toHaveBeenCalledTimes(1);
});

/** The unlock screen says nothing of copies: `SPEC.md` section 8 ends it with
 * "Nothing else". Whatever Rust says of them while the vault is locked - an
 * older Rust, a status that crossed a lock - draws no reminder there, no row
 * in the settings opened over it, and no Save a Copy… in the menu bar. */
it('says nothing of copies on the unlock screen, whatever the status says', async () => {
	ipc.status.mockResolvedValue({
		...locked,
		unlocked: false,
		elsewhere: { otherDisk: null, sameDiskAt: '2026-09-01T00:00:00Z', overdue: 400 }
	});
	const heard = await boot();
	await vi.waitFor(() => expect(host.querySelector('form')).not.toBeNull());
	await vi.waitFor(() => expect(ipc.menuState).toHaveBeenCalled());

	expect(host.textContent).not.toContain('No copy on another disk');
	expect(host.textContent).not.toContain('Copy on another disk');
	expect(host.textContent).not.toContain('Show in Finder');
	expect(told()).not.toContain('saveCopy');

	heard({ action: 'command', command: 'settings' });
	flushSync();
	await vi.waitFor(() => expect(host.textContent).toContain('Open another'));
	expect(host.textContent).not.toContain('Copy on another disk');
	expect(host.textContent).not.toContain('same disk as the vault');
	expect(told()).not.toContain('saveCopy');
	expect(ipc.copyVault).not.toHaveBeenCalled();
});
