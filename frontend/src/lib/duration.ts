/**
 * How long something lasts, said in words.
 *
 * A settings row is read once rather than watched, so it wants words: five
 * minutes rather than three hundred.
 */

/** "45 seconds", "5 minutes", "1 hour". */
export function named(seconds: number): string {
	if (seconds < 60) return plural(seconds, 'second');
	if (seconds < 3600) return plural(Math.round(seconds / 60), 'minute');
	return plural(Math.round(seconds / 3600), 'hour');
}

function plural(count: number, unit: string): string {
	return `${count} ${unit}${count === 1 ? '' : 's'}`;
}
