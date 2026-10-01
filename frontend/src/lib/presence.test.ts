/**
 * The throttle on telling Rust that somebody is at the machine.
 *
 * It runs on every keystroke and every click, so what it does *not* send is the
 * whole of it: a message per event would be thousands of messages to say one
 * thing, and there is no upper bound on how fast a reader types.
 */

import { beforeEach, expect, it, vi } from 'vitest';
import { Presence } from './presence';
import type { Stubbed } from './stubbed';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('./ipc'), async (real) =>
	Object.assign(ipc, (await import('./stubbed')).stubbed(await real()))
);

beforeEach(() => ipc.stirred.mockResolvedValue(300));

/** One turn of the microtask queue, so that a rejection is settled before the
 * test that provoked it ends. */
const settled = () => new Promise((finish) => setTimeout(finish, 0));

it('says so the first time, whenever the first time is', () => {
	const presence = new Presence();
	presence.stir(0);
	expect(ipc.stirred).toHaveBeenCalledTimes(1);
});

it('says nothing again until the throttle has run out', () => {
	const presence = new Presence();
	presence.stir(1_000_000);

	// A paragraph of typing, at a speed nobody reaches.
	for (let at = 0; at < 500; at += 1) presence.stir(1_000_000 + at * 20);
	expect(ipc.stirred).toHaveBeenCalledTimes(1);

	presence.stir(1_000_000 + 15_000);
	expect(ipc.stirred).toHaveBeenCalledTimes(2);
});

/** A refusal is Rust saying there is nothing open to keep awake. It must not
 * reach the console as an unhandled rejection, and it must not stop the next
 * message being sent. */
it('lets a refused message go, and sends the next one anyway', async () => {
	ipc.stirred
		.mockRejectedValueOnce({ code: 'noVault', message: 'no database is open' })
		.mockResolvedValue(300);

	const presence = new Presence();
	presence.stir(0);
	await settled();
	expect(ipc.stirred).toHaveBeenCalledTimes(1);

	presence.stir(20_000);
	await settled();
	expect(ipc.stirred).toHaveBeenCalledTimes(2);
});
