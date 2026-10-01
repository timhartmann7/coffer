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
 * So a revealed value's node takes all three. A copy is cancelled and handed
 * to Rust with the part of the value that was selected, which writes it the
 * way every other copy in Coffer is written; a cut is the same copy, because
 * nothing on the screen can be cut out of the vault; the menu is not drawn; and
 * the drag does not start.
 */

import type { Attachment } from 'svelte/attachments';
import type { Span } from './model';

/** Attaches the rules above to the node a revealed value is written into.
 * `copy` is where a copy goes instead: Rust, by entry and field, with the part
 * of the value the reader selected or `null` for all of it. */
export function sealed(copy: (range: Span | null) => void): Attachment<HTMLElement> {
	return (node) => {
		const taken = (event: Event) => {
			event.preventDefault();
			copy(selected(node, document.getSelection()));
		};
		const refused = (event: Event) => event.preventDefault();

		node.addEventListener('copy', taken);
		node.addEventListener('cut', taken);
		node.addEventListener('contextmenu', refused);
		node.addEventListener('dragstart', refused);
		return () => {
			node.removeEventListener('copy', taken);
			node.removeEventListener('cut', taken);
			node.removeEventListener('contextmenu', refused);
			node.removeEventListener('dragstart', refused);
		};
	};
}

/**
 * The part of the node's value a selection covers, in UTF-16 code units, or
 * `null` for all of it.
 *
 * Worked out from the positions the selection reports and never from its text:
 * reading the text would put the secret in a JavaScript string, which is the
 * one place a revealed value is kept out of. The value is a single text node -
 * `place` writes it as one - so a position inside that node is already an
 * offset into the value, and a selection that runs past either edge of the node
 * is cut there.
 */
function selected(node: HTMLElement, selection: Selection | null): Span | null {
	const text = node.firstChild;
	if (!(text instanceof Text) || !selection || selection.rangeCount === 0) return null;
	if (selection.isCollapsed) return null;

	const chosen = selection.getRangeAt(0);
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
	if (from >= to || (from === 0 && to === text.length)) return null;
	return { from, to };
}
