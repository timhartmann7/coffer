/**
 * Presses that are on their way to Rust, by what they are about.
 *
 * A button stays on the screen until Rust answers, and a save can hold Rust
 * for a second. A second press in that time asks again for something already
 * asked for - and finds it already done, or done to something that has moved
 * since. So a press is dropped while anything it is about is on its way, and
 * only then: a press about something else is a choice of its own.
 */
export class Once {
	readonly #busy = new Set<string>();

	/** Whether something about `key` is on its way. */
	busy(key: string): boolean {
		return this.#busy.has(key);
	}

	/**
	 * Sends `work`, unless something about any of `keys` is on its way: `null`
	 * is a press that was dropped. Every key is let go when the answer comes,
	 * whatever it is.
	 */
	async run<T>(keys: readonly string[], work: () => Promise<T>): Promise<T | null> {
		if (keys.some((key) => this.#busy.has(key))) return null;
		const taken = [...new Set(keys)];
		for (const key of taken) this.#busy.add(key);
		try {
			return await work();
		} finally {
			for (const key of taken) this.#busy.delete(key);
		}
	}
}
