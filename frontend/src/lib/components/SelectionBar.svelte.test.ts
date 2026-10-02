import { flushSync, mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { group, row } from '$lib/fixtures';
import type { EntryRow } from '$lib/model';
import SelectionBar from './SelectionBar.svelte';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => {
	host.remove();
});

const work = group({ name: 'Work' });
const banking = group({ name: 'Banking' });
const vault = group({ name: 'Passwords', sections: [work, banking] });

/** Three logins in Work, as the list draws them outside the bin. */
const logins = ['Gmail', 'Drive', 'Chase'].map((title) => row({ title, group: work.id }));

function draw(rows: EntryRow[] = logins, binned = false) {
	const ways = {
		onMove: vi.fn(),
		onTag: vi.fn(),
		onDelete: vi.fn(),
		onPutBack: vi.fn(),
		onClear: vi.fn()
	};
	const component = mount(SelectionBar, {
		target: host,
		props: { rows, root: vault, binned, ...ways }
	});
	flushSync();
	return { component, ...ways };
}

/** What the bar reads as, with the template's whitespace collapsed. */
function reads(): string {
	return (host.textContent ?? '').replace(/\s+/g, ' ').trim();
}

/** The labels of the bar's buttons, in order. */
function labels(): string[] {
	return [...host.querySelectorAll('button')].map(
		(each) => each.getAttribute('aria-label') ?? each.textContent?.trim() ?? ''
	);
}

function button(label: string): HTMLButtonElement {
	const found = [...host.querySelectorAll<HTMLButtonElement>('button')].find(
		(each) => (each.getAttribute('aria-label') ?? each.textContent?.trim()) === label
	);
	if (!found) throw new Error(`no button labelled ${label}`);
	return found;
}

function key(target: Element, name: string, over: KeyboardEventInit = {}): KeyboardEvent {
	const event = new KeyboardEvent('keydown', {
		key: name,
		bubbles: true,
		cancelable: true,
		...over
	});
	target.dispatchEvent(event);
	flushSync();
	return event;
}

function tagField(): HTMLInputElement {
	const found = host.querySelector<HTMLInputElement>('input[aria-label^="A tag"]');
	if (!found) throw new Error('the tag field is not open');
	return found;
}

/** Text typed into the tag field. */
function typeTag(text: string) {
	button('Add tag').click();
	flushSync();
	tagField().value = text;
	tagField().dispatchEvent(new Event('input', { bubbles: true }));
}

/** Whether a key went on to the window, where Escape lets go of the choice. */
function heard(press: () => void): boolean {
	const window_ = vi.fn();
	window.addEventListener('keydown', window_);
	try {
		press();
	} finally {
		window.removeEventListener('keydown', window_);
	}
	return window_.mock.calls.length > 0;
}

it('says how many are chosen and offers moving, tagging and deleting', () => {
	const { component } = draw();

	expect(reads()).toMatch(/^3 selected · Move to… · Add tag · Delete/);
	expect(labels()).toEqual(['Move to…', 'Add tag', 'Delete', 'Deselect all']);
	expect(button('Delete').title).toBe('Move to the Recycle Bin · ⌘⌫');
	expect(button('Deselect all').title).toBe('Deselect all · Esc');
	expect(host.querySelector('[role="group"]')?.getAttribute('aria-label')).toBe(
		'The selected entries'
	);

	return unmount(component);
});

/** The bin is read, not edited: nothing in it moves or takes a tag, and what
 * is deleted there goes for good. */
it('offers putting back and deleting for good in the bin, and nothing else', () => {
	const binned = logins.map((each) => ({ ...each, deletion: 'forever' as const }));
	const { component, onPutBack } = draw(binned, true);

	expect(labels()).toEqual(['Put back', 'Delete forever…', 'Deselect all']);
	button('Put back').click();
	expect(onPutBack).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/** A move to the bin is offered back by its notice, so it asks nothing. One
 * entry going for good among them is a deletion nothing takes back, and the
 * bar asks first, naming how many, with the way out first. */
it('deletes straight away when everything goes to the bin, and asks first when anything would go for good', () => {
	const binning = draw();
	button('Delete').click();
	flushSync();
	expect(binning.onDelete).toHaveBeenCalledTimes(1);
	expect(host.querySelector('[data-confirm]')).toBeNull();
	void unmount(binning.component);

	const erasing = draw(logins.map((each) => ({ ...each, deletion: 'forever' as const })));
	expect(labels()).toContain('Delete forever…');
	expect(button('Delete forever…').title).toBe('');
	button('Delete forever…').click();
	flushSync();
	expect(erasing.onDelete).not.toHaveBeenCalled();
	const question = host.querySelector('[data-confirm]');
	expect(question?.textContent).toContain('Delete 3 entries forever? This can’t be undone.');
	const answers = [...(question?.querySelectorAll('button') ?? [])].map((each) =>
		each.textContent?.trim()
	);
	expect(answers).toEqual(['Keep them', 'Delete forever']);
	expect(document.activeElement?.textContent?.trim(), 'the focus is not on the way out').toBe(
		'Keep them'
	);

	button('Delete forever').click();
	flushSync();
	expect(erasing.onDelete).toHaveBeenCalledTimes(1);
	expect(host.querySelector('[data-confirm]')).toBeNull();
	return unmount(erasing.component);
});

/** One entry is named, isolated so that a title that turns text around turns
 * nothing else. */
it('names one entry in the question by its title, isolated', () => {
	const { component } = draw([row({ title: 'evil\u202Eslip', deletion: 'forever' })]);
	button('Delete forever…').click();
	flushSync();
	expect(host.querySelector('[data-confirm] p')?.textContent).toBe(
		'Delete “\u2068evil\u202Eslip\u2069” forever? This can’t be undone.'
	);
	expect(button('Keep it')).toBeTruthy();
	return unmount(component);
});

/** Keeping them closes the question and nothing else: the Escape that keeps
 * them is the question's, and the window's own Escape, which would let go of
 * the choice, never hears it. */
it('keeps them on Keep them and on Escape, and the Escape goes no further', () => {
	const { component, onDelete, onClear } = draw(
		logins.map((each) => ({ ...each, deletion: 'forever' as const }))
	);
	button('Delete forever…').click();
	flushSync();
	button('Keep them').click();
	flushSync();
	expect(host.querySelector('[data-confirm]')).toBeNull();

	button('Delete forever…').click();
	flushSync();
	const went = heard(() => key(button('Keep them'), 'Escape'));
	expect(went, 'the window heard the Escape that kept them').toBe(false);
	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(onDelete).not.toHaveBeenCalled();
	expect(onClear).not.toHaveBeenCalled();

	return unmount(component);
});

/** Return puts the tag on, as it was typed less the space around it, and
 * nothing typed is no tag. */
it('puts a tag on with Return, and nothing for an empty one', async () => {
	const { component, onTag } = draw();

	typeTag('  work  ');
	expect(document.activeElement).toBe(tagField());
	const ended = key(tagField(), 'Enter');
	expect(ended.defaultPrevented).toBe(true);
	expect(onTag.mock.calls).toEqual([['work']]);
	expect(host.querySelector('input')).toBeNull();
	await tick();
	expect(document.activeElement, 'the focus did not come back to Add tag').toBe(button('Add tag'));

	typeTag('   ');
	key(tagField(), 'Enter');
	expect(onTag).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/**
 * Leaving the field lets the tag go, the way Escape does: a tag here is a
 * version on every row chosen, and a click on the bar's own Delete, or
 * anywhere else, is not the reader saying it is finished. The window going
 * behind another app is not leaving at all, and what was typed waits for it to
 * come back.
 */
it('lets a tag go when the field is left, and keeps it while the window is away', () => {
	const { component, onTag } = draw();

	typeTag('wo');
	const away = vi.spyOn(document, 'hasFocus').mockReturnValue(false);
	tagField().dispatchEvent(new FocusEvent('blur'));
	flushSync();
	expect(onTag, 'the window going away put a half-typed tag on').not.toHaveBeenCalled();
	expect(tagField().value).toBe('wo');

	away.mockReturnValue(true);
	tagField().dispatchEvent(new FocusEvent('blur'));
	flushSync();
	expect(onTag, 'leaving the field put the tag on').not.toHaveBeenCalled();
	expect(host.querySelector('input')).toBeNull();

	typeTag('home');
	button('Delete').dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
	tagField().dispatchEvent(new FocusEvent('blur'));
	button('Delete').click();
	flushSync();
	expect(onTag, 'a press of Delete put the tag on as well').not.toHaveBeenCalled();

	away.mockRestore();
	return unmount(component);
});

it('lets a tag go on Escape, and the Escape goes no further', () => {
	const { component, onTag, onClear } = draw();

	typeTag('half typed');
	const went = heard(() => key(tagField(), 'Escape'));
	expect(went, 'the window heard the Escape and let go of the choice').toBe(false);
	expect(host.querySelector('input')).toBeNull();
	expect(onTag).not.toHaveBeenCalled();
	expect(onClear).not.toHaveBeenCalled();

	return unmount(component);
});

/** The Return that confirms a kana conversion, and the Escape that cancels
 * one, are the input method's: neither puts a tag on nor lets one go. */
it('leaves a Return or an Escape that ends a composition to the input method', () => {
	const { component, onTag } = draw();

	typeTag('かな');
	key(tagField(), 'Enter', { isComposing: true });
	key(tagField(), 'Enter', { keyCode: 229 });
	key(tagField(), 'Escape', { isComposing: true });
	expect(onTag).not.toHaveBeenCalled();
	expect(tagField().value).toBe('かな');

	key(tagField(), 'Enter');
	expect(onTag.mock.calls).toEqual([['かな']]);

	return unmount(component);
});

it('deselects from the cross', () => {
	const { component, onClear } = draw();
	button('Deselect all').click();
	expect(onClear).toHaveBeenCalledTimes(1);
	return unmount(component);
});

/** The folder list is the one an entry's line opens, marking the folder every
 * chosen entry is in - choosing it again moves nothing - and its Escape is its
 * own. */
it('moves them to a folder from the list, and marks the one they are all in', () => {
	const { component, onMove, onClear } = draw();

	button('Move to…').click();
	flushSync();
	const filter = host.querySelector<HTMLInputElement>('input[role="combobox"]');
	if (!filter) throw new Error('the folder list did not open');
	expect(button('Move to…').getAttribute('aria-expanded')).toBe('true');
	const here = host.querySelector('[aria-current="location"]');
	expect(here?.textContent).toContain('Work');

	// Return on the line it opens on, the folder they are in, moves nothing.
	key(filter, 'Enter');
	expect(onMove).not.toHaveBeenCalled();
	expect(host.querySelector('input[role="combobox"]')).toBeNull();

	button('Move to…').click();
	flushSync();
	const went = heard(() => key(host.querySelector('input[role="combobox"]') as Element, 'Escape'));
	expect(went, 'the window heard the list’s Escape').toBe(false);
	expect(onClear).not.toHaveBeenCalled();

	button('Move to…').click();
	flushSync();
	const banks = [...host.querySelectorAll<HTMLButtonElement>('[role="option"]')].find((each) =>
		each.textContent?.includes('Banking')
	);
	banks?.click();
	flushSync();
	expect(onMove).toHaveBeenCalledWith(banking.id);
	expect(host.querySelector('input[role="combobox"]')).toBeNull();

	return unmount(component);
});

/** Entries from several folders are in no one place, and every folder is one
 * they can go to. */
it('marks no folder when the chosen entries are in several', () => {
	const { component } = draw([logins[0], { ...logins[1], group: banking.id }]);
	button('Move to…').click();
	flushSync();
	expect(host.querySelector('[aria-current="location"]')).toBeNull();
	return unmount(component);
});
