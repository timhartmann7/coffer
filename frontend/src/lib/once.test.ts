import { describe, expect, it } from 'vitest';
import { Once } from './once';

/** Work that waits until the test says Rust has answered. */
function pending<T>() {
	let answer: (value: T) => void = () => {};
	let refuse: (reason: unknown) => void = () => {};
	const promise = new Promise<T>((resolve, reject) => {
		answer = resolve;
		refuse = reject;
	});
	return { promise, answer, refuse };
}

describe('a press on its way', () => {
	it('drops a press while anything it names is on its way, and nothing else', async () => {
		const once = new Once();
		const first = pending<string>();

		const going = once.run(['a', 'b'], () => first.promise);
		expect(once.busy('a')).toBe(true);
		expect(await once.run(['b', 'c'], () => Promise.resolve('again'))).toBeNull();
		expect(await once.run(['c'], () => Promise.resolve('other'))).toBe('other');

		first.answer('done');
		expect(await going).toBe('done');
		expect(once.busy('a')).toBe(false);
		expect(once.busy('b')).toBe(false);
	});

	/** A refusal is an answer like any other, and a key held after one would
	 * drop every press about it from then on. */
	it('lets every key go whatever the answer, a key named twice included', async () => {
		const once = new Once();
		const refused = pending<string>();

		const going = once.run(['a', 'a'], () => refused.promise);
		refused.refuse({ code: 'refused', message: 'no' });
		await expect(going).rejects.toEqual({ code: 'refused', message: 'no' });

		expect(once.busy('a')).toBe(false);
		expect(await once.run(['a'], () => Promise.resolve('later'))).toBe('later');
	});
});
