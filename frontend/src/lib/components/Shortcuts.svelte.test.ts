import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { answer, run } from '$lib/menu.svelte';
import { KEYS, OWN } from '$lib/shortcuts';
import type { Stubbed } from '$lib/stubbed';
import Shortcuts from './Shortcuts.svelte';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('$lib/ipc'), async (real) =>
	Object.assign(ipc, (await import('$lib/stubbed')).stubbed(await real()))
);

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => host.remove());

function open(onClose = vi.fn()) {
	const component = mount(Shortcuts, { target: host, props: { onClose } });
	flushSync();
	return { component, onClose };
}

/** The keys as the sheet draws them, row by row. */
function keys(): string[] {
	return [...host.querySelectorAll('kbd')].map((each) => each.textContent?.trim() ?? '');
}

/** Every key the bar answers and every one the window answers, once each, so
 * the sheet cannot advertise a key that does nothing or leave one out. */
it('lists every key the menu bar answers and the window’s own, each once', () => {
	const { component } = open();

	expect(keys()).toEqual([...Object.values(KEYS), ...Object.values(OWN)]);
	expect(new Set(keys()).size).toBe(keys().length);
	expect(host.querySelector('[role="dialog"]')?.getAttribute('aria-labelledby')).toBe(
		host.querySelector('h2')?.id
	);
	expect(host.querySelector('h2')?.textContent).toBe('Keyboard shortcuts');

	return unmount(component);
});

/** Escape is the sheet's. The screen under it puts the open entry away on the
 * same key, and must not hear it. */
it('closes on Escape, and the screen under it never hears the key', () => {
	const under = vi.fn();
	window.addEventListener('keydown', under);
	const { component, onClose } = open();
	try {
		document.activeElement?.dispatchEvent(
			new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
		);
		expect(onClose).toHaveBeenCalledTimes(1);
		expect(under, 'the key went on to the screen under the sheet').not.toHaveBeenCalled();

		// The Escape that cancels an accent being typed is the input method's.
		document.activeElement?.dispatchEvent(
			new KeyboardEvent('keydown', { key: 'Escape', isComposing: true, bubbles: true })
		);
		expect(onClose).toHaveBeenCalledTimes(1);
	} finally {
		window.removeEventListener('keydown', under);
		unmount(component);
	}
});

/** Cmd+C with nothing selected copies the open entry's password, and the
 * window hears it on its way up. With the sheet over the pane it is the
 * sheet's key, and copies nothing. */
it('keeps Cmd+C from copying a password under it', () => {
	const under = vi.fn();
	window.addEventListener('keydown', under);
	const { component } = open();
	try {
		const copy = new KeyboardEvent('keydown', { key: 'c', metaKey: true, bubbles: true });
		document.activeElement?.dispatchEvent(copy);
		expect(under).not.toHaveBeenCalled();
	} finally {
		window.removeEventListener('keydown', under);
		unmount(component);
	}
});

it('puts the focus on Close and gives it back when it goes', async () => {
	const before = document.createElement('button');
	document.body.appendChild(before);
	before.focus();
	try {
		const { component } = open();
		expect(document.activeElement?.textContent?.trim()).toBe('Close');

		unmount(component);
		await Promise.resolve();
		expect(document.activeElement).toBe(before);
	} finally {
		before.remove();
	}
});

/**
 * The window takes the sheet down and lifts `inert` off the screens in one
 * flush, the sheet first. WebKit puts the focus nowhere inside an inert
 * screen, so a focus given back while the sheet was being taken down landed
 * on nothing, and a keyboard or VoiceOver reader started again from the top of
 * the window. Happy DOM focuses through `inert`, so WebKit's rule is put in
 * for this test.
 */
it('gives the focus back once the screens under it are no longer inert', async () => {
	const screens = document.createElement('div');
	const before = document.createElement('button');
	screens.appendChild(before);
	document.body.appendChild(screens);
	const focus = HTMLElement.prototype.focus;
	vi.spyOn(HTMLElement.prototype, 'focus').mockImplementation(function (
		this: HTMLElement,
		options?: FocusOptions
	) {
		if (!this.closest('[inert]')) focus.call(this, options);
	});
	try {
		before.focus();
		const { component } = open();
		screens.setAttribute('inert', '');
		expect(document.activeElement?.textContent?.trim()).toBe('Close');

		// The order the window's flush keeps: the sheet goes, then the screens
		// under it are let go.
		unmount(component);
		screens.removeAttribute('inert');
		await Promise.resolve();

		expect(document.activeElement, 'the focus was given back into an inert screen').toBe(before);
	} finally {
		screens.remove();
	}
});

/** A choice from the menu bar puts the sheet away and then moves the focus
 * itself - Find into the search field - or leaves it for the screen it opens
 * to take, as New Entry leaves it for the new entry's title. The focus the
 * sheet would give back must not take it from either. */
it('leaves the focus to a choice from the menu bar that put it away', async () => {
	const before = document.createElement('button');
	const search = document.createElement('input');
	document.body.append(before, search);
	let component: ReturnType<typeof mount> | undefined;
	const away = () => {
		if (component) void unmount(component);
		component = undefined;
	};
	const gone = answer({ find: { run: () => search.focus() }, newEntry: { run: () => {} } });
	try {
		before.focus();
		component = open(vi.fn(away)).component;
		run('find');
		await Promise.resolve();
		expect(component, 'the choice left the sheet up').toBeUndefined();
		expect(document.activeElement, 'the sheet took the focus back from Find').toBe(search);

		before.focus();
		component = open(vi.fn(away)).component;
		run('newEntry');
		await Promise.resolve();
		expect(
			document.activeElement,
			'the focus went back under the sheet, ahead of the new entry'
		).not.toBe(before);
	} finally {
		away();
		gone();
		before.remove();
		search.remove();
	}
});

/**
 * One Shift+Tab from Close is the Lock button in the title bar, which stays
 * above the veil; a press on the title bar puts the focus nowhere. A key from
 * either is still the sheet's while it is up: Cmd+C must not copy the
 * password of the entry under it, and Escape must put away the sheet and not
 * the pane under it.
 */
it('keeps Cmd+C and Escape from the pane when the focus is on the title bar', () => {
	const lock = document.createElement('button');
	document.body.appendChild(lock);
	const under = vi.fn();
	window.addEventListener('keydown', under);
	const { component, onClose } = open();
	try {
		for (const target of [lock, document.body]) {
			if (target instanceof HTMLButtonElement) target.focus();
			else (document.activeElement as HTMLElement | null)?.blur();
			target.dispatchEvent(
				new KeyboardEvent('keydown', { key: 'c', metaKey: true, bubbles: true, cancelable: true })
			);
			target.dispatchEvent(
				new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
			);
		}
		expect(under, 'a key went on to the screen under the sheet').not.toHaveBeenCalled();
		expect(onClose).toHaveBeenCalledTimes(2);
	} finally {
		window.removeEventListener('keydown', under);
		lock.remove();
		unmount(component);
	}
});

it('closes on a press on the veil and not on a press inside', () => {
	const { component, onClose } = open();

	host.querySelector<HTMLElement>('[role="dialog"]')?.click();
	host.querySelector('h2')?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
	expect(onClose).not.toHaveBeenCalled();

	host.querySelector<HTMLElement>('[role="presentation"]')?.click();
	expect(onClose).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/** A choice in the menu bar is about the screen under the sheet, which the
 * sheet makes inert: it goes first. Keyboard Shortcuts again leaves it. */
it('goes before a choice from the menu bar is done', () => {
	const said: string[] = [];
	const { component } = open(vi.fn(() => said.push('closed')));
	const gone = answer({
		find: { run: () => said.push('find') },
		shortcuts: { run: () => said.push('shortcuts') }
	});

	run('shortcuts');
	run('find');
	expect(said).toEqual(['shortcuts', 'closed', 'find']);

	gone();
	return unmount(component);
});
