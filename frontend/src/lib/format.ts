/** Values as the screen writes them. */

import type { Entry } from './model';

/**
 * A timestamp out of the database, in the reader's own clock.
 *
 * Today gets the time, this year gets the day and the month, anything older
 * gets the year as well. The list is a column of dates to glance down, not a
 * log, so the year is only there when it tells the reader something.
 */
export function when(stamp: string | null, now: Date): string {
	const moment = parse(stamp);
	if (!moment) return '';

	if (sameDay(moment, now)) {
		return time(moment);
	}
	if (moment.getFullYear() === now.getFullYear()) {
		return `${moment.getDate()} ${month(moment)}`;
	}
	return `${moment.getDate()} ${month(moment)} ${moment.getFullYear()}`;
}

/** The long form the entry screen puts at the bottom. */
export function fully(stamp: string | null, now: Date): string {
	const moment = parse(stamp);
	if (!moment) return 'unknown';

	if (sameDay(moment, now)) {
		return `today at ${time(moment)}`;
	}
	return `${moment.getDate()} ${month(moment)} ${moment.getFullYear()}`;
}

/**
 * A day as a sentence says it: "today", the day and the month for this year,
 * and the year as well for any other. Nothing for a date that is not one.
 */
export function day(stamp: string | null, now: Date): string {
	const moment = parse(stamp);
	if (!moment) return '';
	if (sameDay(moment, now)) return 'today';
	const date = `${moment.getDate()} ${month(moment)}`;
	return moment.getFullYear() === now.getFullYear() ? date : `${date} ${moment.getFullYear()}`;
}

/**
 * A moment as a sentence says it, to the minute: "today at 14:05", "on 12 Sep
 * at 14:05", with the year for any other year. Nothing for a date that is not
 * one. For the times of two files a reader is choosing between, where the day
 * alone would make a copy and the vault it was taken from look the same age.
 */
export function at(stamp: string | null, now: Date): string {
	const moment = parse(stamp);
	if (!moment) return '';
	const on = day(stamp, now);
	return `${on === 'today' ? on : `on ${on}`} at ${time(moment)}`;
}

/**
 * How long ago, the way a row in the recycle bin says it: "today",
 * "yesterday", a count of days for the week behind, and the day itself before
 * that. A date after today is a clock somebody else set, and it is written as
 * the day rather than counted backwards into nonsense.
 */
export function ago(stamp: string | null, now: Date): string {
	const moment = parse(stamp);
	if (!moment) return '';
	// Counted between midnights rather than in hours, so that yesterday evening
	// is yesterday this morning, and a clock change is not a day.
	const days = Math.round((midnight(now) - midnight(moment)) / 86_400_000);
	if (days === 0) return 'today';
	if (days === 1) return 'yesterday';
	if (days > 1 && days < 7) return `${days} days ago`;
	return `on ${day(stamp, now)}`;
}

/**
 * What a sentence calls an entry: its title in quotes, or "this entry" when
 * it has none to show - left empty, or kept protected by the database, which
 * no notice is a reason to reveal.
 */
export function called(entry: Entry): string {
	const title = entry.fields.find((field) => field.kind === 'title')?.value;
	return title ? `“${title}”` : 'this entry';
}

/** An attachment's size, in the unit that keeps it to three or four digits. */
export function size(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${round(bytes / 1024)} KB`;
	if (bytes < 1024 * 1024 * 1024) return `${round(bytes / (1024 * 1024))} MB`;
	return `${round(bytes / (1024 * 1024 * 1024))} GB`;
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/**
 * A KDBX timestamp is UTC and arrives saying so. Anything else - an empty
 * field, a date the format could hold but a calendar cannot - is not a date and
 * is not written as one.
 */
function parse(stamp: string | null): Date | null {
	if (!stamp) return null;
	const moment = new Date(stamp);
	return Number.isNaN(moment.getTime()) ? null : moment;
}

function sameDay(moment: Date, now: Date): boolean {
	return (
		moment.getFullYear() === now.getFullYear() &&
		moment.getMonth() === now.getMonth() &&
		moment.getDate() === now.getDate()
	);
}

function midnight(moment: Date): number {
	return new Date(moment.getTime()).setHours(0, 0, 0, 0);
}

function time(moment: Date): string {
	return `${moment.getHours()}:${String(moment.getMinutes()).padStart(2, '0')}`;
}

function month(moment: Date): string {
	return MONTHS[moment.getMonth()];
}

function round(value: number): string {
	return value < 10 ? value.toFixed(1) : String(Math.round(value));
}
