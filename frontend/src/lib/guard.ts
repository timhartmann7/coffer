/**
 * What the system may do with a revealed value on the screen: nothing that Rust
 * does not do instead.
 *
 * A revealed value can be selected, because a reader reading ten recovery codes
 * off the screen needs to keep their place. What the system does with a
 * selection is the problem. Its copy is an ordinary pasteboard write with none
 * of the markers Coffer's own carries - no concealed type, nothing clearing it
 * after a minute, and a phone on the same account gets it - which is how a
 * password lands in Maccy and stays there. Its menu under the pointer offers
 * Look Up, Translate, Search and Share, each of which hands the value to
 * another application. And a selection can be dragged out of the window into
 * any other.
 *
 * So a revealed value takes all three, wherever they start. A selection does
 * not end at the value's edges: it can begin in a label, run through the value
 * and end in the next row, and the copy, the menu and the drag then belong to
 * whichever node the pointer or the selection's start is on - chrome, not the
 * value. The rules are therefore the document's, and ask one question of every
 * copy, cut, menu and drag: does it reach a value on the screen?
 *
 * A copy that reaches one value is cancelled and handed to Rust with the part
 * of the value that was selected, which writes it the way every other copy in
 * Coffer is written; the chrome selected around it is dropped, because the
 * chrome is not part of any field. A copy that reaches two values copies
 * nothing: there is no one field in the vault to cut it out of, and the
 * system's copy of both is the copy this module exists to stop. A cut is the
 * same copy, because nothing on the screen can be cut out of the vault. The menu
 * is not drawn and the drag does not start.
 */

import type { Attachment } from 'svelte/attachments';
import type { Span } from './model';

/** Every node a value is written into, and where a copy of it goes. */
const seals = new Map<HTMLElement, (range: Span | null) => void>();

const KINDS = ['copy', 'cut', 'contextmenu', 'dragstart'] as const;

/** Attaches the rules above to the node a revealed value is written into.
 * `copy` is where a copy goes instead: Rust, by entry and field, with the part
 * of the value the reader selected or `null` for all of it. */
export function sealed(copy: (range: Span | null) => void): Attachment<HTMLElement> {
	return (node) => {
		if (seals.size === 0) for (const kind of KINDS) document.addEventListener(kind, judge, true);
		seals.set(node, copy);
		return () => {
			seals.delete(node);
			if (seals.size === 0)
				for (const kind of KINDS) document.removeEventListener(kind, judge, true);
		};
	};
}

/** Taken in the capture phase, before anything in the page or WebKit's own
 * handling can act on the event. */
function judge(event: Event) {
	const reached = reach(event);
	if (reached.length === 0) return;
	event.preventDefault();
	if (event.type !== 'copy' && event.type !== 'cut') return;

	const [only] = reached;
	if (reached.length === 1 && only) only.copy(only.range);
}

/**
 * The values on the screen an event reaches, each with the part of it the
 * selection covers: `null` for all of it.
 *
 * A node that holds nothing is not one: there is no value in it to protect, and
 * every row keeps its node on the screen, emptied, while its value is hidden.
 * A selection that covers none of a value's characters does not reach it,
 * though it may touch its edge. The event's own target reaches the value it is
 * inside, whole, when the selection does not cover any of it.
 */
function reach(event: Event): { copy: (range: Span | null) => void; range: Span | null }[] {
	const selection = document.getSelection();
	const chosen =
		selection && selection.rangeCount > 0 && !selection.isCollapsed
			? selection.getRangeAt(0)
			: null;
	const target = event.target instanceof Node ? event.target : null;

	const reached = [];
	for (const [node, copy] of seals) {
		const text = node.firstChild;
		if (!(text instanceof Text) || text.length === 0) continue;

		const span = chosen ? covered(node, text, chosen) : null;
		if (span) {
			const whole = span.from === 0 && span.to === text.length;
			reached.push({ copy, range: whole ? null : span });
		} else if (target && node.contains(target)) {
			reached.push({ copy, range: null });
		}
	}
	return reached;
}

/**
 * The part of the node's value a selection covers, in UTF-16 code units, or
 * `null` when it covers none of it.
 *
 * Worked out from the positions the selection reports and never from its text:
 * reading the text would put the secret in a JavaScript string, which is the
 * one place a revealed value is kept out of. The value is a single text node -
 * `place` writes it as one - so a position inside that node is already an
 * offset into the value, and a selection that runs past either edge of the node
 * is cut there.
 */
function covered(node: HTMLElement, text: Text, chosen: Range): Span | null {
	const whole = document.createRange();
	whole.selectNodeContents(node);

	const offset = (container: Node, at: number): number => {
		const where = whole.comparePoint(container, at);
		if (where < 0) return 0;
		if (where > 0) return text.length;
		if (container === text) return at;
		// A position on the node itself counts its children, and it has one:
		// before the value, or after it.
		return at === 0 ? 0 : text.length;
	};

	const from = offset(chosen.startContainer, chosen.startOffset);
	const to = offset(chosen.endContainer, chosen.endOffset);
	return from < to ? { from, to } : null;
}
