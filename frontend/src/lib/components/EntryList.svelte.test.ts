import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { row } from '$lib/fixtures';
import type { EntryRow } from '$lib/model';
import EntryList from './EntryList.svelte';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => {
	host.remove();
});

function draw(
	rows: EntryRow[],
	handlers: {
		onOpen?: (id: string) => void;
		onCopy?: (row: EntryRow, field: 'UserName' | 'Password') => void;
	}
) {
	return mount(EntryList, {
		target: host,
		props: {
			rows,
			now: new Date('2026-08-29T14:30:00Z'),
			onOpen: handlers.onOpen ?? vi.fn(),
			onCopy: handlers.onCopy ?? vi.fn()
		}
	});
}

/** A title the database protects has no value to draw, and an empty cell would
 * read as an entry without a name. */
it('draws the mask where a value is protected', () => {
	const component = draw([row({ title: null, username: null })], {});
	flushSync();

	expect(host.querySelectorAll('use[href="#redact"]')).toHaveLength(2);

	return unmount(component);
});

it('says how many tags it did not draw rather than cutting one in half', () => {
	const component = draw([row({ tags: ['prod', 'ssh', 'database', 'eu'] })], {});
	flushSync();

	const tags = [...host.querySelectorAll('em')].map((each) => each.textContent?.trim());
	expect(tags).toEqual(['prod', 'ssh', '+2']);

	return unmount(component);
});

it('offers a copy only for a field the entry has', () => {
	const component = draw(
		[row({ username: '', hasPassword: false }), row({ username: 'deploy', hasPassword: true })],
		{}
	);
	flushSync();

	const copies = [...host.querySelectorAll('button')].filter((each) =>
		each.getAttribute('aria-label')?.startsWith('Copy')
	);
	expect(copies.map((each) => each.disabled)).toEqual([true, true, false, false]);

	return unmount(component);
});

it('copies the field the button names, without the value passing through', () => {
	const onCopy = vi.fn();
	const only = row({ username: 'deploy', hasPassword: true });
	const component = draw([only], { onCopy });
	flushSync();

	const copies = [...host.querySelectorAll('button')].filter((each) =>
		each.getAttribute('aria-label')?.startsWith('Copy')
	);
	copies[0].click();
	copies[1].click();
	flushSync();

	expect(onCopy.mock.calls).toEqual([
		[only, 'UserName'],
		[only, 'Password']
	]);

	return unmount(component);
});

/** An entry with no title is still an entry, and the row still has to be
 * something a reader can hit. */
it('opens an entry that has no title at all', () => {
	const onOpen = vi.fn();
	const untitled = row({ title: '' });
	const component = draw([untitled], { onOpen });
	flushSync();

	const opener = host.querySelector('button');
	if (!opener) throw new Error('the row has nothing to click');
	expect(opener.getBoundingClientRect).toBeDefined();
	opener.click();
	flushSync();

	expect(onOpen).toHaveBeenCalledWith(untitled.id);

	return unmount(component);
});

it('writes a value that is markup as text', () => {
	const component = draw([row({ title: '<script>alert(1)</script>', username: '<b>x</b>' })], {});
	flushSync();

	expect(host.querySelector('script')).toBeNull();
	expect(host.querySelector('b')).toBeNull();
	expect(host.textContent).toContain('<script>alert(1)</script>');

	return unmount(component);
});
