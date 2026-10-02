import { flushSync, mount, unmount, type ComponentProps } from 'svelte';
import { afterEach, beforeEach, expect, it } from 'vitest';
import { group, row } from '$lib/fixtures';
import Opening from './Opening.svelte';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => {
	host.remove();
});

const personal = group({ name: 'evil‮' });
const work = group({ name: 'Work' });
const root = group({ name: 'Root', sections: [personal, work] });

function draw(over: Partial<ComponentProps<typeof Opening>>) {
	const component = mount(Opening, {
		target: host,
		props: {
			row: row({ group: work.id }),
			root,
			path: [personal, work],
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onClose: () => {},
			...over
		}
	});
	flushSync();
	return component;
}

/** The line above the title, the first thing in the heading. */
const line = () => host.querySelector('header > :first-child');

/**
 * While the entry is read, the line above its title says where it is the way
 * it will once read, and there is nothing on it to press: a list opened over
 * an entry still arriving would move what the reader had not seen. Where the
 * read entry will keep the plain line - in a vault Coffer will not write back,
 * and in the bin - the entry being read draws that line, so nothing changes
 * when it arrives.
 */
it('draws the line above the title as the entry will, with nothing on it to press', () => {
	const waiting = draw({});
	expect(line()?.getAttribute('aria-disabled')).toBe('true');
	expect(line()?.textContent).toContain('Work');
	expect(host.querySelector('header button[aria-haspopup]')).toBeNull();
	unmount(waiting);

	const binned = row({ group: work.id, binned: { since: null, within: null, from: work.id } });
	for (const over of [{ readOnly: true }, { row: binned }]) {
		const plain = draw(over);
		const what = JSON.stringify(over);
		expect(line()?.getAttribute('aria-disabled'), what).toBeNull();
		expect(line()?.textContent, what).toContain('Work');
		expect(host.querySelector('header button[aria-haspopup]'), what).toBeNull();
		// Each name isolated on its own, the plain line as much as the other.
		expect(
			[...(line()?.querySelectorAll('bdi') ?? [])].map((each) => each.textContent),
			what
		).toEqual(['evil‮', 'Work']);
		unmount(plain);
	}
});
