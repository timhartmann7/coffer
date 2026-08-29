/**
 * How long something lasts, said in words and said on a clock.
 *
 * Two shapes, because the two places a duration appears want different things.
 * A settings row is read once and wants "5 minutes"; a countdown is watched and
 * wants a number that is obviously going down.
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

/** "4:12". Minutes and seconds, however many minutes there are. */
export function clock(seconds: number): string {
	const whole = Math.max(0, Math.floor(seconds));
	return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
}
