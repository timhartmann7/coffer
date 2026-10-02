import { flush } from './drafts';
import { closeWindow, lock } from './ipc';

/**
 * Locks the vault the reader asked to lock, and clears the screen with
 * `clear` once Rust has answered.
 *
 * Locking destroys the window, so nothing after the lock is guaranteed to run.
 * What the reader was typing is handed to Rust before the lock is asked for,
 * and every write already on its way is waited for, so that the lock finds all
 * of it. The screen is cleared after the lock rather than before: clearing it
 * tears down the fields the typing is in, and a field that goes with typing in
 * it tells Rust to let go of it - which, sent ahead of the lock, would be the
 * lock losing it. A window that outlives its own lock, or a lock that failed,
 * still stops drawing a tree that is no longer to be trusted in memory.
 */
export async function lockByHand(clear: () => void): Promise<void> {
	await flush();
	try {
		await lock();
	} finally {
		clear();
	}
}

/**
 * Closes the window the reader asked to close. Rust locks the vault and takes
 * the window down without building it again. What they were typing goes first,
 * the way `lockByHand` sends it; nothing is cleared, because nothing of this
 * window is left to draw.
 */
export async function closeByHand(): Promise<void> {
	await flush();
	await closeWindow();
}
