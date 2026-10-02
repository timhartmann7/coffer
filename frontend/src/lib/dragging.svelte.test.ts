import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Dragging, type Ends } from './dragging.svelte';
import { group, row } from './fixtures';
import type { Moving } from './places';

/**
 * A folders pane as the window draws one, reduced to what a drag reads: places
 * that take a drop, one of them folded, inside the area whose empty part puts
 * the pane away.
 */
let pane: HTMLElement;
let source: HTMLElement;
let work: HTMLElement;
let folded: HTMLElement;
let elsewhere: HTMLElement;
/** What `elementFromPoint` answers, which happy-dom cannot work out. */
let under: Element | null = null;

const moving: Moving = { entries: [row({ group: 'personal' })] };

function place(drop: string, into: string, opens?: string): HTMLElement {
	const made = document.createElement('div');
	made.dataset.drop = drop;
	made.dataset.into = into;
	if (opens) made.dataset.opens = opens;
	made.append(document.createElement('span'));
	pane.append(made);
	return made;
}

beforeEach(() => {
	pane = document.createElement('div');
	document.body.append(pane);
	source = document.createElement('button');
	pane.append(source);
	work = place('work', 'work');
	folded = place('banking', 'banking', 'banking');
	elsewhere = place('bin', 'bin');
	under = null;
	vi.spyOn(document, 'elementFromPoint').mockImplementation(() => under);
});

afterEach(async () => {
	// A drag that lifted swallows the click after it until the next task, and
	// that task comes before the next test: no test's click is another's.
	if (vi.isFakeTimers()) vi.runOnlyPendingTimers();
	vi.useRealTimers();
	await new Promise((resolve) => setTimeout(resolve, 0));
	pane.remove();
	vi.restoreAllMocks();
});

/** The screen's side of a drag, with the bin left out of where it lands. */
function ends() {
	return {
		targets: vi.fn<Ends['targets']>(() => new Set(['work', 'banking'])),
		drop: vi.fn<Ends['drop']>(),
		open: vi.fn<Ends['open']>()
	};
}

function pointer(type: string, x: number, y = 0, over: PointerEventInit = {}): PointerEvent {
	return new PointerEvent(type, { clientX: x, clientY: y, button: 0, bubbles: true, ...over });
}

/** The pointer moved to `x` over `target`. */
function moveTo(x: number, target: Element | null) {
	under = target;
	window.dispatchEvent(pointer('pointermove', x));
}

function release() {
	window.dispatchEvent(pointer('pointerup', 0));
}

describe('a drag with the pointer', () => {
	it('a press that does not travel is a press, and opens what it pressed', () => {
		const screen = ends();
		const dragging = new Dragging(screen);
		const clicked = vi.fn();
		pane.addEventListener('click', clicked);

		dragging.press(pointer('pointerdown', 10), moving);
		moveTo(13, work);
		expect(dragging.moving, 'three pixels lifted it').toBeNull();
		release();
		source.click();

		expect(screen.drop).not.toHaveBeenCalled();
		expect(screen.targets).not.toHaveBeenCalled();
		expect(clicked, 'the press lost its click').toHaveBeenCalledTimes(1);
	});

	it('lifts after four pixels and drops only where it is taken', () => {
		const screen = ends();
		const dragging = new Dragging(screen);

		dragging.press(pointer('pointerdown', 10), moving);
		moveTo(14, null);
		expect(dragging.moving).toBe(moving);
		expect(screen.targets).toHaveBeenCalledTimes(1);

		moveTo(40, elsewhere.firstElementChild);
		expect(dragging.over, 'a place that does not take it lit').toBeNull();
		moveTo(60, work.firstElementChild);
		expect(dragging.over).toBe('work');
		moveTo(80, elsewhere);
		expect(dragging.over).toBeNull();
		expect(screen.targets, 'the places were worked out again').toHaveBeenCalledTimes(1);
		release();

		expect(screen.drop, 'a drop where it is not taken').not.toHaveBeenCalled();
		expect(dragging.moving).toBeNull();

		dragging.press(pointer('pointerdown', 10), moving);
		moveTo(30, work);
		release();
		expect(screen.drop).toHaveBeenCalledWith(moving, 'work');
	});

	it('Escape lets go of a drag and goes no further', () => {
		const screen = ends();
		const dragging = new Dragging(screen);
		const heard = vi.fn();
		window.addEventListener('keydown', heard);

		// Before it lifts, Escape is the window's as always.
		dragging.press(pointer('pointerdown', 10), moving);
		window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
		expect(heard).toHaveBeenCalledTimes(1);

		moveTo(30, work);
		const escape = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
		window.dispatchEvent(escape);
		expect(dragging.moving).toBeNull();
		expect(dragging.over).toBeNull();

		// The button is still held: the pointer goes on moving, lifts nothing
		// and lights nothing, and a second Escape is the window's again.
		moveTo(60, work);
		expect(dragging.moving, 'the drag lifted again after Escape').toBeNull();
		expect(dragging.over).toBeNull();
		window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
		expect(heard).toHaveBeenCalledTimes(2);

		// The release ends in a click on what holds both ends of the drag, the
		// empty part of the pane, and that click is the drag's as well.
		const clicked = vi.fn();
		pane.addEventListener('click', clicked);
		release();
		pane.click();

		expect(clicked, 'the click after the release put the pane away').not.toHaveBeenCalled();
		expect(escape.defaultPrevented).toBe(true);
		expect(screen.drop).not.toHaveBeenCalled();
		pane.click();
		expect(clicked, 'a later click was swallowed too').toHaveBeenCalledTimes(1);
		window.removeEventListener('keydown', heard);
	});

	/** The press and the release land on different elements, and the click
	 * goes to what holds both: the empty part of the folders pane, which puts
	 * the open entry away. */
	it('the click that ends a drag reaches nothing', async () => {
		const screen = ends();
		const dragging = new Dragging(screen);
		const clicked = vi.fn();
		pane.addEventListener('click', clicked);

		dragging.press(pointer('pointerdown', 10), moving);
		moveTo(30, work);
		release();
		pane.click();
		expect(clicked, 'the click after the drag reached the pane').not.toHaveBeenCalled();
		expect(screen.drop).toHaveBeenCalledTimes(1);

		pane.click();
		expect(clicked, 'a later click was swallowed too').toHaveBeenCalledTimes(1);

		// A drag whose click never came does not take the next one with it.
		dragging.press(pointer('pointerdown', 10), moving);
		moveTo(30, work);
		release();
		await new Promise((resolve) => setTimeout(resolve, 0));
		pane.click();
		expect(clicked).toHaveBeenCalledTimes(2);
	});

	it('a folded folder opens when the pointer rests on it, and not when it passes', () => {
		vi.useFakeTimers();
		const screen = ends();
		const dragging = new Dragging(screen);

		dragging.press(pointer('pointerdown', 10), moving);
		moveTo(30, folded.firstElementChild);
		vi.advanceTimersByTime(500);
		moveTo(40, work);
		vi.advanceTimersByTime(500);
		moveTo(50, folded);
		vi.advanceTimersByTime(699);
		expect(screen.open, 'it opened on a pass').not.toHaveBeenCalled();
		// Resting is resting: a step inside the same folder does not start
		// the clock again.
		moveTo(51, folded.firstElementChild);
		vi.advanceTimersByTime(1);
		expect(screen.open).toHaveBeenCalledWith('banking');
		expect(screen.open).toHaveBeenCalledTimes(1);

		release();
		vi.advanceTimersByTime(5000);
		expect(screen.open).toHaveBeenCalledTimes(1);
	});

	it('a right click, a Control press and a second button do not start a drag', () => {
		const screen = ends();
		const dragging = new Dragging(screen);

		for (const over of [{ button: 2 }, { ctrlKey: true }, { button: 1 }]) {
			dragging.press(pointer('pointerdown', 10, 0, over), moving);
			moveTo(60, work);
			release();
		}

		expect(screen.targets).not.toHaveBeenCalled();
		expect(screen.drop).not.toHaveBeenCalled();
		expect(dragging.moving).toBeNull();
	});

	/** A drag that ends without its release - a cancelled pointer, the window
	 * losing focus, the screen going - leaves nothing behind. A swallow left
	 * waiting would take the next click the reader makes anywhere. */
	it('a cancelled pointer and a window losing focus drop nothing and leave no listener', () => {
		const screen = ends();
		const dragging = new Dragging(screen);
		const added = vi.spyOn(window, 'addEventListener');
		const removed = vi.spyOn(window, 'removeEventListener');
		const clicked = vi.fn();
		pane.addEventListener('click', clicked);

		for (const ending of [
			() => window.dispatchEvent(pointer('pointercancel', 30)),
			() => window.dispatchEvent(new Event('blur')),
			() => dragging.cancel()
		]) {
			dragging.press(pointer('pointerdown', 10), moving);
			moveTo(30, work);
			expect(dragging.over).toBe('work');
			ending();
			expect(dragging.moving).toBeNull();
			expect(dragging.over).toBeNull();
			release();
			pane.click();
		}

		expect(screen.drop).not.toHaveBeenCalled();
		expect(clicked, 'a click after a drag that never ended in one').toHaveBeenCalledTimes(3);
		// Every listener added is taken off again, the same function in the same
		// phase, after it was added.
		const capture = (options: unknown) =>
			typeof options === 'boolean'
				? options
				: Boolean((options as AddEventListenerOptions)?.capture);
		added.mock.calls.forEach(([type, listener, options], at) => {
			const since = added.mock.invocationCallOrder[at];
			const off = removed.mock.calls.some(
				([was, gone, how], which) =>
					was === type &&
					gone === listener &&
					capture(how) === capture(options) &&
					removed.mock.invocationCallOrder[which] > since
			);
			expect(off, `the ${type} listener stayed`).toBe(true);
		});
		expect(added).toHaveBeenCalled();

		// Nothing is listening any more: a move after it lifts nothing.
		moveTo(90, work);
		expect(dragging.moving).toBeNull();
	});

	/** However a drag ends, a folder the pointer was resting on stays folded:
	 * a folder opening on its own after the reader has let go is a change
	 * nobody asked for. */
	it('a drag let go before a folded folder opens leaves it folded', () => {
		vi.useFakeTimers();
		const screen = ends();
		const dragging = new Dragging(screen);

		for (const ending of [
			() =>
				window.dispatchEvent(
					new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
				),
			// Dropped on the folded folder itself, which takes it.
			release,
			() => window.dispatchEvent(pointer('pointercancel', 30)),
			() => window.dispatchEvent(new Event('blur')),
			() => dragging.cancel()
		]) {
			dragging.press(pointer('pointerdown', 10), moving);
			moveTo(30, folded.firstElementChild);
			vi.advanceTimersByTime(500);
			ending();
			release();
			vi.advanceTimersByTime(1000);
		}

		expect(screen.open, 'a folder opened after the drag ended').not.toHaveBeenCalled();
	});

	/** A lock, or the screen going, while a drag is in the air. */
	it('lets go when the screen goes, and moves nothing', () => {
		const screen = ends();
		const dragging = new Dragging(screen);

		dragging.press(pointer('pointerdown', 10), { folder: group({ name: 'Work' }) });
		moveTo(30, work);
		dragging.cancel();
		moveTo(40, work);
		release();

		expect(dragging.moving).toBeNull();
		expect(screen.drop).not.toHaveBeenCalled();
	});
});
