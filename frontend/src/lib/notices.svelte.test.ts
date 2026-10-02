import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RISE } from './motion';
import { Notices, REPORTED, UNDOABLE } from './notices.svelte';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

/** An undo that counts how often it ran, and lets the test say when Rust
 * answers. */
function undoing() {
	let answer: () => void = () => {};
	const undo = vi.fn(() => new Promise<void>((resolve) => (answer = resolve)));
	return { undo, answer: () => answer() };
}

describe('the offer to take something back', () => {
	it('runs once when the button and the key are pressed together', async () => {
		const notices = new Notices(vi.fn());
		const { undo, answer } = undoing();
		notices.offer('Moved “Bank” to the Recycle Bin', undo);

		notices.notice?.undo?.();
		notices.takeBack();
		notices.notice?.undo?.();
		answer();
		await vi.runAllTimersAsync();

		expect(undo).toHaveBeenCalledTimes(1);
	});

	it('is withdrawn by a newer notice, from the key and from the old button', async () => {
		const notices = new Notices(vi.fn());
		const { undo } = undoing();
		notices.offer('Moved “Bank” to the Recycle Bin', undo);
		const button = notices.notice?.undo;

		notices.warn('The file could not be written.');
		expect(notices.offering).toBe(false);
		notices.takeBack();
		button?.();
		await vi.runAllTimersAsync();

		expect(undo).not.toHaveBeenCalled();
		expect(notices.notice).toBeNull();
	});

	/** A notice that has started going offers nothing, even while it is still
	 * on the screen fading out under the pointer. */
	it('ends when the notice starts going, not when it has gone', async () => {
		const notices = new Notices(vi.fn());
		const { undo } = undoing();
		notices.offer('Moved “Bank” to the Recycle Bin', undo);

		await vi.advanceTimersByTimeAsync(UNDOABLE - 1);
		expect(notices.offering).toBe(true);
		await vi.advanceTimersByTimeAsync(1);
		expect(notices.offering).toBe(false);
		expect(notices.leaving).toBe(true);
		expect(notices.notice).not.toBeNull();

		notices.notice?.undo?.();
		notices.takeBack();
		await vi.advanceTimersByTimeAsync(RISE);
		expect(undo).not.toHaveBeenCalled();
		expect(notices.notice).toBeNull();
	});

	it('reports an undo Rust refused, rather than dropping it', async () => {
		const failed = vi.fn();
		const notices = new Notices(failed);
		const refusal = { code: 'superseded', message: 'no' };
		notices.offer('Field “PIN” removed', () => Promise.reject(refusal));

		notices.takeBack();
		await vi.runAllTimersAsync();

		expect(failed).toHaveBeenCalledWith(refusal);
	});
});

describe('what an offer is about', () => {
	/** The icon is the kind: a move between folders carries the folder, and a
	 * move to the bin the trash, which is what every offer was before. */
	it('a move is offered back with the folder, not the trash', async () => {
		const notices = new Notices(vi.fn());
		const { undo } = undoing();

		notices.offer('Moved “Chase” to “Banking”', undo, 'moved');
		expect(notices.notice?.kind).toBe('moved');
		expect(notices.offering).toBe(true);
		notices.takeBack();
		await vi.runAllTimersAsync();
		expect(undo).toHaveBeenCalledTimes(1);

		notices.offer('Moved “Bank” to the Recycle Bin', undo);
		expect(notices.notice?.kind).toBe('removed');
	});
});

describe('a notice on the screen', () => {
	/** The clock of the notice before is stopped, or it takes the next one away
	 * early. */
	it('stays for its own length, whatever was there before it', async () => {
		const notices = new Notices(vi.fn());
		notices.warn('first');
		await vi.advanceTimersByTimeAsync(REPORTED - 10);
		notices.warn('second');
		await vi.advanceTimersByTimeAsync(REPORTED - 10);

		expect(notices.notice?.message).toBe('second');
		expect(notices.leaving).toBe(false);
	});

	it('arrives whole when it replaces one that was going', async () => {
		const notices = new Notices(vi.fn());
		notices.warn('first');
		await vi.advanceTimersByTimeAsync(REPORTED);
		expect(notices.leaving).toBe(true);

		const before = notices.told;
		notices.warn('second');
		expect(notices.leaving).toBe(false);
		expect(notices.told).toBe(before + 1);
		await vi.advanceTimersByTimeAsync(RISE);
		expect(notices.notice?.message).toBe('second');
	});

	it('leaves nothing running once the screen has gone', async () => {
		const notices = new Notices(vi.fn());
		const { undo } = undoing();
		notices.offer('Moved “Bank” to the Recycle Bin', undo);

		notices.clear();
		expect(vi.getTimerCount()).toBe(0);
		notices.takeBack();
		await vi.runAllTimersAsync();
		expect(undo).not.toHaveBeenCalled();
	});
});
