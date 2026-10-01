/**
 * A value written in lines: how many it has, and which Return finishes it.
 *
 * The format has no multi-line flag. A value is written in lines because it has
 * a line break in it, so that is what the window reads - from the value the
 * vault holds, and from the value as it stands while the reader writes it.
 */

/**
 * How tall a field grows before it scrolls inside itself.
 *
 * A field that grew without end would be as tall as a megabyte of text in a
 * pane that has to be scrolled past it to reach anything under it.
 */
const TALLEST = 20;

/** How many lines a value has, counted up to the tallest a field grows: the
 * count stops there, so a megabyte is not read to the end on every key. A CRLF
 * is one break and so is a CR on its own, the way a text area reads them. */
export function lines(text: string): number {
	let found = 1;
	for (let at = 0; at < text.length && found < TALLEST; at += 1) {
		const unit = text.charCodeAt(at);
		if (unit === 10 || (unit === 13 && text.charCodeAt(at + 1) !== 10)) found += 1;
	}
	return found;
}

/**
 * Whether a Return finishes what is being written rather than starting a line.
 *
 * A value on one line finishes on Return, the way every single line on a Mac
 * does, and Option-Return is the Mac's own way of putting a break into one. A
 * value already in lines takes Return as the next line, and Cmd-Return
 * finishes it. A Return that ends a composition - an accent, a kana - is the
 * input method's and never the field's.
 */
export function finishes(event: KeyboardEvent, lined: boolean): boolean {
	if (event.key !== 'Enter' || event.isComposing) return false;
	return lined ? event.metaKey : !event.altKey;
}
