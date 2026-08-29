/**
 * The two halves of the boundary, checked against each other.
 *
 * `ipc.ts` names a command and its arguments as strings, and Rust names the
 * same things in a function signature and in a list of handlers. Nothing but
 * this connects them: a renamed argument, a command left out of the handler
 * list, or a handler nothing ever calls are all silent until somebody presses
 * the button.
 */

import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const commands = readFileSync('../crates/vault-gui/src/commands.rs', 'utf8');
const registered = readFileSync('../crates/vault-gui/src/lib.rs', 'utf8');
const ipc = readFileSync('src/lib/ipc.ts', 'utf8');

/** Arguments Tauri fills in itself. They are never sent by the window. */
const PROVIDED = ['session', 'app', 'request'];

/** Every command Rust declares, with the arguments it expects from the window. */
function declared(): Map<string, string[]> {
	const found = new Map<string, string[]>();
	const pattern = /#\[tauri::command(?:\(async\))?\]\s*pub (?:async )?fn (\w+)\(([^)]*)\)/g;

	for (const [, name, parameters] of commands.matchAll(pattern)) {
		const taken = parameters
			.split(',')
			.map((one) => one.trim())
			.filter((one) => one !== '')
			.map((one) => one.split(':')[0].trim())
			.filter((one) => !PROVIDED.includes(one));
		found.set(name, taken);
	}

	return found;
}

/** Every command the window calls, with the arguments it sends. */
function called(): Map<string, string[]> {
	const found = new Map<string, string[]>();
	const pattern = /invoke\('(\w+)'(?:,\s*(\{[^}]*\}|\w+))?\)/g;

	for (const [, name, argument] of ipc.matchAll(pattern)) {
		if (argument === undefined || !argument.startsWith('{')) {
			// `invoke('unlock', password)` sends the bytes as the whole body, which
			// is the one shape that carries no named arguments at all.
			found.set(name, []);
			continue;
		}
		const sent = argument
			.slice(1, -1)
			.split(',')
			.map((one) => one.trim())
			.filter((one) => one !== '')
			.map((one) => one.split(':')[0].trim());
		found.set(name, sent);
	}

	return found;
}

describe('the window and the vault agree on what a command is called', () => {
	it('calls only commands Rust declares', () => {
		const rust = declared();
		expect(rust.size).toBeGreaterThan(20);

		for (const name of called().keys()) {
			expect(rust.has(name), `the window calls ${name}, which Rust does not declare`).toBe(true);
			expect(
				registered.includes(`commands::${name},`),
				`${name} is not in the handler list, so calling it fails at runtime`
			).toBe(true);
		}
	});

	it('sends every argument a command takes and no others', () => {
		const rust = declared();

		for (const [name, sent] of called()) {
			const taken = rust.get(name) ?? [];
			expect([...sent].sort(), `the window sends the wrong arguments to ${name}`).toEqual(
				[...taken].sort()
			);
		}
	});

	/** A command nothing calls is a way into the vault that exists for nobody,
	 * which is what the dead code rule is about. */
	it('declares no command the window never calls', () => {
		const window = called();

		for (const name of declared().keys()) {
			expect(window.has(name), `${name} is a command nothing in the window calls`).toBe(true);
		}
	});
});
