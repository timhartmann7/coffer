/**
 * Nothing a reader sees says the name of another product.
 *
 * The engine names the library it is built on and the tool the round-trip suite
 * compares against, because that is what those files are about. A screen, a file
 * panel or a failure is a different matter: one that names somebody else's
 * application is Coffer wearing another brand. The format has a name of its own,
 * and KDBX is the word for it.
 *
 * What is deliberately not scanned, and why: `README.md`, where the promise that
 * the file opens in the other clients of the format is required rather than
 * forbidden; the engine's comments and docs, which name another client to
 * explain why a constant has the value it has; `install.sh`, `Casks/coffer.rb`
 * and the workflows, which name the tools they run. This test is about strings a
 * reader sees inside the window.
 */

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const OTHERS = /keepass|keepassium|1password|bitwarden|lastpass|dashlane|enpass|strongbox/i;

function screens(): { path: string; text: string }[] {
	const found: { path: string; text: string }[] = [];

	function walk(directory: string) {
		for (const entry of readdirSync(directory)) {
			const path = join(directory, entry);
			if (statSync(path).isDirectory()) walk(path);
			else if (entry.endsWith('.svelte')) found.push({ path, text: readFileSync(path, 'utf8') });
		}
	}

	walk('src/lib/components');
	walk('src/routes');
	return found;
}

function captured(file: string, pattern: RegExp): string[] {
	return [...readFileSync(file, 'utf8').matchAll(pattern)].map((match) => match[1]);
}

describe('the only brand in the window is Coffer', () => {
	/** These become `Failure.message` verbatim and are rendered into the unlock
	 * screen and the toast. They are application strings, not comments. */
	it('says nothing about another product when something fails', () => {
		const messages = captured('../crates/vault-core/src/error.rs', /#\[error\("([^"]*)"\)\]/g);
		expect(messages.length).toBeGreaterThan(10);

		for (const message of messages) {
			expect(OTHERS.test(message), `error.rs says "${message}"`).toBe(false);
		}
	});

	it('gives the file panels words of its own', () => {
		const strings = captured(
			'../crates/vault-gui/src/commands.rs',
			/\.(?:set_title|add_filter)\("([^"]*)"/g
		);
		expect(strings.length).toBeGreaterThan(4);

		for (const string of strings) {
			expect(OTHERS.test(string), `commands.rs opens a panel called "${string}"`).toBe(false);
		}
	});

	/** No screen has a reason to name another client. The two that did said it in
	 * prose a reader reads, which is exactly the case this is here for. */
	it('names nobody else on any screen', () => {
		const found = screens();
		expect(found.length).toBeGreaterThan(10);

		for (const { path, text } of found) {
			expect(OTHERS.test(text), `${path} names another product`).toBe(false);
		}
	});
});

/**
 * The repository is written in one language.
 *
 * `CLAUDE.md` puts it under what gets a change rejected, and nothing else looks:
 * the workflows run fmt, clippy, shear, deny, knip and this suite, and none of
 * them reads a codepoint. A stray line of another alphabet in a comment or a doc
 * therefore ships in silence, which is how one reached `docs/design.md`.
 */
describe('the repository is written in English', () => {
	// Written as codepoints, because a range written as itself is a file that
	// fails its own rule.
	const CYRILLIC = /[\u0400-\u04ff]/;

	/** Directories that are not the repository: what a package manager fetched
	 * and what a compiler wrote. */
	const NOT_OURS = ['node_modules', 'target', '.git', 'build', '.svelte-kit'];

	/**
	 * The mockup, which is a Russian-language design document and the input this
	 * window is built from rather than anything a reader of the application ever
	 * sees. Translating it would change the source of truth for every screen, so
	 * it is named here instead of being quietly skipped.
	 */
	const MOCKUP = 'design.html';

	function everything(directory: string): string[] {
		const found: string[] = [];
		for (const entry of readdirSync(directory)) {
			if (NOT_OURS.includes(entry)) continue;
			const path = join(directory, entry);
			if (statSync(path).isDirectory()) found.push(...everything(path));
			else if (entry !== MOCKUP) found.push(path);
		}
		return found;
	}

	it('has no Cyrillic in any file it holds', () => {
		const files = everything('..');
		expect(files.length).toBeGreaterThan(50);

		for (const path of files) {
			const bytes = readFileSync(path);
			const text = bytes.toString('utf8');
			// A golden database and a key file are bytes, and bytes read as text
			// are whatever the cipher left behind - which is as likely to look
			// like one alphabet as another. What tells them from a file somebody
			// wrote is that they do not decode: a zero, or a run this decoder had
			// to replace.
			if (bytes.includes(0) || text.includes('\uFFFD')) continue;
			expect(CYRILLIC.test(text), `${path} is not written in English`).toBe(false);
		}
	});
});
