/**
 * What the window says about putting another file in the vault's place: the
 * copy a lock left, or a backup. One sentence for each thing that can come of
 * a press, shared by the two strips that make one.
 */

import { quoted } from './format';
import { asFailure } from './ipc';
import type { Adopted } from './model';

/**
 * Why a press that would have replaced the vault's file did not.
 *
 * Rust holds the press to how the strip last said the vault's file stood, and
 * refuses it with `externalChange` when the file stands otherwise now. By the
 * time this is shown the window has read the file again, so the sentence is
 * about the strip rather than Rust's: it points at what the strip says now.
 * Anything else is Rust's own sentence.
 */
export function refusal(thrown: unknown): string {
	const refused = asFailure(thrown);
	return refused.code === 'externalChange'
		? 'Your vault file changed after this was shown, so nothing was replaced. What is said above is how it stands now.'
		: refused.message;
}

/**
 * What became of the vault's file when a backup took its place: the newest
 * backup now, or - when it would not open with the backup's password - a file
 * of its own beside the vault. Either way it is named, so the reader can find
 * it. Nothing is said of a file that was not there. `password` is how the
 * sentence it ends names the backup's password.
 */
function replaced(made: Adopted, password: string): string[] {
	if (made.setAside !== null) {
		return [
			`The file it replaced did not open with ${password} and is kept as ${quoted(made.setAside)}.`
		];
	}
	if (made.keptAs !== null) return [`The file it replaced is kept as ${quoted(made.keptAs)}.`];
	return [];
}

/** The notice after a backup took the vault's place, said over the vault. */
export function adopted(made: Adopted): string {
	return ['This backup is your vault now.', ...replaced(made, 'this password')].join(' ');
}

/**
 * The same, on the unlock screen, when a lock took the window that pressed
 * before its notice could say it. It says which password the vault opens with
 * now as well: the backup's, which after a change is not the one the reader
 * last typed, and this screen is where they are about to type one.
 */
export function adoptedBeforeLocking(made: Adopted): string {
	return [
		'A backup was made your vault before locking, and it opens with the password that backup opened with.',
		...replaced(made, 'that password')
	].join(' ');
}
