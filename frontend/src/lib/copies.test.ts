import { describe, expect, it } from 'vitest';
import { lastMade, nearby, overdue, refused, saved } from './copies';
import type { Elsewhere } from './model';

/** The suite runs with TZ pinned to UTC, so the local clock the screen writes
 * in is the same one Rust sends. */
const now = new Date('2026-10-02T14:30:00Z');

function elsewhere(over: Partial<Elsewhere> = {}): Elsewhere {
	return { otherDisk: null, sameDiskAt: null, overdue: null, ...over };
}

describe('the line under a copy on another disk', () => {
	it('says a copy was never made', () => {
		expect(lastMade(elsewhere(), now)).toBe('Last made: never');
	});

	/** Recently in words, and the day itself once that is a week or more ago -
	 * with the year once it is another year. */
	it('says when the last copy was made and on which disk', () => {
		const on = (at: string, volume: string | null = 'Stick') =>
			lastMade(elsewhere({ otherDisk: { at, volume } }), now);

		expect(on('2026-10-02T09:00:00Z')).toBe('Last made: today, on “⁨Stick⁩”');
		expect(on('2026-10-01T23:00:00Z')).toBe('Last made: yesterday, on “⁨Stick⁩”');
		expect(on('2026-09-29T10:00:00Z')).toBe('Last made: 3 days ago, on “⁨Stick⁩”');
		expect(on('2026-09-12T10:00:00Z')).toBe('Last made: 12 Sep, on “⁨Stick⁩”');
		expect(on('2025-12-30T10:00:00Z')).toBe('Last made: 30 Dec 2025, on “⁨Stick⁩”');
		expect(on('2026-09-29T10:00:00Z', null)).toBe('Last made: 3 days ago, on another disk');
	});

	/** A clock a little ahead is still a copy made today, not one counted
	 * backwards into nonsense. */
	it('says a copy dated later today is today', () => {
		const ahead = lastMade(
			elsewhere({ otherDisk: { at: '2026-10-02T20:00:00Z', volume: null } }),
			now
		);
		expect(ahead).toBe('Last made: today, on another disk');
	});

	/** The newest copy on the vault's own disk is the one a reader may take for
	 * a backup, so it is said for what it is, beside the line about the other
	 * disk - which may well say never. */
	it('says a copy on the vault’s own disk would go with it', () => {
		expect(nearby(elsewhere(), now)).toBeNull();
		expect(nearby(elsewhere({ sameDiskAt: '2026-10-02T10:00:00Z' }), now)).toBe(
			'The copy made today is on the same disk as the vault, and would not survive that disk failing.'
		);
		expect(nearby(elsewhere({ sameDiskAt: '2026-09-12T10:00:00Z' }), now)).toBe(
			'The copy made on 12 Sep is on the same disk as the vault, and would not survive that disk failing.'
		);
	});

	/** A disk's name is the reader's or somebody's who handed them the stick,
	 * and may turn the sentence round or carry markup. It stays text, isolated,
	 * and whole. */
	it('keeps the name of a disk off the rest of the sentence', () => {
		for (const volume of ['a‮b', '<img src=x onerror=alert(1)>', 'line\nbreak']) {
			const line = lastMade(elsewhere({ otherDisk: { at: '2026-10-02T09:00:00Z', volume } }), now);
			expect(line.endsWith('⁩”')).toBe(true);
			expect(line).toContain('⁨');
			expect(line.replace('\n', ' ')).toBe(line);
		}
		const markup = lastMade(
			elsewhere({ otherDisk: { at: '2026-10-02T09:00:00Z', volume: '<b>x</b>' } }),
			now
		);
		expect(markup).toContain('<b>x</b>');
	});
});

describe('the status bar’s line', () => {
	it('counts the days without a copy, and says a year rather than counting thousands', () => {
		expect(overdue(30)).toBe('No copy on another disk for 30 days');
		expect(overdue(364)).toBe('No copy on another disk for 364 days');
		expect(overdue(365)).toBe('No copy on another disk for over a year');
		expect(overdue(20_587)).toBe('No copy on another disk for over a year');
	});
});

describe('what a copy saved says', () => {
	const after = { elsewhere: elsewhere() };

	it('names the disk the copy went to, or says it is another', () => {
		expect(saved({ ...after, sameDisk: false, volume: 'Stick' })).toBe('Copy saved on “⁨Stick⁩”.');
		expect(saved({ ...after, sameDisk: false, volume: null })).toBe('Copy saved on another disk.');
	});

	/** Whatever the disk is called: a stick the vault itself lives on is the
	 * vault's disk, and a copy there is no copy on another. */
	it('says a copy on the vault’s own disk is there, whatever the disk is called', () => {
		for (const volume of [null, 'Stick']) {
			expect(saved({ ...after, sameDisk: true, volume })).toBe(
				'Copy saved on the same disk as the vault, where it would not survive that disk failing.'
			);
		}
	});

	/** A name that holds a file is never written over, and the reader is told
	 * so in words about the copy rather than Rust's, which are about making a
	 * vault. Every other refusal is Rust's sentence, and a throw that is not
	 * one is the window's general one. */
	it('says nothing was replaced when the name was taken', () => {
		expect(refused({ code: 'taken', message: 'there is already a file with that name' })).toBe(
			'A file by that name is already there, so nothing was replaced. Save the copy under another name.'
		);
		expect(
			refused({ code: 'refused', message: 'open the vault itself to keep a copy of it' })
		).toBe('open the vault itself to keep a copy of it');
		expect(refused(new Error('boom'))).toBe('Coffer could not finish that.');
	});
});
