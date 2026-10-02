/** What a list that belongs to the control under it - the settings chip's, a
 * folder list - needs to know to close at the right moment. */

/**
 * Whether the focus is leaving the whole control, and not moving between its
 * parts: the field, the button that opened it, the lines of the list.
 *
 * A list closes when the focus leaves the control and not when it leaves the
 * part that opened it, because those are the same moment for a reader pressing
 * Tab - and a list that closed then could only ever be used with a pointer.
 * `relatedTarget` is what the focus went to.
 */
export function leaves(event: FocusEvent, control: HTMLElement | undefined): boolean {
	const going = event.relatedTarget;
	return !(going instanceof Node && control?.contains(going));
}
