/** Values as the screen writes them. */

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

function time(moment: Date): string {
	return `${moment.getHours()}:${String(moment.getMinutes()).padStart(2, '0')}`;
}

function month(moment: Date): string {
	return MONTHS[moment.getMonth()];
}

function round(value: number): string {
	return value < 10 ? value.toFixed(1) : String(Math.round(value));
}
