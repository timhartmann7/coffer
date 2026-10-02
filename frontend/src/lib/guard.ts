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
 * system's copy of both is the copy this module exists to stop. It says so,
 * because a Cmd+C that did nothing in silence leaves the reader pasting
 * whatever the pasteboard held before - another password, as like as not. A
 * cut is the same copy, because nothing on the screen can be cut out of the
 * vault. The drag does not start.
 *
 * WebKit's menu is not drawn either. A menu that reaches one value is Coffer's
 * own about that value - its Copy goes through Rust like every other copy -
 * and one that reaches two is no menu at all. What reaches a value is narrower
 * for a menu than for a copy: the pointer has to be on the value, or on the
 * selection that runs into it, and a right-click anywhere else is left to
 * whatever is under the pointer, which draws its own. But WebKit's menu is
 * about the selection, wherever the pointer is: while one still covers part
 * of a value, it is drawn nowhere, not even in a field being typed in, which
 * otherwise keeps it.
 */

import type { Attachment } from 'svelte/attachments';
import type { Span } from './model';

/** Where a copy of a value goes, where a copy refused is said, and where a
 * menu about the value is drawn, with the part of it the menu is about. A
 * value with nothing in the vault to copy by name - the generator's - draws
 * none. */
type Seal = {
	copy: (range: Span | null) => void;
	refuse: (thrown: unknown) => void;
	menu?: (event: MouseEvent, range: Span | null) => void;
};

/** Every node a value is written into, and what is done with a copy of it. */
const seals = new Map<HTMLElement, Seal>();

const KINDS = ['copy', 'cut', 'contextmenu', 'dragstart', 'mousedown'] as const;

/**
 * The selection as it stood when a right-click's button went down: a copy of
 * its range, `null` for none, and `undefined` when no right-click has gone down
 * since the last menu.
 *
 * WebKit selects the word under a right-click after the button goes down and
 * before the page hears of the menu, so the selection a menu finds is not the
 * reader's. This is.
 */
let before: Range | null | undefined;

/** Attaches the rules above to the node a revealed value is written into.
 * `copy` is where a copy goes instead: Rust, by entry and field, with the part
 * of the value the reader selected or `null` for all of it. `refuse` is where
 * a copy that reached more than this value is said to have copied nothing.
 * `menu` draws Coffer's menu about the value. */
export function sealed(
	copy: (range: Span | null) => void,
	refuse: (thrown: unknown) => void,
	menu?: (event: MouseEvent, range: Span | null) => void
): Attachment<HTMLElement> {
	return (node) => {
		if (seals.size === 0) for (const kind of KINDS) document.addEventListener(kind, judge, true);
		seals.set(node, { copy, refuse, menu });
		return () => {
			seals.delete(node);
			if (seals.size === 0) {
				for (const kind of KINDS) document.removeEventListener(kind, judge, true);
				before = undefined;
			}
		};
	};
}

/** Taken in the capture phase, before anything in the page or WebKit's own
 * handling can act on the event. */
function judge(event: Event) {
	if (event instanceof MouseEvent && event.type === 'mousedown') {
		pressed(event);
		return;
	}
	if (event instanceof MouseEvent && event.type === 'contextmenu') {
		pointed(event);
		return;
	}

	const reached = reach(event, live(), false);
	if (reached.length === 0) return;
	event.preventDefault();
	if (event.type !== 'copy' && event.type !== 'cut') return;

	const [first] = reached;
	if (!first) return;
	if (reached.length === 1) first.copy(first.range);
	else first.refuse({ code: 'refused', message: 'Select one value at a time to copy it.' });
}

/** The selection, when anything is selected. */
function live(): Range | null {
	const selection = document.getSelection();
	return selection && selection.rangeCount > 0 && !selection.isCollapsed
		? selection.getRangeAt(0)
		: null;
}

/** A button went down: the selection is kept for a right-click's menu, which
 * Control and a click is on a Mac as well, and let go of for anything else. */
function pressed(event: MouseEvent) {
	const secondary = event.button === 2 || (event.button === 0 && event.ctrlKey);
	before = secondary ? (live()?.cloneRange() ?? null) : undefined;
}

/**
 * A right-click. Which selection the menu is about is decided first, then
 * whether it reaches a value: one value is Coffer's menu about it, two are no
 * menu, and none leaves the event to whatever is under the pointer.
 *
 * - No button seen going down - a menu VoiceOver opened - is about the
 *   selection as it is.
 * - Nothing selected before, and something now: WebKit chose the word under
 *   the pointer, which is not a part the reader chose. The menu is about the
 *   value whole, and the word is let go.
 * - The selection from before, unchanged: the pointer was on it, and the menu
 *   is about it.
 * - Another selection now: the pointer was beside the reader's, and WebKit
 *   moved it. The reader's is put back, and the menu is about the value under
 *   the pointer, whole.
 *
 * The selection is put right only when a value is reached; anywhere else what
 * WebKit did is left alone. Nothing reached is the event left to whatever is
 * under the pointer, but never to WebKit while the selection it would act on
 * still covers a value: its Look Up and Share would be about that value. A
 * field being typed in, which no row's menu answers for, then has no menu.
 */
function pointed(event: MouseEvent) {
	const was = before;
	before = undefined;
	const now = live();
	let about = now;
	let settle = () => {};
	if (was === null && now) {
		about = null;
		settle = () => document.getSelection()?.removeAllRanges();
	} else if (was && !(now && same(now, was))) {
		about = null;
		settle = () => {
			const selection = document.getSelection();
			selection?.removeAllRanges();
			selection?.addRange(was);
		};
	}

	const reached = reach(event, about, true);
	if (reached.length === 0) {
		if (reach(event, now, false).length > 0) event.preventDefault();
		return;
	}
	settle();
	event.preventDefault();
	event.stopPropagation();
	const [only] = reached;
	if (reached.length === 1) only.menu?.(event, only.range);
}

function same(one: Range, other: Range): boolean {
	return (
		one.compareBoundaryPoints(Range.START_TO_START, other) === 0 &&
		one.compareBoundaryPoints(Range.END_TO_END, other) === 0
	);
}

/**
 * The values on the screen an event reaches, each with the part of it the
 * selection `chosen` covers: `null` for all of it.
 *
 * A node that holds nothing is not one: there is no value in it to protect, and
 * every row keeps its node on the screen, emptied, while its value is hidden.
 * A selection that covers none of a value's characters does not reach it,
 * though it may touch its edge. The event's own target reaches the value it is
 * inside, whole, when the selection does not cover any of it.
 *
 * For a menu (`pointer`), a selection reaches a value only when the pointer is
 * on it: the target inside the selection, and not a pane around it. A label
 * selected into a password, right-clicked, is a menu about the password; the
 * row beside it is not.
 */
function reach(
	event: Event,
	chosen: Range | null,
	pointer: boolean
): (Seal & { range: Span | null })[] {
	const target = event.target instanceof Node ? event.target : null;
	const under =
		chosen !== null &&
		(!pointer ||
			(target !== null &&
				chosen.intersectsNode(target) &&
				!target.contains(chosen.commonAncestorContainer)));

	const reached = [];
	for (const [node, seal] of seals) {
		const text = node.firstChild;
		if (!(text instanceof Text) || text.length === 0) continue;

		const span = chosen ? covered(node, text, chosen) : null;
		const inside = target !== null && node.contains(target);
		if (span && (under || inside)) {
			const whole = span.from === 0 && span.to === text.length;
			reached.push({ ...seal, range: whole ? null : span });
		} else if (inside) {
			reached.push({ ...seal, range: null });
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
