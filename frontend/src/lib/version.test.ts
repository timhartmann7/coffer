/**
 * One version, in one place.
 *
 * The bundler builds the release asset names out of `tauri.conf.json`, the
 * workspace builds the binary out of `Cargo.toml`, the window crate names its
 * sibling by version because `cargo deny` bans a wildcard path, and the settings
 * screen says a number out loud. Four copies of one fact.
 *
 * The release workflow refuses a tag that disagrees with the first two. This is
 * the check that runs before there is a tag, and it is the only one that knows
 * about the other two: a missed path version fails as Cargo refusing to resolve,
 * on a message that names neither the tag nor the line, and a missed screen
 * shows a stranger a version the application is not.
 */

import { readFileSync } from 'node:fs';
import { expect, it } from 'vitest';

const configuration = JSON.parse(readFileSync('../crates/vault-gui/tauri.conf.json', 'utf8'));
const manifest = readFileSync('../Cargo.toml', 'utf8');
const window = readFileSync('../crates/vault-gui/Cargo.toml', 'utf8');
const settings = readFileSync('src/lib/components/Settings.svelte', 'utf8');

it('is the same in the workspace and in the bundle', () => {
	const workspace = manifest
		.slice(manifest.indexOf('[workspace.package]'))
		.match(/^version = "([^"]+)"$/m);

	expect(workspace, 'Cargo.toml no longer states a workspace version').not.toBeNull();
	expect(configuration.version).toBe(workspace?.[1]);
	expect(configuration.version).toMatch(/^\d+\.\d+\.\d+$/);
});

/** A caret range on a sibling that has moved past it stops the workspace
 * resolving at all, and the error names neither the tag nor this line. */
it('is the same on the path the window crate names its engine by', () => {
	const named = window.match(/vault-core = \{ path = "[^"]+", version = "([^"]+)" \}/);

	expect(named, 'vault-gui no longer names vault-core by version').not.toBeNull();
	expect(named?.[1]).toBe(configuration.version);
});

/** A number typed into the markup is the copy nobody remembers to bump. */
it('reaches the settings screen from the bundle rather than by hand', () => {
	expect(settings).toContain('__COFFER_VERSION__');
	expect(settings).not.toMatch(/coffer \d+\.\d+\.\d+/);
});
