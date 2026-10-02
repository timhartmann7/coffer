/**
 * What both screens that take a new master password ask of it before it goes
 * to Rust: the creation screen, and the change in the settings. One place, so
 * that the two say the same thing in the same words.
 */

/**
 * Whether two passwords are the same, compared as bytes rather than as strings.
 * Two different passwords can look the same after a lossy conversion, and a
 * vault opened by neither of them is not something anybody would find out
 * until later.
 */
export function same(one: Uint8Array, other: Uint8Array): boolean {
	return one.length === other.length && one.every((byte, at) => byte === other[at]);
}

/** Said when the password typed again is not the one typed first. */
export const MISMATCH = 'Those two are not the same.';

/** Said when there is no password at all: a file anybody can open is not a
 * vault. */
export const EMPTY = 'A vault needs a master password.';
