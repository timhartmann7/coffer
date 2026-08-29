import { flushSync, mount, unmount } from 'svelte';
import { SvelteSet } from 'svelte/reactivity';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { group, row } from '$lib/fixtures';
import type { Group } from '$lib/model';
import Tree from './Tree.svelte';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => {
	host.remove();
});

function ids(root: Group): string[] {
	return [root.id, ...root.sections.flatMap(ids)];
}

function draw(root: Group, open: string[] = [], selected: string | null = null) {
	return mount(Tree, {
		target: host,
		props: {
			root,
			selected,
			expanded: new SvelteSet(open),
			onSelect: vi.fn(),
			onToggle: vi.fn()
		}
	});
}

it('counts the entries of a group and of everything under it', () => {
	const root = group({
		sections: [
			group({
				name: 'Work',
				entries: [row(), row()],
				sections: [group({ name: 'Servers', entries: [row()] })]
			})
		]
	});

	const component = draw(root);
	flushSync();
	expect(host.textContent).toContain('3');

	return unmount(component);
});

it('shows a section only when its group is open', () => {
	const inside = group({ name: 'Servers' });
	const work = group({ name: 'Work', sections: [inside] });
	const root = group({ sections: [work] });

	const shut = draw(root);
	flushSync();
	expect(host.textContent).not.toContain('Servers');
	unmount(shut);

	const open = draw(root, [work.id]);
	flushSync();
	expect(host.textContent).toContain('Servers');

	return unmount(open);
});

/** The attack list asks for a hundred levels. The pane is 228 points wide, so
 * the indentation has to give way before the name does. */
it('draws a group a hundred levels deep, and its name with it', () => {
	let deepest = group({ name: 'level 100' });
	for (let level = 99; level > 0; level -= 1) {
		deepest = group({ name: `level ${level}`, sections: [deepest] });
	}
	const root = group({ sections: [deepest] });

	const component = draw(root, ids(root));
	flushSync();

	expect(host.textContent).toContain('level 100');
	expect(host.querySelectorAll('[aria-hidden="true"]').length).toBeGreaterThan(100);

	return unmount(component);
});

it('writes a group name that is markup as text', () => {
	const component = draw(group({ sections: [group({ name: '<img src=x onerror=alert(1)>' })] }));
	flushSync();

	expect(host.querySelector('img')).toBeNull();
	expect(host.textContent).toContain('<img src=x onerror=alert(1)>');

	return unmount(component);
});

it('marks the group being shown with the bar, and only that one', () => {
	const inside = group({ name: 'Servers' });
	const work = group({ name: 'Work', sections: [inside] });
	const root = group({ sections: [work] });

	const component = draw(root, [work.id], inside.id);
	flushSync();

	const bars = [...host.querySelectorAll('span')].filter((each) =>
		each.className.includes('bg-accent')
	);
	expect(bars).toHaveLength(1);
	expect(bars[0].parentElement?.textContent).toContain('Servers');

	return unmount(component);
});
