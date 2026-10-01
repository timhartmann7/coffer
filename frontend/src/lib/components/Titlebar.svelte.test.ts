import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import Titlebar from './Titlebar.svelte';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => host.remove());

function lockButton(): HTMLButtonElement | undefined {
	return [...host.querySelectorAll('button')].find((each) => each.textContent?.trim() === 'Lock');
}

/** The one place on the screen that locks says which key does it from
 * anywhere, so the reader leaving the desk learns it once. */
it('names the key that locks on the Lock button', () => {
	const onLock = vi.fn();
	const component = mount(Titlebar, {
		target: host,
		props: { name: 'personal', unlocked: true, onLock }
	});
	flushSync();

	const lock = lockButton();
	expect(lock?.title).toBe('Lock the vault · ⌘L');
	lock?.click();
	expect(onLock).toHaveBeenCalledTimes(1);

	return unmount(component);
});

it('offers no Lock while nothing is open', () => {
	const component = mount(Titlebar, {
		target: host,
		props: { name: 'Coffer', unlocked: false, onLock: vi.fn() }
	});
	flushSync();

	expect(lockButton()).toBeUndefined();
	expect(host.querySelector('[title]')).toBeNull();

	return unmount(component);
});
