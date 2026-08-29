import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { attachment, entry, field, group } from '$lib/fixtures';
import EntryView from './EntryView.svelte';

const ipc = vi.hoisted(() => ({
	reveal: vi.fn(),
	openUrl: vi.fn(),
	setField: vi.fn(),
	removeField: vi.fn(),
	setTags: vi.fn(),
	addAttachment: vi.fn(),
	exportAttachment: vi.fn(),
	removeAttachment: vi.fn(),
	generatePassword: vi.fn(),
	versions: vi.fn(),
	version: vi.fn(),
	revealVersion: vi.fn(),
	restoreVersion: vi.fn(),
	deleteVersion: vi.fn(),
	clearHistory: vi.fn()
}));
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
			versions: [],
			now: new Date('2026-08-29T14:30:00Z'),
			onCopy: vi.fn(),
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onDelete: vi.fn(),
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

/**
 * Everything the pane has on screen: its text, and what is in the fields it
 * can be edited through.
 *
 * A value being edited is the `value` of an input, which is a property of one
 * DOM node rather than an attribute or a text node. It is on the screen and it
 * is nowhere else, which is what the assertions below are about.
 */
function screen(): string {
	const written = [...host.querySelectorAll('input, textarea')].map(
		(field) => (field as HTMLInputElement | HTMLTextAreaElement).value
	);
	return [host.textContent ?? '', ...written].join(' ');
}

/** What the field showing a secret has in it. */
function value(): string {
	const found = host.querySelector('[data-value]');
	if (!found) throw new Error('the pane has nowhere to show a value');
	return found instanceof HTMLInputElement ? found.value : (found.textContent ?? '');
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
	expect(screen()).toContain(MARKUP);
	expect(attributes().join(' ')).not.toContain('onerror');

	return unmount(component);
});

it('keeps a revealed password in one node and nowhere else', async () => {
	const component = show({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	flushSync();

	expect(screen()).not.toContain(SECRET);

	button('Show').click();
	await vi.waitFor(() => expect(value()).toBe(SECRET));
	flushSync();

	// The value is on the screen, and it is only on the screen: not in the
	// markup, not in an attribute, and nowhere a crash could write it down.
	expect(attributes().join(' ')).not.toContain(SECRET);
	expect(host.innerHTML).not.toContain(SECRET);
	expect(JSON.stringify(localStorage)).not.toContain(SECRET);
	expect(JSON.stringify(sessionStorage)).not.toContain(SECRET);
	expect(window.location.href).not.toContain(SECRET);
	expect(document.title).not.toContain(SECRET);

	button('Hide').click();
	flushSync();
	expect(screen()).not.toContain(SECRET);

	return unmount(component);
});

it('takes the value off the screen when the pane goes', async () => {
	const component = show({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	flushSync();
	button('Show').click();
	await vi.waitFor(() => expect(value()).toBe(SECRET));

	await unmount(component);
	expect(document.body.textContent).not.toContain(SECRET);
	expect(
		[...document.body.querySelectorAll('input')].map((field) => field.value).join(' ')
	).not.toContain(SECRET);
});

/** A reveal that is still in flight when the pane goes has nowhere to put its
 * answer, and the answer must not be written into an element that is off the
 * screen and can no longer be wiped. */
it('drops a value that arrives after the pane has gone', async () => {
	let answer: (value: string) => void = () => {};
	ipc.reveal.mockReturnValue(
		new Promise<string>((resolve) => {
			answer = resolve;
		})
	);

	const component = show({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	flushSync();

	// The node the value would have gone into, held on to the way a leak would.
	const target = host.querySelector('[data-value]');
	if (!target) throw new Error('the pane has nowhere to show a value');

	button('Show').click();
	flushSync();

	await unmount(component);
	answer(SECRET);
	await Promise.resolve();
	await Promise.resolve();
	flushSync();

	expect((target as HTMLInputElement).value).toBe('');
	expect(document.body.textContent).not.toContain(SECRET);
});

/** Thirty seconds is the only bound on how long a password stays on the screen
 * when nobody hides it. */
it('takes the value off the screen when its half minute is up', async () => {
	vi.useFakeTimers();
	try {
		const component = show({
			fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
		});
		flushSync();

		const target = host.querySelector('[data-value]');
		if (!target) throw new Error('the pane has nowhere to show a value');

		button('Show').click();
		await Promise.resolve();
		await Promise.resolve();
		flushSync();
		expect((target as HTMLInputElement).value).toBe(SECRET);

		vi.advanceTimersByTime(29_000);
		flushSync();
		expect((target as HTMLInputElement).value).toBe(SECRET);

		vi.advanceTimersByTime(1_000);
		flushSync();
		expect((target as HTMLInputElement).value).toBe('');
		expect(host.textContent).not.toContain('Hides in');

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** A database may protect any field. The login, the address and the notes then
 * arrive the same way a protected custom field does, and the pane has to offer
 * the same way in. */
it('offers a reveal for a login the database protects', async () => {
	const component = show({
		fields: [field({ name: 'UserName', kind: 'username', value: null, empty: false })]
	});
	flushSync();

	const eye = host.querySelector('[aria-label="Show UserName"]');
	expect(eye).not.toBeNull();
	expect(host.querySelector('use[href="#redact"]')).not.toBeNull();

	(eye as HTMLButtonElement).click();
	await vi.waitFor(() => expect(screen()).toContain(SECRET));
	expect(ipc.reveal).toHaveBeenCalledWith(expect.any(String), 'UserName');

	return unmount(component);
});

it('offers a reveal for notes the database protects', async () => {
	const component = show({
		fields: [field({ name: 'Notes', kind: 'notes', value: null, empty: false })]
	});
	flushSync();

	const eye = host.querySelector('[aria-label="Show Notes"]');
	expect(eye).not.toBeNull();

	(eye as HTMLButtonElement).click();
	await vi.waitFor(() => expect(screen()).toContain(SECRET));

	return unmount(component);
});

/** A file may hold the same tag twice, and a list keyed by the tag throws the
 * whole pane away when it does. */
it('draws an entry whose tags repeat', () => {
	const component = show({ tags: ['prod', 'prod', '', ''] });
	flushSync();

	expect(host.querySelectorAll('span.rounded-full')).toHaveLength(4);

	return unmount(component);
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

/** An entry with no password has nothing to reveal, so the pane offers to write
 * one rather than asking the vault for a value that is not there. */
it('offers to set a password rather than to reveal one when there is none', async () => {
	const component = show({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: true })]
	});
	flushSync();

	expect(host.textContent).toContain('No password on this entry');
	const named = [...host.querySelectorAll('button')].map((each) => each.textContent?.trim());
	expect(named).not.toContain('Show');
	expect(named).toContain('Set one');

	button('Set one').click();
	await Promise.resolve();
	flushSync();
	expect(ipc.reveal).not.toHaveBeenCalled();
	expect(value()).toBe('');

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

	expect(screen()).toContain('javascript:alert(1)');
	expect(host.querySelector('[aria-label="Open this address"]')).toBeNull();
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

	const opener = host.querySelector('[aria-label="Open this address"]');
	expect(opener).not.toBeNull();
	(opener as HTMLButtonElement).click();
	expect(ipc.openUrl).toHaveBeenCalledTimes(1);

	return unmount(ordinary);
});

/**
 * The name in the file is shown as the file holds it, and the name the save
 * panel is offered is the safe one. A pane that built a path out of the first
 * would be handing `../../escape.txt` to a writer.
 */
it('shows a file under the name the database holds and exports it under a safe one', async () => {
	const onChanged = vi.fn();
	ipc.exportAttachment.mockResolvedValue(undefined);
	ipc.removeAttachment.mockResolvedValue(entry());

	const component = mount(EntryView, {
		target: host,
		props: {
			entry: entry({
				attachments: [attachment({ name: '../../escape.txt', fileName: 'escape.txt', size: 12 })]
			}),
			path: [group({ name: 'Work' })],
			versions: [],
			now: new Date('2026-08-29T14:30:00Z'),
			onCopy: vi.fn(),
			onChanged,
			onVersions: vi.fn(),
			onDelete: vi.fn(),
			onFailure: vi.fn()
		}
	});
	flushSync();

	expect(host.textContent).toContain('../../escape.txt');
	host.querySelector<HTMLButtonElement>('[aria-label="Write escape.txt out"]')?.click();
	await vi.waitFor(() =>
		expect(ipc.exportAttachment).toHaveBeenCalledWith(expect.any(String), '../../escape.txt')
	);

	host.querySelector<HTMLButtonElement>('[aria-label="Remove ../../escape.txt"]')?.click();
	await vi.waitFor(() => expect(ipc.removeAttachment).toHaveBeenCalled());
	expect(onChanged).toHaveBeenCalled();

	return unmount(component);
});

/**
 * A field the database keeps protected has to go back protected. The screen
 * cannot work that out from the value, because a protected value never crosses:
 * it reads it off the field it was given and sends it back unchanged.
 */
it('writes a field back with the protection it arrived with', () => {
	ipc.setField.mockResolvedValue(entry());

	const component = show({
		fields: [
			field({ name: 'Title', kind: 'title', value: 'a login', empty: false, protected: false }),
			field({ name: 'UserName', kind: 'username', value: 'alice', empty: false, protected: false })
		]
	});
	flushSync();

	const login = host.querySelector('[aria-label="Login"]') as HTMLInputElement;
	login.value = 'bob';
	login.dispatchEvent(new Event('blur'));

	expect(ipc.setField).toHaveBeenCalledWith(expect.any(String), 'UserName', 'bob', false);

	return unmount(component);
});
