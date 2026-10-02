import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { group } from '$lib/fixtures';
import type { Group } from '$lib/model';
import FolderPicker from './FolderPicker.svelte';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => {
	host.remove();
	vi.restoreAllMocks();
});

const cafe = group({ name: 'Café' });
const personal = group({ name: 'Personal', sections: [cafe] });
const work = group({ name: 'Work' });
const binned = group({ name: 'Old work', binned: { since: null, within: null, from: null } });
const bin = group({ name: 'Recycle Bin', isRecycleBin: true, sections: [binned] });
const vault = group({ name: 'Passwords', sections: [personal, work, bin] });

function draw(over: { root?: Group; current?: string | null; chosen?: string } = {}): ReturnType<
	typeof mount
> & {
	onPick: ReturnType<typeof vi.fn>;
	onClose: ReturnType<typeof vi.fn>;
} {
	const onPick = vi.fn();
	const onClose = vi.fn();
	const root = over.root ?? vault;
	const component = mount(FolderPicker, {
		target: host,
		props: {
			root,
			heading: 'Move to',
			label: 'Find the folder to move it to',
			current: over.current === undefined ? work.id : over.current,
			chosen: over.chosen ?? over.current ?? root.id,
			class: 'left-0',
			onPick,
			onClose
		}
	});
	flushSync();
	return Object.assign(component, { onPick, onClose });
}

function filter(): HTMLInputElement {
	const found = host.querySelector<HTMLInputElement>('input[role="combobox"]');
	if (!found) throw new Error('there is no filter');
	return found;
}

function press(key: string, over: KeyboardEventInit = {}): KeyboardEvent {
	const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...over });
	filter().dispatchEvent(event);
	flushSync();
	return event;
}

function type(text: string) {
	filter().value = text;
	filter().dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
}

/** The lines the list draws, by name. */
function lines(): string[] {
	return [...host.querySelectorAll('[role="option"]')].map(
		(option) => option.querySelector('bdi')?.textContent ?? ''
	);
}

/** The line the keys are on, which the filter names for a screen reader. */
function active(): string | null {
	const id = filter().getAttribute('aria-activedescendant');
	return id === null
		? null
		: (document.getElementById(id)?.querySelector('bdi')?.textContent ?? null);
}

it('starts on the folder it was given, so Return gives it', () => {
	const picker = draw({ current: null, chosen: work.id });

	expect(document.activeElement, 'the keys are not in the filter').toBe(filter());
	expect(active()).toBe('Work');
	press('Enter');

	expect(picker.onPick).toHaveBeenCalledWith(work.id);
	expect(picker.onClose).not.toHaveBeenCalled();
	return unmount(picker);
});

/** A remembered folder that has gone, or gone to the bin, since is not a line
 * of the list, and the keys start on the first line there is. */
it('starts on the top of the vault when the folder it was given is no longer listed', () => {
	const picker = draw({ current: null, chosen: binned.id });

	expect(active()).toBe('Top of the vault');
	press('Enter');
	expect(picker.onPick).toHaveBeenCalledWith(vault.id);
	return unmount(picker);
});

/**
 * The list shows five lines, and the line it opens on - the folder the last
 * entry went into, or where the entry is now - can be far below them. Return
 * takes it with no key before it, so it is scrolled into sight as the list
 * opens, and again wherever the keys take it.
 */
it('opens with the line the keys are on in sight, and keeps it there', () => {
	const many = Array.from({ length: 12 }, (_, at) => group({ name: `Folder ${at}` }));
	const scrolled = vi.spyOn(Element.prototype, 'scrollIntoView');
	const picker = draw({ root: group({ sections: many }), current: null, chosen: many[10].id });

	const shown = () => document.getElementById(filter().getAttribute('aria-activedescendant') ?? '');
	expect(active()).toBe('Folder 10');
	expect(scrolled.mock.contexts, 'the list opened on a line out of sight').toContain(shown());
	expect(scrolled).toHaveBeenLastCalledWith({ block: 'nearest' });

	press('ArrowDown');
	expect(active()).toBe('Folder 11');
	expect(scrolled.mock.contexts.at(-1)).toBe(shown());

	type('folder 3');
	expect(active()).toBe('Folder 3');
	expect(scrolled.mock.contexts.at(-1)).toBe(shown());
	return unmount(picker);
});

it('lists the top of the vault first, and nothing in the recycle bin', () => {
	const picker = draw();

	expect(lines()).toEqual(['Top of the vault', 'Personal', 'Café', 'Work']);
	expect(host.textContent).not.toContain('Old work');
	expect(host.textContent).not.toContain('Recycle Bin');
	return unmount(picker);
});

it('moves through the lines with the arrows, round from the last to the first', () => {
	const picker = draw({ current: null, chosen: vault.id });

	press('ArrowUp');
	expect(active()).toBe('Work');
	press('ArrowDown');
	expect(active()).toBe('Top of the vault');
	press('ArrowDown');
	press('ArrowDown');
	expect(active()).toBe('Café');
	press('Enter');

	expect(picker.onPick).toHaveBeenCalledWith(cafe.id);
	return unmount(picker);
});

it('narrows by name and by the folders above, folding case and composition', () => {
	const picker = draw({ current: null });

	// Written with a combining accent, against a name stored composed.
	type('CAFÉ');
	expect(lines()).toEqual(['Café']);

	// A folder is found by the folders it sits in as well.
	type('personal');
	expect(lines()).toEqual(['Personal', 'Café']);
	expect(active(), 'the first line that matches').toBe('Personal');

	// The line the keys are on stays while it still matches.
	press('ArrowDown');
	type('persona');
	expect(active()).toBe('Café');
	press('Enter');
	expect(picker.onPick).toHaveBeenCalledWith(cafe.id);
	return unmount(picker);
});

it('marks where the thing is now and sends nothing when that is chosen', () => {
	const picker = draw({ current: work.id });

	const here = host.querySelector('[aria-current="location"]');
	expect(here?.querySelector('bdi')?.textContent).toBe('Work');
	expect(here?.querySelector('svg')?.getAttribute('class')).toContain('text-accent');
	expect(active()).toBe('Work');

	press('Enter');
	expect(picker.onPick).not.toHaveBeenCalled();
	expect(picker.onClose).toHaveBeenCalledWith(true);

	here?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
	expect(picker.onPick).not.toHaveBeenCalled();
	return unmount(picker);
});

it('takes a line pressed with the pointer, and keeps the focus in the filter', () => {
	const picker = draw({ current: null });
	const line = [...host.querySelectorAll<HTMLButtonElement>('[role="option"]')].at(1);

	const down = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
	line?.dispatchEvent(down);
	expect(down.defaultPrevented, 'the press took the focus out of the filter').toBe(true);
	line?.click();

	expect(picker.onPick).toHaveBeenCalledWith(personal.id);
	return unmount(picker);
});

it('Escape closes it without reaching the window', () => {
	const heard = vi.fn();
	window.addEventListener('keydown', heard);
	const picker = draw();

	const escape = press('Escape');

	expect(picker.onClose).toHaveBeenCalledWith(true);
	expect(escape.defaultPrevented).toBe(true);
	expect(heard, 'the window cleared the search or closed the pane as well').not.toHaveBeenCalled();
	window.removeEventListener('keydown', heard);
	return unmount(picker);
});

/** The Return that confirms a conversion belongs to the input method, and in
 * WebKit it arrives with `isComposing` already false. */
it('a Return or an Escape that ends an input method’s conversion picks and closes nothing', () => {
	const picker = draw({ current: null, chosen: work.id });

	press('Enter', { keyCode: 229 });
	press('Enter', { isComposing: true });
	press('Escape', { keyCode: 229 });

	expect(picker.onPick).not.toHaveBeenCalled();
	expect(picker.onClose).not.toHaveBeenCalled();
	return unmount(picker);
});

it('draws a folder called <img src=x onerror=alert(1)> as text', () => {
	const hostile = group({ name: '<img src=x onerror=alert(1)>' });
	const turned = group({ name: 'evil‮slip' });
	const picker = draw({ root: group({ sections: [hostile, turned] }), current: null });

	expect(host.querySelector('img')).toBeNull();
	expect(lines()).toContain('<img src=x onerror=alert(1)>');
	const reversed = [...host.querySelectorAll('bdi')].find((each) =>
		each.textContent?.includes('‮')
	);
	expect(reversed, 'the override is outside an isolate').toBeDefined();
	return unmount(picker);
});

/** Each folder above a line is a name from the vault, isolated on its own: an
 * override in one turns round nothing but that name, and a folder's path
 * cannot be made to read as another's. */
it('isolates every folder above a line on its own', () => {
	const deep = group({ name: 'Deep' });
	const inner = group({ name: 'Inner', sections: [deep] });
	const turned = group({ name: 'evil‮', sections: [inner] });
	const picker = draw({ root: group({ sections: [turned] }), current: null });

	const line = [...host.querySelectorAll('[role="option"]')].find(
		(option) => option.querySelector('bdi')?.textContent === 'Deep'
	);
	const above = [...(line?.querySelectorAll('bdi') ?? [])].slice(1).map((each) => each.textContent);
	expect(above).toEqual(['evil‮', 'Inner']);
	expect(line?.textContent).toContain('evil‮ · Inner');
	return unmount(picker);
});

it('says when no folder matches, naming what was typed in quotes', () => {
	const picker = draw();

	type('Banking‮');
	expect(lines()).toEqual([]);
	expect(host.textContent).toContain('No folder matches “⁨Banking‮⁩”');
	expect(filter().getAttribute('aria-activedescendant')).toBeNull();

	press('Enter');
	expect(picker.onPick).not.toHaveBeenCalled();
	expect(picker.onClose).not.toHaveBeenCalled();
	return unmount(picker);
});

it('a folder whose name is a megabyte is drawn and filtered without folding it on every key', () => {
	const huge = group({ name: 'x'.repeat(1_000_000) });
	const folding = vi.spyOn(String.prototype, 'normalize');
	const picker = draw({ root: group({ sections: [huge, work] }), current: null });

	for (const query of ['w', 'wo', 'wor', 'work', 'x']) type(query);

	const big = folding.mock.contexts.filter((text) => String(text).length >= 1_000_000);
	expect(big, 'the megabyte was folded again on a key').toHaveLength(1);
	expect(lines()).toHaveLength(1);
	return unmount(picker);
});

it('closes when the focus leaves it, and not when it moves inside it', () => {
	const opener = document.createElement('button');
	host.append(opener);
	const away = document.createElement('button');
	document.body.append(away);
	const picker = draw();

	// Back to what opened it, in one box with the list: that control's own
	// press is what closes the list then.
	filter().dispatchEvent(new FocusEvent('focusout', { bubbles: true, relatedTarget: opener }));
	const line = host.querySelector('[role="option"]');
	filter().dispatchEvent(new FocusEvent('focusout', { bubbles: true, relatedTarget: line }));
	expect(picker.onClose).not.toHaveBeenCalled();

	filter().dispatchEvent(new FocusEvent('focusout', { bubbles: true, relatedTarget: away }));
	expect(picker.onClose).toHaveBeenCalledWith(false);

	away.remove();
	return unmount(picker);
});
