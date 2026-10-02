/**
 * The order things with names are drawn in, as a reader looks for them.
 *
 * Numbers inside a name are read as numbers, so "Code 2" comes before "Code
 * 10", and a capital is not a reason to stand apart, so "pin" is beside "PIN"
 * rather than after every name that starts with one. Two names that only
 * differ that way keep a fixed order between them, so a list does not change
 * its mind from one drawing to the next.
 */

const READING = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });

export function byName(a: { name: string }, b: { name: string }): number {
	return READING.compare(a.name, b.name) || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
}
