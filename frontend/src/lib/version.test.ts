/**
 * One version, in one place.
 *
 * The bundler builds the release asset names out of `tauri.conf.json`, the
 * workspace builds the binary out of `Cargo.toml`, and the settings screen says
 * a number out loud. Three copies of one fact, and the way the third one
 * announces itself is a stranger reading a version the application is not.
 *
 * The release workflow refuses a tag that disagrees with the first two. This is
 * the check that runs before there is a tag.
 */

import { readFileSync } from 'node:fs';
import { expect, it } from 'vitest';

const configuration = JSON.parse(readFileSync('../crates/vault-gui/tauri.conf.json', 'utf8'));
const manifest = readFileSync('../Cargo.toml', 'utf8');
const settings = readFileSync('src/lib/components/Settings.svelte', 'utf8');

it('is the same in the workspace and in the bundle', () => {
	const workspace = manifest
		.slice(manifest.indexOf('[workspace.package]'))
		.match(/^version = "([^"]+)"$/m);

	expect(workspace, 'Cargo.toml no longer states a workspace version').not.toBeNull();
	expect(configuration.version).toBe(workspace?.[1]);
	expect(configuration.version).toMatch(/^\d+\.\d+\.\d+$/);
});

/** A number typed into the markup is the copy nobody remembers to bump. */
it('reaches the settings screen from the bundle rather than by hand', () => {
	expect(settings).toContain('__COFFER_VERSION__');
	expect(settings).not.toMatch(/coffer \d+\.\d+\.\d+/);
});
