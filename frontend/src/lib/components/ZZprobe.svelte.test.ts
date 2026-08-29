import { flushSync, mount, unmount } from 'svelte';
import { beforeEach, expect, it, vi } from 'vitest';
import { entry, field, version } from '$lib/fixtures';
import Versions from './Versions.svelte';

const ipc = vi.hoisted(() => ({
	version: vi.fn(),
	revealVersion: vi.fn(),
	restoreVersion: vi.fn(),
	deleteVersion: vi.fn(),
	clearHistory: vi.fn()
}));
vi.mock('$lib/ipc', () => ipc);

let host: HTMLElement;
beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

it('probe: two protected fields in one version', async () => {
	ipc.revealVersion.mockResolvedValue('THE-PASSWORD');
	ipc.version.mockResolvedValue(
		entry({
			fields: [
				field({ name: 'Password', kind: 'password', value: null, empty: false }),
				field({ name: 'zz-token', kind: 'custom', value: null, empty: false })
			]
		})
	);

	const component = mount(Versions, {
		target: host,
		props: {
			entry: 'an-entry',
			versions: [version({ index: 0 })],
			now: new Date('2026-08-29T14:30:00Z'),
			onVersions: vi.fn(),
			onChanged: vi.fn(),
			onFailure: vi.fn()
		}
	});
	[...host.querySelectorAll('button')].find((b) => b.textContent?.includes('Versions'))?.click();
	flushSync();
	[...host.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'View')?.click();
	await vi.waitFor(() => expect(host.querySelector('[data-value]')).not.toBeNull());
	flushSync();

	host.querySelector<HTMLButtonElement>('[aria-label="Show Password as it was"]')?.click();
	await vi.waitFor(() => expect(host.textContent).toContain('THE-PASSWORD'));
	flushSync();

	const rows = [...host.querySelectorAll('[data-value]')];
	console.log('number of value nodes:', rows.length);
	rows.forEach((r, i) => console.log('node', i, JSON.stringify(r.textContent)));
	// which row label sits beside the filled node?
	const filled = rows.findIndex((r) => r.textContent === 'THE-PASSWORD');
	console.log('filled index', filled);
	console.log('masks visible:', host.querySelectorAll('use[href="#redact"]').length);
	await unmount(component);
});
