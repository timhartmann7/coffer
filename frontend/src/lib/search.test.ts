import { describe, expect, it } from 'vitest';
import { row } from './fixtures';
import { index, search } from './search';

const find = (rows: ReturnType<typeof row>[], query: string) =>
	search(index(rows), query).map((found) => found.title);

describe('the instant filter', () => {
	it('searches the four things a row carries and nothing else', () => {
		const rows = [
			row({ title: 'Hetzner' }),
			row({ title: 'by login', username: 'deploy' }),
			row({ title: 'by address', url: 'https://console.hetzner.cloud' }),
			row({ title: 'by tag', tags: ['prod', 'ssh'] })
		];

		expect(find(rows, 'deploy')).toEqual(['by login']);
		expect(find(rows, 'console')).toEqual(['by address']);
		expect(find(rows, 'ssh')).toEqual(['by tag']);
		expect(find(rows, 'hetzner')).toEqual(['Hetzner', 'by address']);
	});

	it('ignores case on both sides', () => {
		const rows = [row({ title: 'PostgreSQL' })];
		expect(find(rows, 'postgresql')).toEqual(['PostgreSQL']);
		expect(find(rows, 'POSTGRE')).toEqual(['PostgreSQL']);
	});

	/** The fixture database holds a title written with a combining accent. A
	 * reader typing the composed form is typing the same word. */
	it('finds a title however its accents are composed', () => {
		// The title carries a combining accent, the query a composed one. The
		// fixture database holds a title written the first way.
		const rows = [row({ title: 'cafe\u0301 account' })];
		expect(find(rows, 'caf\u00e9')).toEqual(['cafe\u0301 account']);
		expect(find(rows, 'cafe\u0301')).toEqual(['cafe\u0301 account']);
	});

	it('has nothing to search in a value the database protects', () => {
		const rows = [row({ title: null, username: null, url: null, tags: [] })];
		expect(find(rows, 'anything')).toEqual([]);
		expect(find(rows, '')).toEqual([null]);
	});

	it('treats an empty query as no filter at all', () => {
		const rows = [row({ title: 'one' }), row({ title: 'two' })];
		expect(find(rows, '')).toEqual(['one', 'two']);
	});

	it('takes a query of only spaces literally', () => {
		const rows = [row({ title: 'two words' }), row({ title: 'oneword' })];
		expect(find(rows, ' ')).toEqual(['two words']);
	});

	it('is not fooled by a right-to-left override in the value', () => {
		const rows = [row({ title: '‮gnitirw' })];
		expect(find(rows, 'gnitirw')).toEqual(['‮gnitirw']);
		expect(find(rows, '‮')).toEqual(['‮gnitirw']);
	});

	it('searches a megabyte of title without falling over', () => {
		const long = 'a'.repeat(1_000_000);
		const rows = [row({ title: `${long}needle` }), row({ title: long })];
		expect(find(rows, 'needle')).toHaveLength(1);
	});

	/** The attack list asks for fifty thousand entries. The list draws what the
	 * screen can hold; the filter has to answer for all of them, on every
	 * keystroke. */
	it('filters fifty thousand rows on every keystroke', () => {
		const rows = index(
			Array.from({ length: 50_000 }, (_, at) => row({ title: `entry ${at}`, username: 'deploy' }))
		);

		expect(search(rows, 'entry 49999')).toHaveLength(1);
		expect(search(rows, 'deploy')).toHaveLength(50_000);
		expect(search(rows, 'nothing here')).toHaveLength(0);
	});

	it('matches a value that is nothing but markup', () => {
		const rows = [row({ title: '<script>alert(1)</script>' })];
		expect(find(rows, '<script>')).toHaveLength(1);
	});

	/** The index is built once for a folder and searched on every keystroke, so
	 * it has to survive being searched again and again. */
	it('answers the same way however many times it is asked', () => {
		const rows = index([row({ title: 'stable' })]);
		for (const query of ['s', 'st', 'sta', '', 'stable', 'x']) {
			expect(search(rows, query).length).toBe(query === 'x' ? 0 : 1);
		}
	});
});
