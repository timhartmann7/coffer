import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { drop, flush, release, settle, typed, unfinished, type Place } from './drafts';

const ipc = vi.hoisted(() => ({ draft: vi.fn() }));
vi.mock('./ipc', () => ipc);

/** Every entry in this file is its own, because what is typed is kept for the
 * life of the window and the window here is the whole file. */
let made = 0;
function place(field = 'Notes', protect = false): Place {
	made += 1;
	return { entry: `entry-${made}`, field, protect };
}

/** Every word Rust was told, in the order it was told, as (value, sequence). */
function told(): [string | null, number][] {
	return ipc.draft.mock.calls.map((call) => [call[2] as string | null, call[4] as number]);
}

beforeEach(() => {
	vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
	ipc.draft.mockResolvedValue(undefined);
});

afterEach(() => {
	vi.useRealTimers();
});

/**
 * The debounce. Rust is told once the reader pauses, and told what the field
 * holds then - not what it held at the first key, which is a value nobody ever
 * finished typing.
 */
it('tells Rust what is in the field once the reader pauses, and not at every key', () => {
	const at = place();
	let field = '';
	for (const key of 'passport') {
		field += key;
		typed(at, () => field);
		vi.advanceTimersByTime(100);
	}
	expect(ipc.draft).not.toHaveBeenCalled();
	expect(unfinished(at), 'the field is not marked while it is typed in').toBe(true);

	vi.advanceTimersByTime(250);
	expect(ipc.draft).toHaveBeenCalledTimes(1);
	expect(ipc.draft).toHaveBeenCalledWith(at.entry, 'Notes', 'passport', false, expect.any(Number));

	// Nothing more to say until something more is typed.
	vi.advanceTimersByTime(10_000);
	expect(ipc.draft).toHaveBeenCalledTimes(1);
});

/** Each field is its own draft, and a protected one goes as protected. */
it('tells each field on its own, under the protection it was given', () => {
	const notes = place();
	const pin = { ...notes, field: 'PIN', protect: true };
	typed(notes, () => 'a note');
	typed(pin, () => '4711');
	vi.advanceTimersByTime(250);

	expect(ipc.draft).toHaveBeenCalledTimes(2);
	expect(ipc.draft).toHaveBeenCalledWith(notes.entry, 'Notes', 'a note', false, expect.any(Number));
	expect(ipc.draft).toHaveBeenCalledWith(notes.entry, 'PIN', '4711', true, expect.any(Number));
});

/**
 * The race the numbers are for. The value written carries a number newer than
 * every draft already sent, so a draft that arrives after it is known to be
 * older - and a draft still waiting to be sent is never sent at all.
 */
it('finishes a field with a write newer than any draft of it, and sends no draft after', async () => {
	const at = place();
	typed(at, () => 'half');
	vi.advanceTimersByTime(250);
	typed(at, () => 'half and the rest');

	const write = vi.fn((sequence: number) => Promise.resolve(sequence));
	const written = await settle(at, write);

	const [[, drafted]] = told();
	expect(written, 'the write is not newer than the draft').toBeGreaterThan(drafted);
	expect(unfinished(at)).toBe(false);
	vi.advanceTimersByTime(10_000);
	expect(ipc.draft, 'a draft went after the value that finished it').toHaveBeenCalledTimes(1);
});

/** Taken back, and Rust is told to let go - after what it was told, never
 * before, and never when there was nothing to let go of. */
it('takes a draft back with a newer word, and only when Rust was told one', () => {
	const toldFirst = place();
	typed(toldFirst, () => 'never mind');
	vi.advanceTimersByTime(250);
	drop(toldFirst);

	const [[first, before], [second, after]] = told();
	expect(first).toBe('never mind');
	expect(second, 'the draft was not taken back').toBeNull();
	expect(after).toBeGreaterThan(before);
	expect(unfinished(toldFirst)).toBe(false);

	const neverTold = place();
	typed(neverTold, () => 'gone before the pause');
	drop(neverTold);
	vi.advanceTimersByTime(10_000);
	expect(ipc.draft, 'a draft nobody sent was taken back, or sent late').toHaveBeenCalledTimes(2);
});

/**
 * What the window losing focus and a lock by hand both wait for: everything
 * typed is told now, and every word already on its way - a value being written
 * included - has arrived before it answers.
 */
it('tells everything at once and waits for every word on its way', async () => {
	const at = place();
	typed(at, () => 'the lid is closing');

	let arrive: (value: number) => void = () => {};
	const writing = settle(
		{ ...at, field: 'Title' },
		() => new Promise<number>((done) => (arrive = done))
	);
	let flushed = false;
	const flushing = flush().then(() => (flushed = true));

	expect(ipc.draft, 'the draft waited for the pause').toHaveBeenCalledTimes(1);
	await Promise.resolve();
	expect(flushed, 'it answered before the write had arrived').toBe(false);

	arrive(1);
	await writing;
	await flushing;
	expect(flushed).toBe(true);

	vi.advanceTimersByTime(10_000);
	expect(ipc.draft, 'the draft was sent a second time').toHaveBeenCalledTimes(1);
});

/** A word Rust refused - the vault locked while it was on its way - is nobody's
 * business, and nothing waits on it for ever. */
it('shrugs off a draft that is refused', async () => {
	ipc.draft.mockRejectedValue({ code: 'noVault', message: 'no database is open' });
	const at = place();
	typed(at, () => 'too late');
	await expect(flush()).resolves.toBeUndefined();
	expect(ipc.draft).toHaveBeenCalledTimes(1);
});

/**
 * An entry deleted, or the file read again: what was typed goes with it, and
 * the number the command carries is newer than every draft sent before it.
 * Typing into another entry is left alone.
 */
it('lets go of an entry, or of everything, with a number newer than all of it', () => {
	const deleted = place();
	const other = place();
	typed(deleted, () => 'in the entry going to the bin');
	typed(other, () => 'in another entry');
	vi.advanceTimersByTime(250);
	typed(deleted, () => 'still typing');

	const sequence = release(deleted.entry);
	expect(sequence).toBeGreaterThan(Math.max(...told().map(([, number]) => number)));
	expect(unfinished(deleted)).toBe(false);
	expect(unfinished(other)).toBe(true);
	vi.advanceTimersByTime(10_000);
	expect(ipc.draft, 'typing in a deleted entry was still sent').toHaveBeenCalledTimes(2);

	typed(other, () => 'in another entry, more');
	release(null);
	expect(unfinished(other)).toBe(false);
	vi.advanceTimersByTime(10_000);
	expect(ipc.draft, 'typing was sent after the file was read again').toHaveBeenCalledTimes(2);
});

/** However the words interleave, the numbers only go up. */
it('numbers every word in the order it was said', async () => {
	const one = place();
	const two = place();
	const numbers: number[] = [];
	typed(one, () => 'a');
	vi.advanceTimersByTime(250);
	numbers.push(await settle(two, (sequence) => Promise.resolve(sequence)));
	typed(two, () => 'b');
	vi.advanceTimersByTime(250);
	drop(two);
	numbers.push(release(one.entry));

	const all = [...told().map(([, number]) => number), ...numbers].sort((a, b) => a - b);
	expect(new Set(all).size, 'two words shared a number').toBe(all.length);
	const [[, first], [, second], [, third]] = told();
	expect(first).toBeLessThan(numbers[0]);
	expect(numbers[0]).toBeLessThan(second);
	expect(second).toBeLessThan(third);
	expect(third).toBeLessThan(numbers[1]);
});
