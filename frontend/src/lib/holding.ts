/**
 * What keeps the entry pane, or the settings over it, where it is.
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
 *
 * A master password being changed holds too, and has nothing to ask: it is
 * only waited for. The settings stay until Rust answers, because the answer
 * is the one place that says which password now opens the vault.
 */

/** Something that must not be taken away while it holds, as the window sees
 * it: a Change field holding what was typed or saving it, or a master password
 * on its way to Rust. */
interface Holding {
	/** Whether it holds now. */
	holds: () => boolean;
	/** Puts its question, taking the focus when `focus` says so, and answers
	 * whether there was a question to put: a save or a new master password on
	 * its way has none, and is only waited for. */
	ask: (focus: boolean) => boolean;
}

const holders = new Set<Holding>();

/** Something that may hold came into being: a Change field opened, a master
 * password sent. What this answers with lets go of it again. */
export function hold(holder: Holding): () => void {
	holders.add(holder);
	return () => holders.delete(holder);
}

/**
 * Whether the pane, and the settings over it, have to stay where they are.
 *
 * Everything holding is asked to say so, and the first one with a question
 * takes the focus: one answer at a time, where the reader is looking.
 */
export function held(): boolean {
	let holding = false;
	let asked = false;
	for (const holder of holders) {
		if (!holder.holds()) continue;
		holding = true;
		if (holder.ask(!asked)) asked = true;
	}
	return holding;
}
