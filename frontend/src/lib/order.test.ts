import { describe, expect, it } from 'vitest';
import { byName } from './order';

/** The names in the order `byName` draws them. */
function sorted(names: string[]): string[] {
	return names
		.map((name) => ({ name }))
		.sort(byName)
		.map((each) => each.name);
}

/** Every order the names can arrive in. */
function* orders(names: string[]): Generator<string[]> {
	if (names.length <= 1) {
		yield names;
		return;
	}
	for (const [at, first] of names.entries()) {
		for (const rest of orders([...names.slice(0, at), ...names.slice(at + 1)])) {
			yield [first, ...rest];
		}
	}
}

/** A generator of numbers that gives the same ones on every run, so a name
 * that breaks the order is a name that can be looked at again. */
function seeded(seed: number): () => number {
	let state = seed >>> 0;
	return () => {
		state = (state + 0x6d2b79f5) >>> 0;
		let mixed = Math.imul(state ^ (state >>> 15), 1 | state);
		mixed ^= mixed + Math.imul(mixed ^ (mixed >>> 7), 61 | mixed);
		return ((mixed ^ (mixed >>> 14)) >>> 0) / 4_294_967_296;
	};
}

const compare = (a: string, b: string) => byName({ name: a }, { name: b });

describe('the order names are drawn in', () => {
	it('reads the numbers inside a name as numbers', () => {
		expect(sorted(['Code 10', 'Code 2', 'Code 1', 'Code 100', 'Code 9'])).toEqual([
			'Code 1',
			'Code 2',
			'Code 9',
			'Code 10',
			'Code 100'
		]);
		expect(sorted(['v1.10', 'v1.9', 'v1.2'])).toEqual(['v1.2', 'v1.9', 'v1.10']);
	});

	/** Larger than a number JavaScript holds exactly, which an order that
	 * parsed the digits into one would get wrong. */
	it('reads a number longer than a double holds as the number it is', () => {
		expect(
			sorted([
				'Code 100000000000000000000',
				'Code 99999999999999999999',
				'Code 10',
				'Code 9007199254740993',
				'Code 9007199254740992'
			])
		).toEqual([
			'Code 10',
			'Code 9007199254740992',
			'Code 9007199254740993',
			'Code 99999999999999999999',
			'Code 100000000000000000000'
		]);
	});

	it('puts a name beside the same name in another case, not after every capital', () => {
		expect(sorted(['pin', 'Zebra', 'apple', 'PIN', 'banana', 'Apple'])).toEqual([
			'Apple',
			'apple',
			'banana',
			'PIN',
			'pin',
			'Zebra'
		]);
	});

	/** Two names that read the same to the collator used to come out in
	 * whichever order they went in, so a list drawn again after an edit could
	 * swap two of its rows under the reader's pointer. */
	it('keeps the same order between names that differ only in case, however they arrive', () => {
		expect(sorted(['pin', 'PIN'])).toEqual(sorted(['PIN', 'pin']));
		expect(compare('pin', 'PIN')).toBe(-compare('PIN', 'pin'));
		expect(compare('pin', 'PIN')).not.toBe(0);
	});

	it('puts a name with an accent beside the one without, not after the last letter', () => {
		expect(sorted(['zebra', 'éclair', 'eclair', 'apple', 'Eclair'])).toEqual([
			'apple',
			'Eclair',
			'eclair',
			'éclair',
			'zebra'
		]);
		// The same letter written as one character and as a letter with a
		// combining accent is still two names, and the two keep an order.
		const composed = 'café';
		const decomposed = 'cafe\u0301';
		expect(compare(composed, decomposed)).not.toBe(0);
		expect(sorted([composed, decomposed])).toEqual(sorted([decomposed, composed]));
	});

	it('puts a field with no name first, and does not trip on two of them', () => {
		expect(sorted(['b', '', 'a', ''])).toEqual(['', '', 'a', 'b']);
		expect(compare('', '')).toBe(0);
		expect(compare('', ' ')).toBeLessThan(0);
	});

	it('draws the same list whatever order the names arrive in', () => {
		const names = ['pin', 'PIN', 'Pin', 'Code 10', 'Code 2', 'é', 'e', ''];
		const first = JSON.stringify(sorted(names));
		// Forty thousand orders, so the first one drawn differently is kept and
		// asserted once rather than each one being asserted on its own.
		let differs: string[] | null = null;
		for (const order of orders(names)) {
			if (JSON.stringify(sorted(order)) === first) continue;
			differs = order;
			break;
		}
		expect(differs).toBeNull();
	});

	/**
	 * A comparison that is not a total order is one `sort` is free to answer
	 * with any arrangement at all, and a different one each time. Names a
	 * reader or another client could give a field - cases, accents, combining
	 * marks, digits of more than one script, overrides, spaces - checked
	 * against every rule a total order keeps.
	 */
	it('is a total order over a thousand names nobody chose', () => {
		const pieces = [
			'a',
			'A',
			'b',
			'B',
			'e',
			'E',
			'é',
			'É',
			'\u0301',
			'z',
			'Z',
			'ss',
			'ß',
			'i',
			'I',
			'İ',
			'ı',
			'0',
			'1',
			'2',
			'9',
			'10',
			'007',
			'\u0663',
			' ',
			'-',
			'_',
			'.',
			'\u202E',
			'\u200B',
			'\u05D0',
			'\u{1F511}'
		];
		const random = seeded(0xc0ffe5);
		const names = Array.from({ length: 1000 }, () => {
			let name = '';
			const length = Math.floor(random() * 6);
			for (let at = 0; at < length; at += 1) {
				name += pieces[Math.floor(random() * pieces.length)];
			}
			return name;
		});

		const drawn = sorted(names);
		expect(sorted([...names].reverse())).toEqual(drawn);
		// Collected rather than asserted pair by pair: half a million pairs, and
		// the first one that breaks a rule is the one worth reading.
		const broken: string[] = [];
		for (let at = 0; at < drawn.length && broken.length === 0; at += 1) {
			const name = drawn[at] as string;
			if (compare(name, name) !== 0) broken.push(`${JSON.stringify(name)} is not equal to itself`);
			for (let later = at + 1; later < drawn.length; later += 1) {
				const next = drawn[later] as string;
				const forward = compare(name, next);
				const back = compare(next, name);
				// Antisymmetric, and equal only for the same name: two names that
				// compare equal are two rows `sort` may put either way round.
				// Transitive: everything drawn after a name comes after it.
				const rule =
					Math.sign(forward) + Math.sign(back) !== 0
						? 'disagree'
						: name !== next && forward === 0
							? 'tie'
							: forward > 0
								? 'are drawn out of order'
								: null;
				if (rule === null) continue;
				broken.push(`${JSON.stringify(name)} and ${JSON.stringify(next)} ${rule}`);
				break;
			}
		}
		expect(broken).toEqual([]);
	});
});
