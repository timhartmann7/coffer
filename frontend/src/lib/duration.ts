/**
 * How long something lasts, said in words.
 *
 * One shape: a settings row is read once and wants "5 minutes". The clock face
 * that went with it belonged to a countdown in the status bar, and that ticked
 * once a second in the corner of every screen for no reason a reader could act
 * on.
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
