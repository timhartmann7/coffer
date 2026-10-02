import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { forget, offer } from './context.svelte';
import { typed } from './drafts';
import { answer } from './menu.svelte';
import { outside } from './outside';
import type { Stubbed } from './stubbed';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('./ipc'), async (real) =>
	Object.assign(ipc, (await import('./stubbed')).stubbed(await real()))
);

const left: (() => void)[] = [];

beforeEach(() => {
	ipc.draft.mockResolvedValue(undefined);
	ipc.closeWindow.mockResolvedValue(undefined);
});

afterEach(() => {
	for (const gone of left.splice(0)) gone();
	forget();
});

/** The close button is the window going, not a choice of something on it:
 * what is typed goes to Rust and the close is asked for, and nothing a screen
 * answers runs - not even a way that would answer anything. Nor is it the
 * reader being there, which would put the idle lock off for a window that is
 * about to lock anyway. */
it('closes the window on the close button, after what is typed, and runs nothing', async () => {
	const ran = vi.fn();
	left.push(answer({ lock: { run: ran }, settings: { run: ran } }));
	const at = { entry: 'outside-1', field: 'Notes', protect: false };
	typed(at, () => 'written as the window went');
	const stir = vi.fn();

	outside({ action: 'closing' }, stir);

	await vi.waitFor(() => expect(ipc.closeWindow).toHaveBeenCalledTimes(1));
	expect(ipc.draft).toHaveBeenCalledWith(
		at.entry,
		'Notes',
		'written as the window went',
		false,
		false,
		expect.any(Number)
	);
	expect(ipc.draft.mock.invocationCallOrder[0]).toBeLessThan(
		ipc.closeWindow.mock.invocationCallOrder[0]
	);
	expect(ran).not.toHaveBeenCalled();
	expect(stir).not.toHaveBeenCalled();
});

/** A close Rust refused is left to Rust, which closes the window itself once
 * the page has had its grace: nothing is thrown at nobody. */
it('lets a close that failed go without a word', async () => {
	ipc.closeWindow.mockRejectedValueOnce({
		code: 'other',
		message: 'Coffer could not finish that.'
	});

	await expect(outside({ action: 'closing' }, vi.fn())).resolves.toBeUndefined();
	expect(ipc.closeWindow).toHaveBeenCalledTimes(1);
});

/** An item of the menu bar is the reader being there, and is run by the
 * screen drawn last that can do it. */
it('runs a choice from the menu bar on the newest screen that can, and counts the reader in', () => {
	const under = vi.fn();
	const over = vi.fn();
	left.push(answer({ find: { run: under } }));
	left.push(answer({ find: { run: over } }));
	const stir = vi.fn();

	outside({ action: 'command', command: 'find' }, stir);

	expect(over).toHaveBeenCalledTimes(1);
	expect(under).not.toHaveBeenCalled();
	expect(stir).toHaveBeenCalledTimes(1);
	expect(ipc.closeWindow).not.toHaveBeenCalled();
});

/** An item of a menu under the pointer is the reader being there too. It is
 * run by what asked for that menu, and nothing that answers the menu bar runs
 * instead. */
it('runs an item of a menu under the pointer, and counts the reader in', () => {
	ipc.contextMenu.mockResolvedValue(undefined);
	const ran = vi.fn();
	left.push(answer({ newFolder: { run: ran } }));
	const answered = vi.fn();
	const folder = document.createElement('div');
	folder.addEventListener('contextmenu', (event) =>
		offer(event, { kind: 'folder', group: 'group-1', places: [] }, answered, vi.fn())
	);
	document.body.appendChild(folder);
	folder.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
	const serial = ipc.contextMenu.mock.calls[0][0] as number;
	const stir = vi.fn();

	outside({ action: 'context', serial, chosen: { item: 'newFolderIn', group: 'group-1' } }, stir);

	expect(answered).toHaveBeenCalledWith({ item: 'newFolderIn', group: 'group-1' });
	expect(ran).not.toHaveBeenCalled();
	expect(stir).toHaveBeenCalledTimes(1);
	folder.remove();
});
