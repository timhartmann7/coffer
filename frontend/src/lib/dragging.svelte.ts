/**
 * A row or a folder dragged onto a folder with the pointer, from the press to
 * the drop.
 *
 * Not the system's drag and drop. That one puts what is dragged on the drag
 * pasteboard, where whatever application is under the pointer can read it,
 * and a row of a vault has no business there. So nothing leaves the window:
 * the press is followed with pointer events, the place under the pointer is
 * found with `elementFromPoint`, and the drop is a call into the screen.
 *
 * A place takes a drag when it carries `data-drop`, the key it lights up under,
 * and `data-into`, the folder a drop there goes into: "All entries" and "Not in
 * a folder" are two keys for the top of the vault. A folded folder carries
 * `data-opens`, and opens when the pointer rests on it.
 */

import type { Moving } from './places';

/**
 * How a place that would take the drag is drawn: the way the tree draws the
 * folder being shown, with the accent outline the focus ring is - inside the
 * line, so the pane does not clip it. Every place a drag can land is drawn
 * from this one string.
 */
export const LANDING = 'bg-raised text-txt outline-2 -outline-offset-2 outline-accent';

/** What the vault screen does with a drag. */
export interface Ends {
	/** Where `moving` may land, worked out once as it lifts. */
	targets(moving: Moving): Set<string>;
	drop(moving: Moving, into: string): void;
	/** Opens a folded folder the pointer rested on. */
	open(id: string): void;
}

/**
 * How far a press travels before it is a drag: the system's own threshold, so
 * a press that wobbles is still a press. A distance, not a drawn size.
 */
const TRAVEL = 4;

/** How long the pointer rests on a folded folder before it opens. */
const LINGER = 700;

/** Whether a press can start a drag: the primary button, and no Control,
 * which on a Mac makes it the other button. */
function primary(event: PointerEvent): boolean {
	return event.button === 0 && !event.ctrlKey;
}

/** Swallows the one click that ends a drag. */
function swallow(event: Event) {
	event.stopPropagation();
	event.preventDefault();
}

export class Dragging {
	/** What is being dragged, once the pointer has travelled far enough. */
	moving = $state.raw<Moving | null>(null);
	/** The `data-drop` key of the place under the pointer that takes it. */
	over = $state<string | null>(null);

	readonly #ends: Ends;
	/** Where the press began, and what it was on. */
	#pressed: { x: number; y: number; moving: Moving } | null = null;
	/** Whether the press lifted into a drag, whether or not Escape has let go
	 * of it since: the click its release ends in is the drag's either way. */
	#lifted = false;
	/** Where it may land, from the moment it lifts. */
	#targets: ReadonlySet<string> | null = null;
	/** The folder a drop now would go into. */
	#into: string | null = null;
	/** The folded folder the pointer is resting on, and its clock. */
	#resting: string | null = null;
	#lingering: ReturnType<typeof setTimeout> | null = null;

	constructor(ends: Ends) {
		this.#ends = ends;
	}

	/**
	 * A press on something that can be dragged.
	 *
	 * Nothing is prevented: a press that never travels is a press, and the
	 * row's own click opens what it pressed.
	 */
	press(event: PointerEvent, moving: Moving): void {
		if (!primary(event)) return;
		this.#end();
		this.#pressed = { x: event.clientX, y: event.clientY, moving };
		window.addEventListener('pointermove', this.#move);
		window.addEventListener('pointerup', this.#up);
		window.addEventListener('pointercancel', this.#cancelled);
		window.addEventListener('blur', this.#cancelled);
		// In the capture phase, so that an Escape that lets go of a drag is
		// the drag's and not the window's, which would close the pane.
		window.addEventListener('keydown', this.#key, { capture: true });
	}

	/** Lets go of a drag without moving anything: a lock, the screen going. */
	cancel(): void {
		this.#end();
	}

	readonly #move = (event: PointerEvent) => {
		const pressed = this.#pressed;
		if (pressed === null) return;
		if (this.moving === null) {
			if (Math.hypot(event.clientX - pressed.x, event.clientY - pressed.y) < TRAVEL) return;
			this.#targets = this.#ends.targets(pressed.moving);
			this.moving = pressed.moving;
			this.#lifted = true;
		}

		const spot = document.elementFromPoint(event.clientX, event.clientY);
		const place = spot?.closest<HTMLElement>('[data-drop]');
		const into = place?.dataset.into;
		if (place && into !== undefined && this.#targets?.has(into)) {
			this.over = place.dataset.drop ?? null;
			this.#into = into;
		} else {
			this.over = null;
			this.#into = null;
		}

		// Whether or not the folder takes the drag: a reader may need to open
		// the folder a thing is in to reach one under it.
		this.#rest(spot?.closest<HTMLElement>('[data-opens]')?.dataset.opens ?? null);
	};

	/** Starts the clock on a folded folder the pointer came to rest on, and
	 * stops it when the pointer leaves it. */
	#rest(folded: string | null) {
		if (folded === this.#resting) return;
		this.#still();
		this.#resting = folded;
		if (folded === null) return;
		this.#lingering = setTimeout(() => {
			this.#lingering = null;
			this.#ends.open(folded);
		}, LINGER);
	}

	#still() {
		if (this.#lingering !== null) clearTimeout(this.#lingering);
		this.#lingering = null;
		this.#resting = null;
	}

	readonly #up = () => {
		const moving = this.moving;
		const into = this.#into;
		if (this.#lifted) {
			// The press and the release were on different elements, so the click
			// that follows lands on whatever holds both - in the folders pane, the
			// empty part that puts the pane away. It is the drag's, and goes
			// nowhere. Let go on the next task if none came.
			window.addEventListener('click', swallow, { capture: true, once: true });
			setTimeout(() => window.removeEventListener('click', swallow, { capture: true }));
		}
		this.#end();
		if (moving !== null && into !== null) this.#ends.drop(moving, into);
	};

	readonly #cancelled = () => {
		this.#end();
	};

	/**
	 * Escape lets go of the drag in the air. The button is still held, though,
	 * and its release ends in a click on whatever holds both ends of the drag,
	 * like any other: so the release is still listened for, and that click is
	 * swallowed with it.
	 */
	readonly #key = (event: KeyboardEvent) => {
		if (this.moving === null || event.key !== 'Escape') return;
		event.preventDefault();
		event.stopPropagation();
		this.#letGo();
	};

	/** Lets go of what is being dragged: nothing lifted, nothing lit, no folder
	 * about to open, and nothing the pointer does lifts it again. */
	#letGo() {
		this.#still();
		this.#pressed = null;
		this.#targets = null;
		this.#into = null;
		this.moving = null;
		this.over = null;
	}

	/** Lets go of the drag and of the press, and stops listening. */
	#end() {
		window.removeEventListener('pointermove', this.#move);
		window.removeEventListener('pointerup', this.#up);
		window.removeEventListener('pointercancel', this.#cancelled);
		window.removeEventListener('blur', this.#cancelled);
		window.removeEventListener('keydown', this.#key, { capture: true });
		this.#letGo();
		this.#lifted = false;
	}
}
