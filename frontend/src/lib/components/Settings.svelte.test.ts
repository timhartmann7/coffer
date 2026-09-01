import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Settings as Chosen } from '$lib/model';
import Settings from './Settings.svelte';

const ipc = vi.hoisted(() => ({
	setSettings: vi.fn(),
	asFailure: vi.fn()
}));
vi.mock('$lib/ipc', () => ipc);

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
	// The faithful implementation, so that a failure reaches the screen the way
	// a rejected command really does.
	ipc.asFailure.mockImplementation((thrown: unknown) =>
		thrown && typeof (thrown as { code?: unknown }).code === 'string'
			? thrown
			: { code: 'other', message: 'Coffer could not finish that.' }
	);
});

afterEach(() => host.remove());

function show(props: Record<string, unknown> = {}) {
	return mount(Settings, {
		target: host,
		props: {
			settings: CHOSEN,
			database: { path: '/Users/someone/Vault/personal.kdbx', name: 'personal' },
			onSettings: vi.fn(),
			onChoose: vi.fn(),
			...props
		}
	});
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
	expect(host.textContent).toContain('personal.1–10.bak');

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
