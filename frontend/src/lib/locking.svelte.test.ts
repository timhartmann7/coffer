import { beforeEach, expect, it, vi } from 'vitest';
import { drop, replacing, settle, typed, type Place } from './drafts';
import { closeByHand, lockByHand } from './locking';
import type { Stubbed } from './stubbed';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('./ipc'), async (real) =>
	Object.assign(ipc, (await import('./stubbed')).stubbed(await real()))
);

/** Everything Rust was told, in order: drafts as their value, and the lock. */
function heard(): (string | null)[] {
	const said = [
		...ipc.draft.mock.calls.map((call, n) => ({
			at: ipc.draft.mock.invocationCallOrder[n],
			word: call[2] as string | null
		})),
		...ipc.lock.mock.invocationCallOrder.map((at) => ({ at, word: 'lock' }))
	];
	return said.sort((a, b) => a.at - b.at).map(({ word }) => word);
}

let made = 0;
function place(field = 'Notes', protect = false): Place {
	made += 1;
	return { entry: `locking-${made}`, field, protect };
}

beforeEach(() => {
	ipc.draft.mockResolvedValue(undefined);
});

/**
 * The regression this guards against is the order the Lock button used to
 * keep: the screen cleared first, the Change field torn down with a new
 * passport number in it, and its teardown telling Rust to let go of the number
 * before the lock was asked for. The lock then wrote nothing of it.
 */
it('tells Rust what is typed, locks, and only then lets the screen go', async () => {
	const at = place('Password', true);
	replacing(at, () => 'C4123456');

	let answer: () => void = () => {};
	ipc.lock.mockReturnValue(new Promise<void>((done) => (answer = done)));
	let cleared = false;
	const locking = lockByHand(() => {
		cleared = true;
		drop(at);
	});

	await vi.waitFor(() => expect(ipc.lock).toHaveBeenCalledTimes(1));
	expect(cleared, 'the screen went before the lock had answered').toBe(false);
	expect(heard()).toEqual(['C4123456', 'lock']);

	answer();
	await locking;
	expect(cleared).toBe(true);
	expect(heard(), 'the text was taken back ahead of the lock').toEqual(['C4123456', 'lock', null]);
});

/** A value already on its way is waited for: the lock must find it written. */
it('waits for a write already on its way before it asks for the lock', async () => {
	const at = place();
	typed(at, () => 'half a note');
	let written: () => void = () => {};
	const writing = settle(at, () => new Promise<void>((done) => (written = done)));
	ipc.lock.mockResolvedValue(undefined);

	const locking = lockByHand(() => {});
	await Promise.resolve();
	await Promise.resolve();
	expect(ipc.lock, 'the lock went ahead of the write').not.toHaveBeenCalled();

	written();
	await writing;
	await locking;
	expect(ipc.lock).toHaveBeenCalledTimes(1);
});

/** A lock that failed still stops drawing a tree the reader asked to put away. */
it('clears the screen when the lock is refused, and says so to the caller', async () => {
	ipc.lock.mockRejectedValue({ code: 'internal', message: 'the lock failed' });
	let cleared = false;

	await expect(lockByHand(() => (cleared = true))).rejects.toEqual({
		code: 'internal',
		message: 'the lock failed'
	});
	expect(cleared).toBe(true);
});

/** Closing the window is a lock the reader asked for by another button, and
 * it keeps the same order: what is typed reaches Rust before the close does. */
it('tells Rust what is typed before it asks for the window to close', async () => {
	const at = place('Notes');
	typed(at, () => 'written as the window went');
	ipc.closeWindow.mockResolvedValue(undefined);

	await closeByHand();

	expect(ipc.draft).toHaveBeenCalledWith(
		at.entry,
		'Notes',
		'written as the window went',
		false,
		false,
		expect.any(Number)
	);
	expect(ipc.closeWindow).toHaveBeenCalledTimes(1);
	expect(ipc.draft.mock.invocationCallOrder[0]).toBeLessThan(
		ipc.closeWindow.mock.invocationCallOrder[0]
	);
});

/** A write that failed - the disk went, the vault was already locking - is
 * no reason to leave the window up: the close still locks the vault. */
it('asks for the close even when a write on its way failed', async () => {
	const at = place();
	typed(at, () => 'half a note');
	ipc.draft.mockRejectedValueOnce({ code: 'noVault', message: 'no database is open' });
	ipc.closeWindow.mockResolvedValue(undefined);

	await closeByHand();

	expect(ipc.closeWindow).toHaveBeenCalledTimes(1);
});
