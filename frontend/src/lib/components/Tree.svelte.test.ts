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
			onToggle: vi.fn(),
			onMenu: vi.fn()
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

/** Only the folder a drop would go into lights, in the accent, and the folder
 * being dragged keeps the plane it was lifted on. */
it('marks the folder a drag would land in with the accent, and only that one', () => {
	const servers = group({ name: 'Servers' });
	const work = group({ name: 'Work', sections: [servers] });
	const home = group({ name: 'Home' });
	const root = group({ sections: [work, home] });

	const component = mount(Tree, {
		target: host,
		props: {
			root,
			selected: null,
			expanded: new SvelteSet<string>(),
			drop: home.id,
			lifted: work.id,
			onSelect: vi.fn(),
			onToggle: vi.fn(),
			onPress: vi.fn(),
			onMenu: vi.fn()
		}
	});
	flushSync();

	const line = (id: string) => host.querySelector<HTMLElement>(`[data-drop="${id}"]`);
	expect(line(home.id)?.className).toContain('outline-accent');
	expect(line(home.id)?.querySelector('svg')?.getAttribute('class')).toContain('text-accent');
	expect(line(work.id)?.className).not.toContain('outline-accent');
	expect(line(work.id)?.className).toContain('bg-raised');

	// Where a drop goes, and which folded folder a rest opens, by id only.
	expect(line(home.id)?.dataset.into).toBe(home.id);
	expect(line(work.id)?.dataset.opens).toBe(work.id);
	expect(line(home.id)?.dataset.opens, 'a folder with nothing to open').toBeUndefined();

	return unmount(component);
});

/** The press a drag starts from is on the folder's own button, not on the
 * chevron that folds it. */
it('hands a press on a folder to the screen, and none on its chevron', () => {
	const servers = group({ name: 'Servers' });
	const work = group({ name: 'Work', sections: [servers] });
	const onPress = vi.fn();

	const component = mount(Tree, {
		target: host,
		props: {
			root: group({ sections: [work] }),
			selected: null,
			expanded: new SvelteSet<string>(),
			onSelect: vi.fn(),
			onToggle: vi.fn(),
			onPress,
			onMenu: vi.fn()
		}
	});
	flushSync();

	host
		.querySelector('[aria-label="Expand Work"]')
		?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
	expect(onPress).not.toHaveBeenCalled();
	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.includes('Work'))
		?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
	expect(onPress).toHaveBeenCalledWith(expect.any(PointerEvent), work);

	return unmount(component);
});

/** A right-click anywhere on a folder's line - its name, its count, the
 * chevron that folds it - asks for that folder's menu, and only one menu. */
it('asks for a folder’s menu from anywhere on its line', () => {
	const servers = group({ name: 'Servers' });
	const work = group({ name: 'Work', sections: [servers] });
	const onMenu = vi.fn();

	const component = mount(Tree, {
		target: host,
		props: {
			root: group({ sections: [work] }),
			selected: null,
			expanded: new SvelteSet<string>([work.id]),
			onSelect: vi.fn(),
			onToggle: vi.fn(),
			onMenu
		}
	});
	flushSync();

	const chevron = host.querySelector('[aria-label="Collapse Work"]');
	const name = [...host.querySelectorAll('button')].find((each) =>
		each.textContent?.includes('Servers')
	);
	for (const target of [chevron, name]) {
		target?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
	}

	expect(onMenu.mock.calls.map(([, folder]) => folder.id)).toEqual([work.id, servers.id]);
	return unmount(component);
});
