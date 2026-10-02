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

/**
 * Moves the focus one line through a list with the arrows: down from the line
 * that has it, or up, round from the last to the first and back. From outside
 * the list - the button that opened it - down lands on the first line and up
 * on the last. `role` names the lines, so a heading among them is stepped
 * over, and a list with none is left as it is.
 */
export function step(list: HTMLElement | undefined, role: string, key: string): void {
	const lines = [...(list?.querySelectorAll<HTMLElement>(`[role="${role}"]`) ?? [])];
	if (lines.length === 0) return;
	const at = lines.indexOf(document.activeElement as HTMLElement);
	const by = key === 'ArrowDown' ? 1 : -1;
	const next =
		at === -1 ? (by === 1 ? 0 : lines.length - 1) : (at + by + lines.length) % lines.length;
	lines[next].focus();
}
