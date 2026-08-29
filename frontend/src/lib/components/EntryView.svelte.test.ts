import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { entry, field, group } from '$lib/fixtures';
import EntryView from './EntryView.svelte';

const ipc = vi.hoisted(() => ({ reveal: vi.fn(), openUrl: vi.fn() }));
vi.mock('$lib/ipc', () => ipc);

const SECRET = 'correct horse battery staple';
const MARKUP = '<script>alert(1)</script>';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.reveal.mockResolvedValue(SECRET);
	ipc.openUrl.mockResolvedValue(undefined);
});

afterEach(() => {
	host.remove();
	localStorage.clear();
});

function show(entryOver: Parameters<typeof entry>[0]) {
	return mount(EntryView, {
		target: host,
		props: {
			entry: entry(entryOver),
			path: [group({ name: 'Work' })],
			now: new Date('2026-08-29T14:30:00Z'),
			onCopy: vi.fn(),
			onFailure: vi.fn()
		}
	});
}

function button(label: string): HTMLButtonElement {
	const found = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === label
	);
	if (!found) throw new Error(`no button labelled ${label}`);
	return found;
}

/** Every attribute value anywhere in the pane. A secret in one of these is a
 * secret in the accessibility tree, in a copied element and in a crash dump. */
function attributes(): string[] {
	return [...host.querySelectorAll('*')].flatMap((element) =>
		[...element.attributes].map((attribute) => attribute.value)
	);
}

it('writes a value that is markup as text and never as markup', () => {
	const component = show({
		fields: [
			field({ name: 'Title', kind: 'title', value: MARKUP, empty: false }),
			field({ name: 'Notes', kind: 'notes', value: `notes ${MARKUP}`, empty: false }),
			field({ name: 'evil', kind: 'custom', value: `<img src=x onerror="alert(1)">`, empty: false })
		]
	});
	flushSync();

	expect(host.querySelector('script')).toBeNull();
	expect(host.querySelector('img')).toBeNull();
	expect(host.textContent).toContain(MARKUP);
	expect(attributes().join(' ')).not.toContain('onerror');

	return unmount(component);
});

it('keeps a revealed password in a text node and nowhere else', async () => {
	const component = show({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	flushSync();

	expect(host.textContent).not.toContain(SECRET);

	button('Show').click();
	await vi.waitFor(() => expect(host.textContent).toContain(SECRET));
	flushSync();

	// The value is on the screen, and it is only on the screen.
	expect(attributes().join(' ')).not.toContain(SECRET);
	expect(host.innerHTML.replace(SECRET, '')).not.toContain(SECRET);
	expect(JSON.stringify(localStorage)).not.toContain(SECRET);
	expect(JSON.stringify(sessionStorage)).not.toContain(SECRET);
	expect(window.location.href).not.toContain(SECRET);
	expect(document.title).not.toContain(SECRET);

	button('Hide').click();
	flushSync();
	expect(host.innerHTML).not.toContain(SECRET);

	return unmount(component);
});

it('takes the value off the screen when the pane goes', async () => {
	const component = show({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	flushSync();
	button('Show').click();
	await vi.waitFor(() => expect(host.textContent).toContain(SECRET));

	await unmount(component);
	expect(document.body.textContent).not.toContain(SECRET);
});

it('asks for a password once per reveal and never on its own', async () => {
	const component = show({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	flushSync();
	expect(ipc.reveal).not.toHaveBeenCalled();

	button('Show').click();
	await vi.waitFor(() => expect(ipc.reveal).toHaveBeenCalledTimes(1));
	flushSync();

	button('Hide').click();
	flushSync();
	expect(ipc.reveal).toHaveBeenCalledTimes(1);

	return unmount(component);
});

it('says there is nothing to reveal when the entry has no password', () => {
	const component = show({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: true })]
	});
	flushSync();

	expect(host.textContent).toContain('No password on this entry');
	expect(
		[...host.querySelectorAll('button')].map((each) => each.textContent?.trim())
	).not.toContain('Show');

	return unmount(component);
});

it('offers an address only when Coffer would open it', () => {
	const dangerous = show({
		fields: [
			field({
				name: 'URL',
				kind: 'url',
				value: 'javascript:alert(1)',
				empty: false,
				openable: false
			})
		]
	});
	flushSync();

	expect(host.textContent).toContain('javascript:alert(1)');
	expect(
		[...host.querySelectorAll('button')].map((each) => each.textContent?.trim())
	).not.toContain('javascript:alert(1)');
	expect(host.querySelector('a')).toBeNull();
	unmount(dangerous);

	const ordinary = show({
		fields: [
			field({
				name: 'URL',
				kind: 'url',
				value: 'https://example.com',
				empty: false,
				openable: true
			})
		]
	});
	flushSync();

	button('https://example.com').click();
	expect(ipc.openUrl).toHaveBeenCalledTimes(1);

	return unmount(ordinary);
});
