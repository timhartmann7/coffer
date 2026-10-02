import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Elsewhere, Settings as Chosen } from '$lib/model';
import { applying, run } from '$lib/menu.svelte';
import { reactive } from '$lib/props.svelte';
import type { Stubbed } from '$lib/stubbed';
import Settings from './Settings.svelte';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('$lib/ipc'), async (real) =>
	Object.assign(ipc, (await import('$lib/stubbed')).stubbed(await real()))
);

let host: HTMLElement;

const CHOSEN: Chosen = {
	idleSeconds: 300,
	clipboardSeconds: 60,
	lockOnSleep: true,
	lockOnScreenLock: true,
	theme: 'dark',
	idleChoices: [60, 300, 900, 1800, 3600],
	clipboardChoices: [15, 30, 60, 300],
	themeChoices: ['system', 'dark', 'light']
};

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.setSettings.mockImplementation((wanted: Chosen) => Promise.resolve(wanted));
	ipc.snapshots.mockResolvedValue([]);
});

afterEach(() => host.remove());

/** What every screen hands the settings, whatever else it hands them. */
function given() {
	return {
		settings: CHOSEN,
		database: { path: '/Users/someone/Vault/personal.kdbx', name: 'personal' },
		onSettings: vi.fn(),
		onChoose: vi.fn(),
		onLook: vi.fn()
	};
}

function show(props: Record<string, unknown> = {}) {
	return mount(Settings, { target: host, props: { ...given(), ...props } });
}

function chip(label: string): HTMLButtonElement {
	const found = host.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
	if (!found) throw new Error(`no control labelled ${label}`);
	return found;
}

const IDLE = 'How long an untouched vault stays open';
const CLIPBOARD = 'How long a copied password stays on the clipboard';
const SLEEP = 'Lock when this Mac goes to sleep';
const THEME = 'How the window is drawn';

function segments(): HTMLButtonElement[] {
	const group = host.querySelector(`[role="group"][aria-label="${THEME}"]`);
	if (!group) throw new Error('no segmented control for the look');
	return [...group.querySelectorAll('button')];
}

it('draws what was chosen, in words rather than in seconds', () => {
	const component = show();
	flushSync();

	expect(chip(IDLE).textContent).toContain('5 minutes');
	expect(chip(CLIPBOARD).textContent).toContain('1 minute');
	expect(host.textContent).toContain('/Users/someone/Vault/personal.kdbx');

	unmount(component);
});

/** Ten backups beside the open vault, the third of them open now, as Rust
 * lists them. */
function backups() {
	return Array.from({ length: 10 }, (_, at) => ({
		name: `personal.kdbx.${at + 1}.bak`,
		index: at + 1,
		taken: `2026-08-27T18:${String(40 - at).padStart(2, '0')}:00Z`
	}));
}

function pressed(label: string) {
	const found = [...host.querySelectorAll('button')].find(
		(each) => each.textContent?.trim() === label
	);
	if (!found) throw new Error(`there is no button called ${label}`);
	found.click();
	flushSync();
	return found;
}

/** How many backups there are is said at once; the list is under the row only
 * when asked for, and says which one is open now. A file name is isolated in
 * its row, whatever it holds. */
it('counts the backups and lists them only when asked', async () => {
	ipc.snapshots.mockResolvedValue([
		...backups(),
		{ name: 'a\u202Eb.kdbx.11.bak', index: 11, taken: null }
	]);
	const component = show({
		database: { path: '/Users/someone/Vault/personal.kdbx.3.bak', name: 'personal.kdbx.3' }
	});

	await vi.waitFor(() => expect(host.textContent).toContain('11 copies'));
	expect(host.textContent).toContain('Automatic backups');
	expect(host.querySelector('#backups')).toBeNull();

	const opened = pressed('Show');
	expect(opened.getAttribute('aria-expanded')).toBe('true');
	expect(opened.getAttribute('aria-controls')).toBe('backups');
	const list = host.querySelector('#backups');
	expect(list?.querySelectorAll('li')).toHaveLength(11);
	expect(list?.querySelectorAll('li')[2].textContent).toContain('Open now');
	expect([...(list?.querySelectorAll('bdi') ?? [])].map((each) => each.textContent)).toContain(
		'a\u202Eb.kdbx.11.bak'
	);

	pressed('Hide');
	expect(host.querySelector('#backups')).toBeNull();
	unmount(component);
});

/** A vault never saved has none, and with nothing chosen there is nothing to
 * ask Rust about: both say so, and offer nothing to show. */
it('says none yet for a vault that was never saved, or none chosen', async () => {
	let component = show();
	await vi.waitFor(() => expect(ipc.snapshots).toHaveBeenCalledTimes(1));
	expect(host.textContent).toContain('None yet');
	expect(
		[...host.querySelectorAll('button')].map((each) => each.textContent?.trim())
	).not.toContain('Show');
	unmount(component);

	ipc.snapshots.mockClear();
	component = show({ database: null });
	flushSync();
	expect(host.textContent).toContain('None yet');
	expect(ipc.snapshots).not.toHaveBeenCalled();
	unmount(component);

	ipc.snapshots.mockResolvedValue([backups()[0]]);
	component = show();
	await vi.waitFor(() => expect(host.textContent).toContain('1 copy'));
	unmount(component);
});

/** "Open to look" is the slot the row was shown at, handed up: the window
 * above is what locks an open vault on the way. */
it('opens a backup to look at by its slot', async () => {
	ipc.snapshots.mockResolvedValue(backups());
	const onLook = vi.fn().mockResolvedValue(undefined);
	const component = show({ onLook });

	await vi.waitFor(() => expect(host.textContent).toContain('10 copies'));
	pressed('Show');
	host.querySelector<HTMLButtonElement>('[aria-label="Open to look personal.kdbx.2.bak"]')?.click();
	await vi.waitFor(() => expect(onLook).toHaveBeenCalledWith(2));

	unmount(component);
});

/** The backup pressed went before the press reached it, and it was the last
 * one. The list says so and that none are left, rather than vanishing with the
 * sentence that would have said why. */
it('says the last backup went, and that none are left', async () => {
	ipc.snapshots.mockResolvedValueOnce([backups()[0]]).mockResolvedValue([]);
	const onLook = vi.fn().mockRejectedValue({ code: 'gone', message: 'the database file is gone' });
	const component = show({ onLook });

	await vi.waitFor(() => expect(host.textContent).toContain('1 copy'));
	pressed('Show');
	host.querySelector<HTMLButtonElement>('[aria-label="Open to look personal.kdbx.1.bak"]')?.click();

	await vi.waitFor(() => expect(host.textContent).toContain('No backups are left.'));
	expect(host.textContent).toContain('That backup is not there any more.');
	expect(host.textContent).toContain('None yet');
	pressed('Hide');
	expect(host.querySelector('#backups')).toBeNull();

	unmount(component);
});

/** The password row writes the vault, which moves the backups: the count is
 * read again after a new password and after the old backups are removed, so
 * it never says ten beside a sentence that says they went. */
it('reads the backups again after the password row changes them', async () => {
	ipc.snapshots.mockResolvedValueOnce(backups()).mockResolvedValue([backups()[0]]);
	ipc.removeOldSnapshots.mockResolvedValue({ gone: 10, left: 0, refused: null });
	const component = show({ rekey: vi.fn().mockResolvedValue(10) });
	await vi.waitFor(() => expect(host.textContent).toContain('10 copies'));

	pressed('Change…');
	const fields = [...host.querySelectorAll<HTMLInputElement>('input[type="password"]')];
	expect(fields).toHaveLength(3);
	for (const [at, value] of ['the one it had', 'a new one', 'a new one'].entries()) {
		fields[at].value = value;
		fields[at].dispatchEvent(new Event('input', { bubbles: true }));
	}
	flushSync();
	host.querySelector('form')?.dispatchEvent(new Event('submit', { cancelable: true }));
	await vi.waitFor(() => expect(host.querySelector('[data-confirm]')).not.toBeNull());
	expect(ipc.snapshots).toHaveBeenCalledTimes(2);

	pressed('Remove old backups');
	await vi.waitFor(() => expect(host.textContent).toContain('Removed 10 old backups.'));
	expect(ipc.snapshots).toHaveBeenCalledTimes(3);
	expect(host.textContent).toContain('1 copy');
	expect(host.textContent).not.toContain('10 copies');

	unmount(component);
});

/** A folder that would not be read is said on the line every refusal here
 * is said on, and nothing is listed. */
it('says why the backups could not be read', async () => {
	ipc.snapshots.mockRejectedValue({ code: 'io', message: 'the folder would not be read' });
	const component = show();

	await vi.waitFor(() => expect(host.textContent).toContain('the folder would not be read'));
	expect(host.textContent).toContain('None yet');

	unmount(component);
});

/** Both states, including the one the mockup does not draw. A switch whose off
 * state was never looked at is a switch that can only be turned on. */
it('draws a switch that is off as well as one that is on', () => {
	const component = show({ settings: { ...CHOSEN, lockOnSleep: false } });
	flushSync();

	expect(chip(SLEEP).getAttribute('aria-checked')).toBe('false');
	unmount(component);

	const second = show();
	flushSync();
	expect(chip(SLEEP).getAttribute('aria-checked')).toBe('true');
	unmount(second);
});

it('sends a switch the moment it is pressed, with nothing else changed', async () => {
	const onSettings = vi.fn();
	const component = show({ onSettings });
	flushSync();

	chip(SLEEP).click();
	await vi.waitFor(() => expect(ipc.setSettings).toHaveBeenCalledTimes(1));

	expect(ipc.setSettings).toHaveBeenCalledWith({ ...CHOSEN, lockOnSleep: false });
	expect(onSettings).toHaveBeenCalledWith({ ...CHOSEN, lockOnSleep: false });

	unmount(component);
});

it('offers every value Rust said it may, and sends the one that is picked', async () => {
	const onSettings = vi.fn();
	const component = show({ onSettings });
	flushSync();

	chip(IDLE).click();
	flushSync();

	const offered = [...host.querySelectorAll('button')]
		.map((candidate) => candidate.textContent?.trim())
		.filter((text): text is string => text !== undefined);
	for (const wanted of ['1 minute', '5 minutes', '15 minutes', '30 minutes', '1 hour']) {
		expect(offered).toContain(wanted);
	}

	const hour = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === '1 hour'
	);
	hour?.click();
	await vi.waitFor(() => expect(ipc.setSettings).toHaveBeenCalledTimes(1));

	expect(ipc.setSettings).toHaveBeenCalledWith({ ...CHOSEN, idleSeconds: 3600 });
	unmount(component);
});

/** Rust settles a value onto one it offers, and what it settled on is what the
 * screen has to end up showing - not what was clicked. */
it('draws what was stored rather than what was sent', async () => {
	let held = CHOSEN;
	const component = show({
		get settings() {
			return held;
		},
		onSettings: (settled: Chosen) => (held = settled)
	});
	flushSync();

	ipc.setSettings.mockResolvedValue({ ...CHOSEN, idleSeconds: 900 });
	chip(IDLE).click();
	flushSync();
	const hour = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === '1 hour'
	);
	hour?.click();

	await vi.waitFor(() => expect(held.idleSeconds).toBe(900));
	unmount(component);
});

it('says so when a choice cannot be stored, and leaves the value alone', async () => {
	const onSettings = vi.fn();
	const component = show({ onSettings });
	flushSync();

	ipc.setSettings.mockRejectedValue({ code: 'io', message: 'that folder is read only' });
	chip(SLEEP).click();

	await vi.waitFor(() => expect(host.textContent).toContain('that folder is read only'));
	expect(onSettings).not.toHaveBeenCalled();
	expect(chip(SLEEP).getAttribute('aria-checked')).toBe('true');

	unmount(component);
});

/** A vault Coffer has not been pointed at yet is a state the mockup does not
 * draw, and a screen that said nothing there would look broken. */
it('has something to say before a vault has been chosen', () => {
	const component = show({ database: null });
	flushSync();

	expect(host.textContent).toContain('None chosen yet');
	unmount(component);
});

/**
 * The list is not a native one - macOS draws that popup in colours the mockup
 * does not contain - so reaching it from the keyboard is this window's job. It
 * closed on the button's own blur, which is the same moment Tab moves into it,
 * and two of the seven rows could only be changed with a pointer.
 */
it('can be opened and chosen from with the keyboard alone', async () => {
	const onSettings = vi.fn();
	const component = show({ onSettings });
	flushSync();

	const idle = chip(IDLE);
	idle.focus();
	idle.click();
	flushSync();

	const options = [...host.querySelectorAll<HTMLElement>('[role="option"]')];
	expect(options).toHaveLength(CHOSEN.idleChoices.length);

	// Tab moves focus off the button and into the list. The list has to still
	// be there when it arrives.
	options[0].focus();
	idle.dispatchEvent(new FocusEvent('focusout', { relatedTarget: options[0], bubbles: true }));
	flushSync();
	expect(host.querySelectorAll('[role="option"]')).toHaveLength(CHOSEN.idleChoices.length);

	options[4].click();
	await vi.waitFor(() => expect(ipc.setSettings).toHaveBeenCalledTimes(1));
	expect(ipc.setSettings).toHaveBeenCalledWith({ ...CHOSEN, idleSeconds: 3600 });

	unmount(component);
});

it('closes when focus leaves the control altogether', () => {
	const component = show();
	flushSync();

	const idle = chip(IDLE);
	idle.click();
	flushSync();
	expect(host.querySelectorAll('[role="option"]').length).toBeGreaterThan(0);

	// Somewhere else entirely: another row's control.
	const elsewhere = chip(SLEEP);
	idle.dispatchEvent(new FocusEvent('focusout', { relatedTarget: elsewhere, bubbles: true }));
	flushSync();
	expect(host.querySelectorAll('[role="option"]')).toHaveLength(0);

	unmount(component);
});

it('closes on Escape and gives the button its focus back', () => {
	const component = show();
	flushSync();

	const idle = chip(IDLE);
	idle.focus();
	idle.click();
	flushSync();
	expect(host.querySelectorAll('[role="option"]').length).toBeGreaterThan(0);

	idle.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
	flushSync();

	expect(host.querySelectorAll('[role="option"]')).toHaveLength(0);
	expect(document.activeElement).toBe(idle);
	expect(ipc.setSettings).not.toHaveBeenCalled();

	unmount(component);
});

/** A chip says what it is and what it does, so a reader who cannot see it is
 * told there is a list behind it and which value is on. */
it('says it opens a list, and which value is chosen', () => {
	const component = show();
	flushSync();

	const idle = chip(IDLE);
	expect(idle.getAttribute('aria-haspopup')).toBe('listbox');
	expect(idle.getAttribute('aria-expanded')).toBe('false');

	idle.click();
	flushSync();
	expect(idle.getAttribute('aria-expanded')).toBe('true');

	const chosen = [...host.querySelectorAll('[role="option"]')].filter(
		(option) => option.getAttribute('aria-selected') === 'true'
	);
	expect(chosen).toHaveLength(1);
	expect(chosen[0].textContent?.trim()).toBe('5 minutes');

	unmount(component);
});

it('draws all three looks and lights the one the window is wearing', () => {
	const component = show();
	flushSync();

	expect(segments().map((one) => one.textContent?.trim())).toEqual(['System', 'Dark', 'Light']);
	expect(segments().filter((one) => one.getAttribute('aria-pressed') === 'true')).toHaveLength(1);
	expect(segments()[1].getAttribute('aria-pressed')).toBe('true');

	unmount(component);
});

/** A segmented control writes a settings file on every click unless it checks,
 * and the reader clicking the look they are already wearing is the common one. */
it('says nothing when the look that is already on is chosen again', () => {
	const component = show();
	flushSync();

	segments()[1].click();
	flushSync();

	expect(ipc.setSettings).not.toHaveBeenCalled();

	unmount(component);
});

/** The spread is what carries the rest of the settings back. A row that sent
 * only its own field would reset both timers to whatever Rust defaults to. */
it('sends the whole of what was chosen when the look changes', async () => {
	const component = show();
	flushSync();

	segments()[2].click();
	await vi.waitFor(() => expect(ipc.setSettings).toHaveBeenCalledTimes(1));

	expect(ipc.setSettings).toHaveBeenCalledWith({ ...CHOSEN, theme: 'light' });

	unmount(component);
});

/**
 * Rust will not point the session at another file while a vault is open, and
 * the refusal names the way through. A screen that caught it left the reader
 * pressing a button that did nothing at all.
 */
it('says why another vault cannot be opened from here yet', async () => {
	const onChoose = vi
		.fn()
		.mockRejectedValue({ code: 'refused', message: 'lock the vault before opening another' });
	const component = show({ onChoose });
	flushSync();

	const found = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === 'Open another'
	);
	found?.click();

	await vi.waitFor(() =>
		expect(host.textContent).toContain('lock the vault before opening another')
	);

	unmount(component);
});

/** Open Vault… in the menu bar is "Open another": with a vault open, Rust
 * refuses the picker, and the refusal is said here rather than nowhere. */
it('opens another vault from the menu, and says why when Rust refuses', async () => {
	const onChoose = vi
		.fn()
		.mockRejectedValue({ code: 'refused', message: 'lock the vault before opening another' });
	const component = show({ onChoose });
	flushSync();

	run('openVault');
	await vi.waitFor(() => expect(onChoose).toHaveBeenCalledTimes(1));
	await vi.waitFor(() =>
		expect(host.textContent).toContain('lock the vault before opening another')
	);

	unmount(component);
});

/** With nothing open, or a vault Coffer reads and does not write, there is no
 * password to change from here, and no row offering to. */
it('offers to change the master password only with a vault it can write', () => {
	const without = show();
	flushSync();
	expect(host.textContent).not.toContain('Master password');
	unmount(without);

	const component = show({ rekey: vi.fn() });
	flushSync();
	expect(host.textContent).toContain('Master password');
	expect(
		[...host.querySelectorAll('button')].some((each) => each.textContent?.trim() === 'Change…')
	).toBe(true);
	unmount(component);
});

it('puts the master password beside the vault it opens', () => {
	const component = show({ rekey: vi.fn() });
	flushSync();

	const rows = [...host.querySelectorAll('.text-row')].map((each) => each.textContent?.trim());
	expect(rows.indexOf('Master password')).toBe(rows.indexOf('Vault') + 1);
	unmount(component);
});

/** Nothing known of copies yet: none made, and nothing due. */
const NEVER: Elsewhere = { otherDisk: null, sameDiskAt: null, overdue: null };

/** The buttons in the settings, by what they say. */
function labels(): string[] {
	return [...host.querySelectorAll('button')].map((each) => each.textContent?.trim() ?? '');
}

/** The button that saves a copy on another disk, whatever it says now. */
function copyButton(): HTMLButtonElement {
	const found = [...host.querySelectorAll('button')].find((each) =>
		each.textContent?.includes('a copy')
	);
	if (!found) throw new Error('there is no button that saves a copy');
	return found;
}

/** The vault's file is one press from the Finder, beside "Open another". A
 * file that went since is said where every refusal here is, and the press
 * after it clears that. */
it('shows the vault in the Finder, and says why when it cannot', async () => {
	ipc.showInFinder
		.mockRejectedValueOnce({ code: 'gone', message: 'the database file is gone' })
		.mockResolvedValueOnce(undefined);
	const component = show();
	flushSync();

	expect(labels().indexOf('Show in Finder')).toBe(labels().indexOf('Open another') - 1);
	pressed('Show in Finder');
	await vi.waitFor(() => expect(host.textContent).toContain('the database file is gone'));

	pressed('Show in Finder');
	await vi.waitFor(() => expect(ipc.showInFinder).toHaveBeenCalledTimes(2));
	await vi.waitFor(() => expect(host.textContent).not.toContain('the database file is gone'));

	unmount(component);
});

it('offers no Finder while no vault is chosen', () => {
	const component = show({ database: null });
	flushSync();

	expect(labels()).not.toContain('Show in Finder');
	expect(applying()).not.toContain('showInFinder');

	unmount(component);
});

/** The row says none was ever made, and the press is the copy. Whether one is
 * on its way is the screen's below, which holds it for every way a copy is
 * asked for: while it is, the button says so and takes no second press, from
 * itself or the menu bar. */
it('says no copy was ever made, and saves one once however often it is asked', async () => {
	let finish: (made: null) => void = () => {};
	const props = reactive({
		...given(),
		elsewhere: NEVER,
		copying: false,
		onCopy: vi.fn(() => {
			props.copying = true;
			return new Promise<null>((resolve) => (finish = resolve)).finally(
				() => (props.copying = false)
			);
		})
	});
	const component = mount(Settings, { target: host, props });
	flushSync();

	expect(host.textContent).toContain('Copy on another disk');
	expect(host.textContent).toContain('Last made: never');
	expect(copyButton().textContent?.trim()).toBe('Save a copy…');

	copyButton().click();
	copyButton().click();
	flushSync();
	expect(copyButton().textContent?.trim()).toBe('Saving a copy…');
	expect(copyButton().disabled).toBe(true);
	expect(applying(), 'offered while a copy is on its way').not.toContain('saveCopy');
	copyButton().click();
	run('saveCopy');
	flushSync();
	expect(props.onCopy).toHaveBeenCalledTimes(1);

	finish(null);
	await vi.waitFor(() => expect(copyButton().textContent?.trim()).toBe('Save a copy…'));
	expect(copyButton().disabled).toBe(false);
	expect(applying()).toContain('saveCopy');

	unmount(component);
});

/** A copy asked for from the status bar or the menu bar before the settings
 * opened is still on its way when they are drawn: the button says so, and
 * neither it nor the menu bar starts another that would do nothing. */
it('says a copy started before the settings opened is on its way', () => {
	const onCopy = vi.fn();
	const component = show({ elsewhere: NEVER, onCopy, copying: true });
	flushSync();

	expect(copyButton().textContent?.trim()).toBe('Saving a copy…');
	expect(copyButton().disabled).toBe(true);
	expect(applying()).not.toContain('saveCopy');
	copyButton().click();
	run('saveCopy');
	flushSync();
	expect(onCopy).not.toHaveBeenCalled();

	unmount(component);
});

/** When, and on which disk; and the newest copy, when it is on the vault's
 * own disk, said for what it is under that. */
it('says when and on which disk the last copy went, and what one on the vault’s disk is', () => {
	const today = new Date().toISOString();
	const component = show({
		elsewhere: { otherDisk: { at: today, volume: 'Stick' }, sameDiskAt: null, overdue: null },
		onCopy: vi.fn()
	});
	flushSync();
	expect(host.textContent).toContain('Last made: today, on “\u2068Stick\u2069”');
	expect(host.textContent).not.toContain('same disk');
	unmount(component);

	const near = show({
		elsewhere: { otherDisk: null, sameDiskAt: today, overdue: 40 },
		onCopy: vi.fn()
	});
	flushSync();
	expect(host.textContent).toContain('Last made: never');
	expect(host.textContent).toContain(
		'The copy made today is on the same disk as the vault, and would not survive that disk failing.'
	);
	unmount(near);
});

/** A backup, the copy a lock left and a format Coffer will not write have no
 * copies of their own to keep: no row, and nothing in the menu bar. */
it('draws no row for copies while nothing Coffer keeps copies of is open', () => {
	for (const props of [{ elsewhere: null, onCopy: vi.fn() }, { elsewhere: NEVER }, {}]) {
		const component = show(props);
		flushSync();
		expect(host.textContent, JSON.stringify(props)).not.toContain('Copy on another disk');
		expect(applying(), JSON.stringify(props)).not.toContain('saveCopy');
		unmount(component);
	}
});

/** A name that holds a file is never written over, and the reader hears that
 * nothing was replaced; a panel closed says nothing at all. */
it('says nothing was replaced when the name was taken, and nothing when the panel was closed', async () => {
	const onCopy = vi
		.fn()
		.mockRejectedValueOnce({ code: 'taken', message: 'there is already a file with that name' })
		.mockResolvedValueOnce(null);
	const component = show({ elsewhere: NEVER, onCopy });
	flushSync();

	copyButton().click();
	await vi.waitFor(() =>
		expect(host.textContent).toContain(
			'A file by that name is already there, so nothing was replaced. Save the copy under another name.'
		)
	);
	expect(host.textContent).not.toContain('there is already a file with that name');

	copyButton().click();
	await vi.waitFor(() => expect(onCopy).toHaveBeenCalledTimes(2));
	await vi.waitFor(() => expect(host.textContent).not.toContain('nothing was replaced'));
	expect(host.querySelector('.text-warn')).toBeNull();

	unmount(component);
});

/** A disk's name is the reader's, or the name of a stick somebody handed
 * them, and reaches the row as text. */
it('writes the name of a disk as text', () => {
	const volume = '<img src=x onerror="window.pwned=1">';
	const component = show({
		elsewhere: {
			otherDisk: { at: new Date().toISOString(), volume },
			sameDiskAt: null,
			overdue: null
		},
		onCopy: vi.fn()
	});
	flushSync();

	expect(host.querySelector('img')).toBeNull();
	expect(host.textContent).toContain(volume);
	expect((window as { pwned?: number }).pwned).toBeUndefined();

	unmount(component);
});

/** Over the vault, the menu bar's Show in Finder and Save a Copy… are the
 * settings' own buttons. */
it('answers Show in Finder and Save a Copy… from the menu bar while it is open', async () => {
	ipc.showInFinder.mockResolvedValue(undefined);
	const onCopy = vi.fn().mockResolvedValue(null);
	const component = show({ elsewhere: NEVER, onCopy });
	flushSync();

	expect(applying()).toEqual(expect.arrayContaining(['showInFinder', 'saveCopy']));
	run('showInFinder');
	await vi.waitFor(() => expect(ipc.showInFinder).toHaveBeenCalledTimes(1));
	run('saveCopy');
	await vi.waitFor(() => expect(onCopy).toHaveBeenCalledTimes(1));

	unmount(component);
	expect(applying()).not.toContain('saveCopy');
});

/** The two kinds of copy are read together: the one on another disk right
 * above the automatic backups beside the vault. */
it('puts the copy on another disk right above the automatic backups', () => {
	const component = show({ elsewhere: NEVER, onCopy: vi.fn(), rekey: vi.fn() });
	flushSync();

	const rows = [...host.querySelectorAll('.text-row')].map((each) => each.textContent?.trim());
	expect(rows.indexOf('Copy on another disk')).toBe(rows.indexOf('Automatic backups') - 1);
	expect(rows.indexOf('Copy on another disk')).toBe(rows.indexOf('Master password') + 1);
	unmount(component);
});
