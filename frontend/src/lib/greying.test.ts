import { beforeEach, expect, it, vi } from 'vitest';
import { report } from './greying';
import type { Stubbed } from './stubbed';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('./ipc'), async (real) =>
	Object.assign(ipc, (await import('./stubbed')).stubbed(await real()))
);

beforeEach(() => {
	ipc.menuState.mockResolvedValue(undefined);
});

/**
 * Rust answers two reports side by side and in either order, so a bar told
 * "Copy Login" then "nothing" could be left offering Copy Login. One report is
 * on its way at a time, and what changed meanwhile goes after it - only the
 * latest of it.
 */
it('tells Rust one report at a time, and only the latest of what changed meanwhile', async () => {
	let landed: () => void = () => {};
	ipc.menuState.mockReturnValueOnce(new Promise<void>((resolve) => (landed = resolve)));

	report(['copyLogin']);
	report(['find']);
	report(['lock']);
	expect(ipc.menuState).toHaveBeenCalledTimes(1);
	expect(ipc.menuState).toHaveBeenLastCalledWith(['copyLogin']);

	landed();
	await vi.waitFor(() => expect(ipc.menuState).toHaveBeenCalledTimes(2));
	expect(ipc.menuState).toHaveBeenLastCalledWith(['lock']);

	report(['lock']);
	await Promise.resolve();
	expect(ipc.menuState, 'the same list was sent twice').toHaveBeenCalledTimes(2);
});

/** A report Rust refused leaves the bar as it was, so the same list is worth
 * sending again rather than taken for told. */
it('sends a report again after one that failed', async () => {
	ipc.menuState.mockRejectedValueOnce({ code: 'other', message: 'Coffer could not finish that.' });

	report(['newFolder']);
	await vi.waitFor(() => expect(ipc.menuState).toHaveBeenCalledTimes(1));
	await Promise.resolve();

	report(['newFolder']);
	await vi.waitFor(() => expect(ipc.menuState).toHaveBeenCalledTimes(2));
	expect(ipc.menuState).toHaveBeenLastCalledWith(['newFolder']);
});
