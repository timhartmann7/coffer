/**
 * What the window says about copies of the vault kept on another disk. Rust
 * decides when one is due, and which disk the last went to; these are the
 * sentences, and the one place each lives.
 */

import { ago, day, quoted, recently } from './format';
import { asFailure } from './ipc';
import type { Elsewhere, SavedCopy } from './model';

/** The line under "Copy on another disk" in the settings: when the last copy
 * on another disk was made, and on which. */
export function lastMade(elsewhere: Elsewhere, now: Date): string {
	const away = elsewhere.otherDisk;
	if (!away) return 'Last made: never';
	const on = away.volume === null ? 'another disk' : quoted(away.volume);
	return `Last made: ${recently(away.at, now) ?? day(away.at, now)}, on ${on}`;
}

/** What the settings say under it while the newest copy is on the vault's own
 * disk, which a reader may have taken for a backup. Nothing otherwise. */
export function nearby(elsewhere: Elsewhere, now: Date): string | null {
	if (elsewhere.sameDiskAt === null) return null;
	return `The copy made ${ago(elsewhere.sameDiskAt, now)} is on the same disk as the vault, and would not survive that disk failing.`;
}

/** The status bar's line once a month has gone by without a copy on another
 * disk. A year on, it stops counting: a number of days nobody reads is not a
 * reminder. */
export function overdue(days: number): string {
	return days < 365
		? `No copy on another disk for ${days} days`
		: 'No copy on another disk for over a year';
}

/** What the notice says once a copy is written: where it went, and, when that
 * is the vault's own disk, what that means. */
export function saved(copy: SavedCopy): string {
	if (copy.sameDisk)
		return 'Copy saved on the same disk as the vault, where it would not survive that disk failing.';
	return copy.volume === null
		? 'Copy saved on another disk.'
		: `Copy saved on ${quoted(copy.volume)}.`;
}

/**
 * Why a copy was not saved. A name that holds a file is said in words about
 * the copy, because Rust's are about making a vault: the reader has to hear
 * that nothing was replaced, and what to do instead. Everything else is
 * Rust's sentence.
 */
export function refused(thrown: unknown): string {
	const failure = asFailure(thrown);
	return failure.code === 'taken'
		? 'A file by that name is already there, so nothing was replaced. Save the copy under another name.'
		: failure.message;
}
