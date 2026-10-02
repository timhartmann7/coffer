import { flushSync, mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { kinds, row } from '$lib/fixtures';
import type { EntryRow } from '$lib/model';
import NewEntry from './NewEntry.svelte';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => {
	host.remove();
	vi.restoreAllMocks();
});

function draw(
	over: { templates?: EntryRow[]; asking?: boolean; choosing?: boolean } = {}
): ReturnType<typeof mount> & {
	onMake: ReturnType<typeof vi.fn>;
	onTemplate: ReturnType<typeof vi.fn>;
	onDismiss: ReturnType<typeof vi.fn>;
} {
	const onMake = vi.fn();
	const onTemplate = vi.fn();
	const onDismiss = vi.fn();
	const component = mount(NewEntry, {
		target: host,
		props: {
			offered: kinds().offered,
			templates: over.templates ?? [],
			asking: over.asking ?? false,
			choosing: over.choosing ?? false,
			onMake,
			onTemplate,
			onDismiss
		}
	});
	flushSync();
	return Object.assign(component, { onMake, onTemplate, onDismiss });
}

const pill = () =>
	[...host.querySelectorAll('button')].find((each) => each.textContent?.trim() === 'Entry');
const chevron = () => host.querySelector<HTMLButtonElement>('[aria-label="Other kinds of entry"]');
const menu = () => host.querySelector<HTMLElement>('[role="menu"]');

/** The lines of the list, as they read. */
function lines(): string[] {
	return [...host.querySelectorAll('[role="menuitem"]')].map(
		(line) => line.getAttribute('aria-label') ?? line.textContent?.trim() ?? ''
	);
}

/** What the line with the focus says. */
function focused(): string | null {
	const at = document.activeElement;
	return at?.getAttribute('role') === 'menuitem' ? (at.textContent?.trim() ?? null) : null;
}

async function opened() {
	chevron()?.click();
	await tick();
	flushSync();
}

function key(key: string, over: KeyboardEventInit = {}): KeyboardEvent {
	const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...over });
	(document.activeElement ?? host).dispatchEvent(event);
	flushSync();
	return event;
}

/** The pill names no kind: what is made when nobody chose is the screen's
 * to say, in one place, for the pill and New Entry in the menu bar alike. */
it('makes what nobody chose from the pill, and opens no list', () => {
	const component = draw();
	pill()?.click();
	expect(component.onMake).toHaveBeenCalledTimes(1);
	expect(component.onMake).toHaveBeenCalledWith();
	expect(menu(), 'the pill opened the list').toBeNull();
	return unmount(component);
});

it('lists every kind in order, then the vault’s templates under their own heading', async () => {
	const titled = row({ title: 'Card template' });
	const component = draw({ templates: [titled] });
	await opened();

	expect(chevron()?.getAttribute('aria-expanded')).toBe('true');
	expect(lines()).toEqual([...kinds().offered.map((offer) => offer.name), 'Card template']);
	const group = menu()?.querySelector('[role="group"]');
	expect(group?.getAttribute('aria-labelledby') ?? '').not.toBe('');
	expect(
		document.getElementById(group?.getAttribute('aria-labelledby') ?? '')?.textContent?.trim()
	).toBe('Templates in this vault');
	expect(focused(), 'opening put the focus nowhere in the list').toBe('Login');

	[...host.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].at(-1)?.click();
	flushSync();
	expect(component.onTemplate).toHaveBeenCalledWith(titled);
	expect(component.onMake).not.toHaveBeenCalled();
	expect(menu(), 'the list stayed open').toBeNull();
	expect(document.activeElement, 'the chevron took the focus back from the entry').not.toBe(
		chevron()
	);
	return unmount(component);
});

it('draws no heading for a vault that keeps no templates', async () => {
	const component = draw();
	await opened();
	expect(menu()?.textContent).not.toContain('Templates in this vault');
	expect(menu()?.querySelector('[role="group"]')).toBeNull();
	return unmount(component);
});

it('steps through the list with the arrows, round from either end', async () => {
	const template = row({ title: 'Wi-Fi at the office' });
	const component = draw({ templates: [template] });
	await opened();

	key('ArrowUp');
	expect(focused(), 'up from the first line').toBe('Wi-Fi at the office');
	key('ArrowDown');
	expect(focused(), 'down from the last line').toBe('Login');
	key('ArrowDown');
	expect(focused()).toBe('Bank card');

	// The line the keys are on is the one made.
	(document.activeElement as HTMLElement | null)?.click();
	expect(component.onMake).toHaveBeenCalledWith(kinds().offered[1]);
	await unmount(component);

	// Down on the chevron opens the list on its first line, up on its last.
	const again = draw({ templates: [template] });
	chevron()?.focus();
	const down = key('ArrowDown');
	await tick();
	flushSync();
	expect(down.defaultPrevented).toBe(true);
	expect(focused()).toBe('Login');
	key('Escape');
	key('ArrowUp');
	await tick();
	flushSync();
	expect(focused()).toBe('Wi-Fi at the office');
	return unmount(again);
});

/** Escape closes the list and goes no further: the window's own Escape would
 * clear the search or put the pane away behind it. */
it('closes on Escape with the focus back on the chevron, and keeps the key', async () => {
	const component = draw();
	const window_ = vi.fn();
	window.addEventListener('keydown', window_);
	try {
		await opened();
		const escape = key('Escape');
		expect(menu()).toBeNull();
		expect(escape.defaultPrevented).toBe(true);
		expect(window_, 'the window heard the Escape that closed the list').not.toHaveBeenCalled();
		expect(document.activeElement).toBe(chevron());

		// An Escape that ends a composition is the input method's.
		await opened();
		key('Escape', { isComposing: true });
		expect(menu(), 'a composition’s Escape closed the list').not.toBeNull();
	} finally {
		window.removeEventListener('keydown', window_);
	}
	return unmount(component);
});

/** With the focus on the chevron and the list still open - Shift+Tab from the
 * first line - Escape closes the list the way it does from a line, and goes no
 * further. */
it('closes on Escape from the chevron too, and keeps the key', async () => {
	const component = draw();
	const window_ = vi.fn();
	window.addEventListener('keydown', window_);
	try {
		await opened();
		chevron()?.focus();
		flushSync();
		expect(menu()).not.toBeNull();
		const escape = key('Escape');
		expect(menu(), 'Escape on the chevron left the list open').toBeNull();
		expect(escape.defaultPrevented).toBe(true);
		expect(window_, 'the window heard the Escape that closed the list').not.toHaveBeenCalled();
		expect(document.activeElement).toBe(chevron());

		// With the list closed, Escape is the window's.
		key('Escape');
		expect(window_).toHaveBeenCalledTimes(1);
	} finally {
		window.removeEventListener('keydown', window_);
	}
	return unmount(component);
});

/** A second press on the chevron closes the list. In WebKit a pressed button
 * takes no focus, so a press that let it go would take it off the line to
 * nothing: the list would close as the focus left, and the click would open it
 * again. The press keeps it, and the click closes the list with the focus on
 * the chevron rather than on the line going away. */
it('closes on a second press of the chevron, with the focus on it', async () => {
	const component = draw();
	await opened();
	expect(focused()).toBe('Login');

	const down = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
	chevron()?.dispatchEvent(down);
	expect(down.defaultPrevented, 'the press took the focus off the line').toBe(true);
	chevron()?.click();
	flushSync();
	expect(menu(), 'the second press left the list open').toBeNull();
	expect(chevron()?.getAttribute('aria-expanded')).toBe('false');
	expect(document.activeElement).toBe(chevron());

	// With the list closed, the press is an ordinary one.
	const opening = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
	chevron()?.dispatchEvent(opening);
	expect(opening.defaultPrevented).toBe(false);
	return unmount(component);
});

/** The list closes when the focus leaves the control - for the next control a
 * Tab reaches, or for the page a press outside lands on - and not when it
 * moves between the chevron and the lines. */
it('closes when the focus leaves it, and only then', async () => {
	const elsewhere = document.createElement('button');
	document.body.appendChild(elsewhere);
	const component = draw();
	try {
		await opened();
		chevron()?.focus();
		flushSync();
		expect(menu(), 'moving to the chevron closed it').not.toBeNull();

		elsewhere.focus();
		flushSync();
		expect(menu()).toBeNull();
		expect(chevron()?.getAttribute('aria-expanded')).toBe('false');
	} finally {
		elsewhere.remove();
	}
	return unmount(component);
});

/** A template is named by the vault, and so is drawn the way every name from
 * it is: isolated, the mask for a name the database protects, and "Untitled"
 * for none at all. */
it('draws a template’s name isolated, protected, or missing the way the pane does', async () => {
	const component = draw({
		templates: [
			row({ title: '‮evil' }),
			row({ title: null }),
			row({ title: '' }),
			row({ title: '<img src=x onerror=alert(1)>' })
		]
	});
	await opened();

	const drawn = [...host.querySelectorAll('[role="group"] [role="menuitem"]')];
	expect(drawn[0].querySelector('bdi')?.textContent).toBe('‮evil');
	expect(drawn[1].getAttribute('aria-label')).toBe('A template whose name is hidden');
	expect(drawn[1].querySelector('svg')).not.toBeNull();
	expect(drawn[2].textContent?.trim()).toBe('Untitled');
	expect(drawn[3].querySelector('img'), 'a name was drawn as markup').toBeNull();
	expect(drawn[3].textContent?.trim()).toBe('<img src=x onerror=alert(1)>');
	return unmount(component);
});

/** While the pill asks where, it says so, and a second press on it closes the
 * question rather than asking again. */
it('says the pill asks where, and closes the question on a second press', () => {
	const component = draw({ asking: true, choosing: true });
	expect(pill()?.getAttribute('aria-haspopup')).toBe('listbox');
	expect(pill()?.getAttribute('aria-expanded')).toBe('true');

	const down = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
	pill()?.dispatchEvent(down);
	expect(down.defaultPrevented, 'the press took the focus out of the question').toBe(true);
	pill()?.click();
	expect(component.onDismiss).toHaveBeenCalledTimes(1);
	expect(component.onMake).not.toHaveBeenCalled();
	void unmount(component);

	const plain = draw();
	expect(pill()?.getAttribute('aria-haspopup')).toBeNull();
	return unmount(plain);
});
