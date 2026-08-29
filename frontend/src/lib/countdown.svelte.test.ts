import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Countdown } from './countdown.svelte';

const ipc = vi.hoisted(() => ({ stirred: vi.fn() }));
vi.mock('$lib/ipc', () => ipc);

beforeEach(() => {
	vi.useFakeTimers();
	ipc.stirred.mockResolvedValue(300);
});

afterEach(() => vi.useRealTimers());

/** Lets the promise the message is waiting on settle. `vi.waitFor` drives the
 * clock, and the clock here is a fake one the test is holding still. */
async function settled() {
	await Promise.resolve();
	await Promise.resolve();
}

describe('the clock the window keeps between messages', () => {
	it('counts down the number Rust gave, once a second', () => {
		const countdown = new Countdown();
		countdown.sync(300);

		expect(countdown.left).toBe(300);
		vi.advanceTimersByTime(3000);
		expect(countdown.left).toBe(297);

		countdown.stop();
	});

	/** The moment it reaches zero is the moment Rust destroys this window, so
	 * there is nothing to draw past it and nothing to say about it. */
	it('never counts past zero', () => {
		const countdown = new Countdown();
		countdown.sync(2);

		vi.advanceTimersByTime(60_000);
		expect(countdown.left).toBe(0);

		countdown.stop();
	});

	/** A vault that is not open has nothing counting down, and a clock left
	 * running would be counting towards something that already happened. */
	it('stops when there is nothing open', () => {
		const countdown = new Countdown();
		countdown.sync(300);
		countdown.sync(null);

		expect(countdown.left).toBeNull();
		vi.advanceTimersByTime(60_000);
		expect(countdown.left).toBeNull();
	});

	/**
	 * The throttle is the whole point. This runs on every keystroke, and a
	 * message to Rust for each of them would be thousands of messages saying one
	 * thing - and an idle timer being reset by the thing that draws it never
	 * fires at all.
	 */
	it('tells Rust the reader is there rarely, whatever the reader does', () => {
		const countdown = new Countdown();

		for (let at = 0; at < 10_000; at += 1) countdown.stir(at);
		expect(ipc.stirred).toHaveBeenCalledTimes(1);

		// Far enough apart to be worth saying again.
		countdown.stir(20_000);
		expect(ipc.stirred).toHaveBeenCalledTimes(2);

		countdown.stop();
	});

	it('takes the number that comes back and starts again from it', async () => {
		const countdown = new Countdown();
		countdown.sync(300);
		vi.advanceTimersByTime(120_000);
		expect(countdown.left).toBe(180);

		countdown.stir(0);
		await settled();
		expect(countdown.left).toBe(300);

		countdown.stop();
	});

	/** A rejected message is a vault that locked while the window was speaking.
	 * There is nothing to say about it: the window is about to go. */
	it('says nothing when Rust will not answer', async () => {
		ipc.stirred.mockRejectedValue({ code: 'noVault', message: 'no database is open' });
		const countdown = new Countdown();
		countdown.sync(300);

		countdown.stir(0);
		await settled();
		expect(ipc.stirred).toHaveBeenCalledTimes(1);
		expect(countdown.left).toBe(300);

		countdown.stop();
	});

	/** The bar is a class out of a fixed table, because Tailwind cannot read a
	 * class that was computed and the window may not set a width from script. */
	it('draws the bar as one of a fixed set of widths', () => {
		const countdown = new Countdown();
		expect(countdown.width()).toBe('w-0');

		countdown.sync(120);
		expect(countdown.width()).toBe('w-full');

		countdown.left = 60;
		expect(countdown.width()).toBe('w-6/12');

		countdown.left = 0;
		expect(countdown.width()).toBe('w-0');

		countdown.stop();
	});

	/** Every round starts the bar again, so a bar that was nearly empty is full
	 * the moment the deadline moves. */
	it('marks a fresh round whenever a new number arrives', () => {
		const countdown = new Countdown();
		const first = countdown.round;

		countdown.sync(300);
		expect(countdown.round).toBe(first + 1);
		countdown.sync(300);
		expect(countdown.round).toBe(first + 2);

		// Nothing open is not a round.
		countdown.sync(null);
		expect(countdown.round).toBe(first + 2);
	});
});
