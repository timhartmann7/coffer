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
 * Whether a key belongs to an input method rather than to the field: an
 * accent being built, a kana being converted.
 *
 * `isComposing` alone is not enough in WebKit, which is the only engine Coffer
 * runs in. The Return that confirms a conversion there commonly arrives with
 * `isComposing` already false and the key code the platform gives every key an
 * input method has taken, 229 - and that Return saved half a converted word.
 */
export function composing(event: KeyboardEvent): boolean {
	return event.isComposing || event.keyCode === 229;
}

/**
 * Whether an Escape puts away what is being written.
 *
 * The Escape that cancels a conversion is the input method's for the same
 * reason the Return that confirms one is, and arrives the same way. Read as
 * the field's, it closed a Change and threw away everything typed into it.
 */
export function cancels(event: KeyboardEvent): boolean {
	return event.key === 'Escape' && !composing(event);
}

/**
 * Whether a Return finishes what is being written rather than starting a line.
 *
 * A value on one line finishes on Return, the way every single line on a Mac
 * does, and Option-Return is the Mac's own way of putting a break into one. A
 * value already in lines takes Return as the next line, and Cmd-Return
 * finishes it. A Return that ends a composition is the input method's and
 * never the field's.
 */
export function finishes(event: KeyboardEvent, lined: boolean): boolean {
	if (event.key !== 'Enter' || composing(event)) return false;
	return lined ? event.metaKey : !event.altKey;
}
