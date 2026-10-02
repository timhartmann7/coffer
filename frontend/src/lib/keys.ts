/**
 * What a key press is aimed at.
 *
 * The window has shortcuts of its own on the keys every Mac application also
 * uses, so each of them first has to decide whether the key was meant for it or
 * for whatever the reader is doing. The rule is in one place because the window
 * and the rows of an entry both ask it, and two copies of it would answer the
 * same key two ways.
 */

/** Whether the key went to somewhere the reader is writing. What is in there
 * is theirs, and so are the keys that edit it, copy included. */
export function typing(target: EventTarget | null): boolean {
	return (
		target instanceof HTMLInputElement ||
		target instanceof HTMLTextAreaElement ||
		(target instanceof HTMLElement && target.isContentEditable)
	);
}

/** The kinds of input whose text the reader types and edits. */
const TEXT = new Set(['text', 'search', 'password', 'email', 'url', 'tel', 'number']);

/**
 * Whether a right-click is in a field the reader can type into, where WebKit's
 * own menu - Cut, Copy, Paste, spelling - is about their typing and stays. A
 * slider, a field that is disabled or read only, and everything else are not,
 * and get Coffer's menu or none.
 */
export function writable(target: EventTarget | null): boolean {
	if (target instanceof HTMLInputElement)
		return TEXT.has(target.type) && !target.disabled && !target.readOnly;
	if (target instanceof HTMLTextAreaElement) return !target.disabled && !target.readOnly;
	return target instanceof HTMLElement && target.isContentEditable;
}

/**
 * Whether a key is Cmd+C for Coffer rather than for the system.
 *
 * Not while the reader is writing, which is their own text. And not while
 * anything is selected: the system's copy then fires, and one that reaches a
 * revealed value is sent to Rust instead (see `guard.ts`), with the part that
 * was selected rather than the whole field.
 *
 * A selection inside a field is reported collapsed - WebKit never exposes a
 * position inside a control's own shadow tree - so the selection test is about
 * the page, and `typing` is what answers for a field.
 */
export function copying(event: KeyboardEvent): boolean {
	return (
		event.metaKey &&
		!event.shiftKey &&
		!event.altKey &&
		!event.ctrlKey &&
		event.key.toLowerCase() === 'c' &&
		!typing(event.target) &&
		document.getSelection()?.isCollapsed !== false
	);
}
