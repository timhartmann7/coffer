/**
 * What keeps the entry pane where it is.
 *
 * A new value typed into a Change field is the reader's and is nowhere else
 * until Save: not in the vault, and not in anything the window could draw
 * again. The pane going - another row pressed, Escape, Close, a new entry, a
 * folder chosen, the entry moved to the bin, the settings drawn over it - used
 * to take the field with it, and a password just set on a website went with a
 * notice that it had not been saved.
 *
 * So every Change field that holds something typed, or is saving it, says so
 * here, and the window asks here before the pane goes or moves. The pane stays,
 * and the field puts its own question - with the focus on it - so the reader
 * answers it rather than losing it.
 */

/** A Change field, as the window sees it. */
interface Holding {
	/** Whether it holds something typed, or is saving it. */
	holds: () => boolean;
	/** Puts the field's question, taking the focus when `focus` says so, and
	 * answers whether there was a question to put: a save on its way has
	 * none, and is only waited for. */
	ask: (focus: boolean) => boolean;
}

const fields = new Set<Holding>();

/** A Change field opened. What it answers with lets go of it again. */
export function hold(field: Holding): () => void {
	fields.add(field);
	return () => fields.delete(field);
}

/**
 * Whether the pane has to stay where it is.
 *
 * Every field holding something is asked to say so, and the first one with a
 * question takes the focus: one answer at a time, where the reader is looking.
 */
export function held(): boolean {
	let holding = false;
	let asked = false;
	for (const field of fields) {
		if (!field.holds()) continue;
		holding = true;
		if (field.ask(!asked)) asked = true;
	}
	return holding;
}
