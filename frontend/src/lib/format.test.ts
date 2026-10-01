import { describe, expect, it } from 'vitest';
import { entry, field } from './fixtures';
import { ago, called, day, fully, size, when } from './format';

/** The suite runs with TZ pinned to UTC, so the local clock the screen writes
 * in is the same one the database keeps. */
const now = new Date('2026-08-29T14:30:00Z');

describe('a date as the list writes it', () => {
	it('gives the time for today and the day for anything older', () => {
		expect(when('2026-08-29T13:08:00Z', now)).toBe('13:08');
		expect(when('2026-08-29T09:05:00Z', now)).toBe('9:05');
		expect(when('2026-07-01T10:00:00Z', now)).toBe('1 Jul');
		expect(when('2024-03-04T10:00:00Z', now)).toBe('4 Mar 2024');
	});

	it('writes nothing for an entry with no date and for a date it cannot read', () => {
		expect(when(null, now)).toBe('');
		expect(when('', now)).toBe('');
		expect(when('not a date', now)).toBe('');
		expect(when('0000-00-00T00:00:00Z', now)).toBe('');
	});

	/** KDBX can hold dates centuries either side of now, and the fixture does. */
	it('writes the years at the edges of what the format can hold', () => {
		expect(when('1600-01-01T00:00:00Z', now)).toBe('1 Jan 1600');
		expect(when('3000-12-31T23:59:59Z', now)).toBe('31 Dec 3000');
	});

	it('says "today" on the entry screen and the full date everywhere else', () => {
		expect(fully('2026-08-29T13:08:00Z', now)).toBe('today at 13:08');
		expect(fully('2024-03-12T18:42:00Z', now)).toBe('12 Mar 2024');
		expect(fully(null, now)).toBe('unknown');
	});
});

describe('an attachment size', () => {
	it('keeps to three or four digits', () => {
		expect(size(0)).toBe('0 B');
		expect(size(411)).toBe('411 B');
		expect(size(1023)).toBe('1023 B');
		expect(size(1024)).toBe('1.0 KB');
		expect(size(1536)).toBe('1.5 KB');
		expect(size(100 * 1024)).toBe('100 KB');
		expect(size(3 * 1024 * 1024)).toBe('3.0 MB');
		expect(size(100 * 1024 * 1024)).toBe('100 MB');
		expect(size(2 * 1024 * 1024 * 1024)).toBe('2.0 GB');
	});
});

describe('a day as a sentence says it', () => {
	it('says today, the day this year, and the year as well for any other', () => {
		expect(day('2026-08-29T01:00:00Z', now)).toBe('today');
		expect(day('2026-08-27T10:00:00Z', now)).toBe('27 Aug');
		expect(day('2025-09-30T10:00:00Z', now)).toBe('30 Sep 2025');
		expect(day(null, now)).toBe('');
		expect(day('not a date', now)).toBe('');
	});

	/** Counted between midnights: an evening is yesterday the next morning,
	 * however few hours ago it was. */
	it('counts days back the way a person does', () => {
		const morning = new Date('2026-08-29T00:10:00Z');
		expect(ago('2026-08-28T23:50:00Z', morning)).toBe('yesterday');
		expect(ago('2026-08-29T00:00:00Z', morning)).toBe('today');
		expect(ago('2026-08-26T09:00:00Z', now)).toBe('3 days ago');
		expect(ago('2026-08-23T09:00:00Z', now)).toBe('6 days ago');
		expect(ago('2026-08-22T09:00:00Z', now)).toBe('on 22 Aug');
		expect(ago('2024-03-04T10:00:00Z', now)).toBe('on 4 Mar 2024');
		expect(ago(null, now)).toBe('');
	});

	/** A date after today is somebody else's clock, and the edges of what the
	 * format holds are dates like any other. Neither is counted into nonsense. */
	it('writes a date in the future or at the edge of the format as the day', () => {
		expect(ago('2026-08-30T09:00:00Z', now)).toBe('on 30 Aug');
		expect(ago('3000-12-31T23:59:59Z', now)).toBe('on 31 Dec 3000');
		expect(ago('1600-01-01T00:00:00Z', now)).toBe('on 1 Jan 1600');
		expect(ago('0050-06-01T00:00:00Z', now)).toBe('on 1 Jun 50');
	});
});

describe('what a sentence calls an entry', () => {
	const titled = (value: string | null) =>
		entry({ fields: [field({ name: 'Title', kind: 'title', value, empty: value === '' })] });

	it('quotes the title, and says "this entry" when there is none to show', () => {
		expect(called(titled('Bank'))).toBe('“Bank”');
		expect(called(titled(''))).toBe('this entry');
		// Protected: the value is not here, and a notice is no reason to fetch it.
		expect(called(titled(null))).toBe('this entry');
		expect(called(entry({ fields: [] }))).toBe('this entry');
	});
});
