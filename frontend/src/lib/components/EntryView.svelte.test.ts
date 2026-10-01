import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { attachment, entry, field, group } from '$lib/fixtures';
import type { Attached, Clash } from '$lib/model';
import { reactive } from '$lib/props.svelte';
import EntryView from './EntryView.svelte';

const ipc = vi.hoisted(() => ({
	reveal: vi.fn(),
	openUrl: vi.fn(),
	setField: vi.fn(),
	removeField: vi.fn(),
	setTags: vi.fn(),
	addAttachment: vi.fn(),
	keepBothAttachments: vi.fn(),
	replaceAttachment: vi.fn(),
	withdrawAttachment: vi.fn(),
	exportAttachment: vi.fn(),
	removeAttachment: vi.fn(),
	generatePassword: vi.fn(),
	versions: vi.fn(),
	version: vi.fn(),
	revealVersion: vi.fn(),
	restoreVersion: vi.fn(),
	deleteVersion: vi.fn(),
	clearHistory: vi.fn(),
	removeAttachmentAndVersions: vi.fn(),
	// The same reading the real one does: a command rejects with the value Rust
	// serialised, and anything else is not one.
	asFailure: (thrown: unknown) =>
		thrown && typeof (thrown as { message?: unknown }).message === 'string'
			? (thrown as { code: string; message: string })
			: { code: 'other', message: 'Coffer could not finish that.' }
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
	ipc.withdrawAttachment.mockResolvedValue(undefined);
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
			readOnly: false,
			onCopy: vi.fn(),
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onFieldRemoved: vi.fn(),
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

/**
 * Hide hides, once, and writes nothing.
 *
 * The reveal focuses the field so the value can be edited where it stands, and
 * a press on Hide therefore blurs it. The blur committed - which saved the file
 * and derived the key again, a second of a window that answers nothing - and it
 * left the field closed, so the press it had raced opened the value again with
 * a fresh half minute. The button said Hide and did the opposite, slowly.
 */
it('hides the value on a press, without writing it or opening it again', async () => {
	const component = show({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	flushSync();

	button('Show').click();
	await vi.waitFor(() => expect(value()).toBe(SECRET));
	flushSync();

	// What the reveal does, and what makes the press below a blur as well.
	const node = host.querySelector('[data-value]') as HTMLInputElement;
	node.focus();
	expect(document.activeElement, 'the reveal did not focus the field').toBe(node);

	const hide = button('Hide');
	const press = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
	hide.dispatchEvent(press);
	expect(press.defaultPrevented, 'the press moves the focus off the live field').toBe(true);
	hide.click();
	flushSync();

	expect(screen(), 'the value is still on the screen').not.toContain(SECRET);
	expect(ipc.setField, 'hiding a value wrote it back').not.toHaveBeenCalled();
	expect(ipc.reveal, 'hiding a value asked for it again').toHaveBeenCalledTimes(1);
	expect(host.textContent, 'the countdown is still running').not.toContain('Hides in');

	return unmount(component);
});

/**
 * Hide is the one press that keeps the focus, and it is not where a reader goes
 * next: they press Copy, or click another entry, or reach for the search box.
 * Every one of those blurs the live field, and the blur used to write the
 * revealed value straight back - a full save, a second of key derivation, and
 * one of the ten snapshots beside the vault rotated away, spent on the reader
 * having looked at a password.
 */
it('does not write a revealed value back when the focus leaves the field', async () => {
	const component = show({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	flushSync();

	button('Show').click();
	await vi.waitFor(() => expect(value()).toBe(SECRET));
	flushSync();

	const node = host.querySelector('[data-value]') as HTMLInputElement;
	node.focus();
	node.dispatchEvent(new FocusEvent('blur'));
	flushSync();

	expect(ipc.setField, 'looking at a password rewrote the whole vault').not.toHaveBeenCalled();
	expect(screen(), 'the value is still on the screen').not.toContain(SECRET);

	return unmount(component);
});

/** The other half of the same rule: a value the reader typed is still written. */
it('writes a password the reader typed over a revealed one', async () => {
	const component = show({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	flushSync();

	button('Show').click();
	await vi.waitFor(() => expect(value()).toBe(SECRET));
	flushSync();

	const node = host.querySelector('[data-value]') as HTMLInputElement;
	node.focus();
	node.value = 'a different one';
	node.dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
	node.dispatchEvent(new FocusEvent('blur'));
	flushSync();

	await vi.waitFor(() =>
		expect(ipc.setField).toHaveBeenCalledWith(
			expect.any(String),
			'Password',
			'a different one',
			false
		)
	);

	return unmount(component);
});

/** Opening the field on an entry that has none and typing nothing is not an
 * edit either, and it is the one case where the pane knows what it would be
 * writing over. */
it('does not write an empty password when the field was opened and left alone', async () => {
	const component = show({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: true })]
	});
	flushSync();

	button('Set one').click();
	await Promise.resolve();
	flushSync();

	const node = host.querySelector('[data-value]') as HTMLInputElement;
	node.dispatchEvent(new FocusEvent('blur'));
	flushSync();

	expect(ipc.setField).not.toHaveBeenCalled();

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
			readOnly: false,
			onCopy: vi.fn(),
			onChanged,
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onFieldRemoved: vi.fn(),
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
	flushSync();
	button('Remove').click();
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

/**
 * What a field can be asked to hold, from the standing attack list.
 *
 * The box every value now sits in has a floor and clips to its own edges, so
 * the thing to prove is that neither of those loses anything: a megabyte still
 * arrives whole, a right-to-left override does not reach out of the field it is
 * in, and the fields under a long one are still on the screen to be read.
 */
it('holds a value of any size or direction without losing what is under it', () => {
	const LONG = 'a'.repeat(1_000_000);
	const FLIPPED = 'note\u202Egnihtemos';
	const NULLED = 'before\u0000after';

	const component = show({
		fields: [
			field({ name: 'Title', kind: 'title', value: 'an entry', empty: false }),
			field({ name: 'Notes', kind: 'notes', value: LONG, empty: false }),
			field({ name: 'Flipped', kind: 'custom', value: FLIPPED, empty: false }),
			field({ name: 'Nulled', kind: 'custom', value: NULLED, empty: false })
		]
	});
	flushSync();

	const notes = host.querySelector('textarea[aria-label="Notes"]') as HTMLTextAreaElement;
	expect(notes?.value, 'a megabyte was truncated on the way to the screen').toHaveLength(
		LONG.length
	);

	// The fields after it are still drawn. A box that grew without bound inside a
	// pane that scrolls is a long note pushing the rest of the entry away, not out
	// of the document - and this is what says so.
	const flipped = host.querySelector('[aria-label="Flipped"]') as HTMLInputElement;
	const nulled = host.querySelector('[aria-label="Nulled"]') as HTMLInputElement;
	expect(flipped?.value, 'the override reached out of its own field').toBe(FLIPPED);
	expect(nulled?.value, 'the null byte was dropped or split the value').toBe(NULLED);

	// Each of them is in a box of its own, and that box clips - so a value
	// cannot be drawn over the label of the field below it. Anchored to the box
	// rather than to any clipping ancestor, because the pane clips too.
	for (const each of [notes, flipped, nulled]) {
		const box = each?.closest('[class*="min-h-"]');
		expect(box, 'a value is drawn outside a field box').not.toBeNull();
		expect(box?.className, 'the box a value is drawn in does not clip').toContain(
			'overflow-hidden'
		);
	}

	return unmount(component);
});

/**
 * The same rule for a field of the reader's own, which is the one the pane has
 * to decide rather than read off a standard name.
 *
 * A protected custom field that is empty has no value to reveal, so it is drawn
 * as a field like any other - and the protection has to survive that. Writing it
 * back unprotected puts a value the database was keeping protected into the file
 * as plain text, which is the one thing this boundary exists to prevent, and
 * nothing else in the window would notice.
 */
it('writes a custom field back with the protection it arrived with', () => {
	ipc.setField.mockResolvedValue(entry());

	const component = show({
		fields: [
			field({ name: 'Region', kind: 'custom', value: 'eu-central', empty: false }),
			field({ name: 'API token', kind: 'custom', value: null, empty: true, protected: true })
		]
	});
	flushSync();

	const plain = host.querySelector('[aria-label="Region"]') as HTMLInputElement;
	plain.value = 'eu-west';
	plain.dispatchEvent(new Event('blur'));
	expect(ipc.setField).toHaveBeenCalledWith(expect.any(String), 'Region', 'eu-west', false);

	const kept = host.querySelector('[aria-label="API token"]') as HTMLInputElement;
	kept.value = 'a new token';
	kept.dispatchEvent(new Event('blur'));
	expect(ipc.setField).toHaveBeenCalledWith(expect.any(String), 'API token', 'a new token', true);

	return unmount(component);
});

/**
 * A file a previous version still holds cannot go: the format keeps versions
 * inside the entry and nothing can rewrite one. The pane says so and offers the
 * one sequence that works, rather than reporting a failure the reader can do
 * nothing about.
 */
it('offers to clear the versions that are holding a file back', async () => {
	const onChanged = vi.fn();
	const onVersions = vi.fn();
	const onFailure = vi.fn();
	ipc.removeAttachment
		.mockRejectedValueOnce({
			code: 'attachmentInHistory',
			message: '2 earlier versions still hold that file'
		})
		.mockResolvedValueOnce(entry());
	ipc.removeAttachmentAndVersions.mockResolvedValue(entry());

	const component = mount(EntryView, {
		target: host,
		props: {
			entry: entry({ attachments: [attachment({ name: 'id_ed25519' })] }),
			path: [group({ name: 'Work' })],
			versions: [],
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy: vi.fn(),
			onChanged,
			onVersions,
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onFieldRemoved: vi.fn(),
			onFailure
		}
	});
	flushSync();

	host.querySelector<HTMLButtonElement>('[aria-label="Remove id_ed25519"]')?.click();
	flushSync();
	button('Remove').click();
	await vi.waitFor(() => expect(host.textContent).toContain('2 earlier versions still hold'));
	flushSync();

	// A refusal the reader can act on is not an error message.
	expect(onFailure).not.toHaveBeenCalled();
	expect(onChanged).not.toHaveBeenCalled();

	// One call, not two: the versions go only if the file then goes.
	button('Clear those versions and remove it').click();
	await vi.waitFor(() => expect(ipc.removeAttachmentAndVersions).toHaveBeenCalledTimes(1));
	await vi.waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));

	// The list is not emptied from here. Only the versions that were holding the
	// file go, and which ones those were is the engine's answer: saying so from
	// the pane drew an entry as having no history when it still had most of it,
	// and wrote the whole vault a second time to say it.
	expect(onVersions).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * Writing a field writes over whatever that name held. A "new" field named
 * after one the entry already has would empty it - and typing `Password` would
 * empty the password.
 */
it('refuses a new field named after one the entry already has', async () => {
	const onFailure = vi.fn();
	const component = mount(EntryView, {
		target: host,
		props: {
			entry: entry({
				fields: [
					field({ name: 'Password', kind: 'password', value: null, empty: false }),
					field({ name: 'Notes', kind: 'notes', value: 'root access', empty: false })
				]
			}),
			path: [group({ name: 'Work' })],
			versions: [],
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy: vi.fn(),
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onFieldRemoved: vi.fn(),
			onFailure
		}
	});
	flushSync();

	host.querySelector<HTMLButtonElement>('[aria-label="Add a field"]')?.click();
	flushSync();

	const named = host.querySelector('[aria-label="The name of the new field"]') as HTMLInputElement;
	named.value = 'Notes';
	named.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
	flushSync();

	expect(ipc.setField).not.toHaveBeenCalled();
	expect(onFailure).toHaveBeenCalledWith(
		expect.objectContaining({ message: expect.stringContaining('already has a field') })
	);

	return unmount(component);
});

/**
 * The pane is one component that another entry is handed to, so anything it is
 * saying about the entry that was open has to go with that entry. A banner
 * offering to clear a history would otherwise clear the wrong one.
 */
it('says nothing about the entry that was open once another one is', async () => {
	// `clearMocks` clears the calls and leaves the queued answers, so a `once`
	// from an earlier test would answer this one.
	ipc.removeAttachment.mockReset();
	ipc.removeAttachment.mockRejectedValue({
		code: 'attachmentInHistory',
		message: '2 earlier versions still hold that file'
	});

	// A props object the test can change, which is how the window hands the pane
	// another entry.
	const props = reactive({
		entry: entry({ attachments: [attachment({ name: 'id_ed25519' })] }),
		path: [group({ name: 'Work' })],
		versions: [],
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onFieldRemoved: vi.fn(),
		onFailure: vi.fn()
	});
	const component = mount(EntryView, { target: host, props });
	flushSync();

	host.querySelector<HTMLButtonElement>('[aria-label="Remove id_ed25519"]')?.click();
	flushSync();
	button('Remove').click();
	await vi.waitFor(() => expect(host.textContent).toContain('2 earlier versions still hold'));
	flushSync();

	props.entry = entry({ attachments: [attachment({ name: 'other.pem' })] });
	flushSync();

	expect(host.textContent).not.toContain('2 earlier versions still hold');

	return unmount(component);
});

/** A database Coffer will not write back has values to read and copy, and
 * nothing at all to change. */
it('offers no change on a database it cannot write', () => {
	const component = mount(EntryView, {
		target: host,
		props: {
			entry: entry({
				fields: [
					field({ name: 'Title', kind: 'title', value: 'node-3', empty: false }),
					field({ name: 'Password', kind: 'password', value: null, empty: false })
				],
				tags: ['prod'],
				attachments: [attachment({ name: 'id_ed25519' })]
			}),
			path: [group({ name: 'Work' })],
			versions: [],
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: true,
			onCopy: vi.fn(),
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onFieldRemoved: vi.fn(),
			onFailure: vi.fn()
		}
	});
	flushSync();

	expect(host.textContent).toContain('node-3');
	expect(host.querySelector('[aria-label="Delete this entry"]')).toBeNull();
	expect(host.querySelector('[aria-label="Add a field"]')).toBeNull();
	expect(host.querySelector('[aria-label="Add a file"]')).toBeNull();
	expect(host.querySelector('[aria-label="Remove id_ed25519"]')).toBeNull();
	expect(host.querySelector('[aria-label="Remove the tag prod"]')).toBeNull();

	// Only the password field, which is not editable until it is revealed.
	const named = [...host.querySelectorAll('button')].map((each) => each.textContent?.trim());
	expect(named).toContain('Show');
	expect(named).not.toContain('Make one');
	expect(named).not.toContain('+ tag');

	return unmount(component);
});

/**
 * The half minute is how long a password Coffer put on the screen stays there.
 * What the reader has started typing is not that: a timer that wiped the field
 * mid-word would take away the password they were writing and leave them
 * looking at an empty one.
 */
it('stops the clock once the reader starts writing a password', async () => {
	vi.useFakeTimers();
	try {
		const component = show({
			fields: [
				field({
					name: 'Password',
					kind: 'password',
					value: null,
					empty: false,
					protected: true
				})
			]
		});
		flushSync();

		button('Show').click();
		await Promise.resolve();
		await Promise.resolve();
		flushSync();
		expect(value()).toBe(SECRET);

		const written = host.querySelector('[data-value]') as HTMLInputElement;
		written.value = 'a password of my own';
		written.dispatchEvent(new Event('input', { bubbles: true }));
		flushSync();

		vi.advanceTimersByTime(60_000);
		flushSync();

		expect(written.value).toBe('a password of my own');
		expect(host.textContent).not.toContain('Hides in');

		// And it is still what gets written when the focus leaves.
		written.dispatchEvent(new Event('blur'));
		expect(ipc.setField).toHaveBeenCalledWith(
			expect.any(String),
			'Password',
			'a password of my own',
			true
		);

		await unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/**
 * The plus opens the field that asks for a name, and closes it again.
 *
 * The press has to be kept from moving the focus, or the open field blurs
 * first, writes a field, and the press then opens a fresh one - which from the
 * outside is a button that flickers and never closes.
 */
it('closes the field it opened when the plus is pressed again', () => {
	const component = show({ fields: [] });
	flushSync();

	const plus = host.querySelector('[aria-label="Add a field"]') as HTMLButtonElement;
	plus.click();
	flushSync();
	expect(host.querySelector('[aria-label="The name of the new field"]')).not.toBeNull();

	// A press that would move the focus is refused, which is what stops the
	// blur from committing on the way out.
	const press = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
	const opener = host.querySelector('[aria-label="Never mind the new field"]') as HTMLButtonElement;
	opener.dispatchEvent(press);
	expect(press.defaultPrevented).toBe(true);

	opener.click();
	flushSync();

	expect(host.querySelector('[aria-label="The name of the new field"]')).toBeNull();
	expect(ipc.setField).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * The whole of what was wrong with this row. Opening an eye swapped an eight
 * pixel mask for a field four times that, put a countdown on a row of its own
 * underneath, and let three buttons that had been sharing the value's line go
 * back to a line of their own - so the entry walked down the screen on every
 * press and back up again half a minute later, unasked.
 *
 * Stated as structure because there is no layout in this environment: one box
 * holds both states, the buttons are not in it, and revealing adds nothing
 * beside it.
 */
it('keeps the password field the same box whether or not it is revealed', async () => {
	const component = show({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	flushSync();

	const box = host.querySelector('[data-value]')?.parentElement;
	const drawn = box?.className;
	const block = box?.parentElement?.parentElement;
	const rows = block?.children.length;

	expect(box?.querySelector('use[href="#redact"]'), 'the mask is somewhere else').not.toBeNull();
	expect(box?.contains(button('Copy')), 'the buttons share the value line').toBe(false);

	button('Show').click();
	await vi.waitFor(() => expect(ipc.reveal).toHaveBeenCalled());
	flushSync();

	expect(host.querySelector('[data-value]')?.parentElement, 'the value moved box').toBe(box);
	expect(box?.className, 'the box is drawn differently once it holds a value').toBe(drawn);
	expect(block?.children.length, 'revealing added a row under the field').toBe(rows);

	// The height itself, which is what the reader sees move. There is no layout
	// in this environment, so the floor that holds the two states to one line is
	// asserted where it is written: an eight pixel mask and a forty pixel field
	// are the same box only because the box is told what it is.
	expect(drawn, 'the box no longer has a floor, so the two states differ again').toMatch(
		/min-h-\[\d+px\]/
	);

	return unmount(component);
});

/**
 * A way out of the pane that is not the way to lose the entry.
 *
 * Escape closed it and nothing else did, so the pane had exactly one visible
 * button and that button was the trash. The close goes beside it and not after
 * it: the mockup's rule is that the destructive action stands at the end of a
 * row, so that missing it costs a movement rather than an entry.
 */
it('offers a way out of the entry, and it is not the way to delete one', () => {
	const onClose = vi.fn();
	const onDelete = vi.fn();
	const component = mount(EntryView, {
		target: host,
		props: {
			entry: entry({ fields: [field({ name: 'Title', kind: 'title', value: 'node-3' })] }),
			path: [group({ name: 'Work' })],
			versions: [],
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy: vi.fn(),
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose,
			onDelete,
			onFieldRemoved: vi.fn(),
			onFailure: vi.fn()
		}
	});
	flushSync();

	const header = host.querySelector('header');
	const buttons = [...(header?.querySelectorAll('button') ?? [])].map((each) =>
		each.getAttribute('aria-label')
	);
	expect(buttons, 'the close is not the one before the delete').toEqual([
		'Close this entry',
		'Delete this entry'
	]);

	header?.querySelector<HTMLButtonElement>('button[aria-label="Close this entry"]')?.click();
	flushSync();

	expect(onClose).toHaveBeenCalledTimes(1);
	expect(onDelete, 'closing the entry deleted it').not.toHaveBeenCalled();

	return unmount(component);
});

/** A database Coffer will not write back has nothing to delete with, and the
 * way out has to survive that: it is the pane's only button there. */
it('still offers the way out when there is nothing else in the header', () => {
	const onClose = vi.fn();
	const component = mount(EntryView, {
		target: host,
		props: {
			entry: entry({ fields: [field({ name: 'Title', kind: 'title', value: 'node-3' })] }),
			path: [group({ name: 'Work' })],
			versions: [],
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: true,
			onCopy: vi.fn(),
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose,
			onDelete: vi.fn(),
			onFieldRemoved: vi.fn(),
			onFailure: vi.fn()
		}
	});
	flushSync();

	const out = host.querySelector<HTMLButtonElement>('button[aria-label="Close this entry"]');
	expect(out, 'a read only entry cannot be put away').not.toBeNull();
	out?.click();
	expect(onClose).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/**
 * Closing the panel without choosing is not a change, and saying it is costs
 * the reader a key derivation, the re-encryption of every file in the vault,
 * and the oldest of the ten snapshots that lead back to yesterday.
 */
it('does not report a change when the file panel was closed without one', async () => {
	const onChanged = vi.fn();
	const onFailure = vi.fn();
	ipc.addAttachment.mockResolvedValue(null);

	const component = mount(EntryView, {
		target: host,
		props: {
			entry: entry({ attachments: [attachment({ name: 'id_ed25519' })] }),
			path: [group({ name: 'Work' })],
			versions: [],
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy: vi.fn(),
			onChanged,
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onFieldRemoved: vi.fn(),
			onFailure
		}
	});
	flushSync();

	host.querySelector<HTMLButtonElement>('[aria-label="Add a file"]')?.click();
	await vi.waitFor(() => expect(ipc.addAttachment).toHaveBeenCalled());

	expect(onChanged).not.toHaveBeenCalled();
	expect(onFailure).not.toHaveBeenCalled();

	return unmount(component);
});

const SCAN = 'Scanned Document.pdf';

/** What Rust answers when the entry already gives the chosen file's name to
 * another: a question, and nothing on the entry yet. */
function taken(over: Partial<Clash> = {}): Attached {
	return {
		outcome: 'taken',
		clash: {
			name: SCAN,
			size: 1.2 * 1024 * 1024,
			chosen: 840 * 1024,
			free: 'Scanned Document 2.pdf',
			...over
		}
	};
}

/** A pane whose entry has a scan, with the question about a second one open. */
async function asked(over: Partial<Clash> = {}) {
	ipc.addAttachment.mockReset();
	ipc.addAttachment.mockResolvedValue(taken(over));
	const name = over.name ?? SCAN;
	const opened = pane({ attachments: [attachment({ name, fileName: name })] });

	icon('Add a file').click();
	await vi.waitFor(() => expect(host.querySelector('[data-confirm]')).not.toBeNull());
	flushSync();
	return opened;
}

function question(): string {
	return host.querySelector('[data-confirm]')?.textContent ?? '';
}

/**
 * Every phone calls every scan the same thing, and the second page of a
 * passport used to take the place of the first without a word. Now nothing is
 * on the entry until the reader has been asked, in sizes as well as names, and
 * the answer that loses nothing is the one the focus is on.
 */
it('asks before a file goes on under a name the entry already has', async () => {
	const { component, onChanged, onFailure } = await asked();

	expect(question()).toContain(`This entry already has “${SCAN}” (1.2 MB).`);
	expect(question()).toContain(
		'Keep both to add the new one (840 KB) as “Scanned Document 2.pdf”.'
	);
	expect(question()).toContain('can’t be undone');

	// Asked, not reported, and nothing has changed to be written.
	expect(onChanged).not.toHaveBeenCalled();
	expect(onFailure).not.toHaveBeenCalled();

	// The way out, the answer that loses nothing, and the destructive one last
	// and the only one in red.
	const answers = [...host.querySelectorAll('[data-confirm] button')];
	expect(answers.map((each) => each.textContent?.trim())).toEqual([
		'Don’t add it',
		'Keep both',
		'Replace'
	]);
	expect(answers.filter((each) => each.className.includes('text-danger'))).toEqual([
		button('Replace')
	]);
	expect(document.activeElement).toBe(button('Keep both'));

	return unmount(component);
});

/** The default answer puts the file beside the one there, through Rust, and
 * the entry that comes back is the change the window writes. */
it('keeps both files on the answer that loses nothing', async () => {
	const both = entry({
		attachments: [attachment({ name: SCAN }), attachment({ name: 'Scanned Document 2.pdf' })]
	});
	ipc.keepBothAttachments.mockResolvedValue(both);
	const { component, entry: shown, onChanged } = await asked();

	button('Keep both').click();
	await vi.waitFor(() => expect(onChanged).toHaveBeenCalledWith(both));
	flushSync();

	expect(ipc.keepBothAttachments).toHaveBeenCalledWith(shown.id);
	expect(ipc.replaceAttachment).not.toHaveBeenCalled();
	expect(ipc.withdrawAttachment).not.toHaveBeenCalled();
	expect(host.querySelector('[data-confirm]')).toBeNull();

	return unmount(component);
});

it('replaces the file there only on the destructive answer', async () => {
	const replaced = entry({ attachments: [attachment({ name: SCAN })] });
	ipc.replaceAttachment.mockResolvedValue(replaced);
	const { component, entry: shown, onChanged } = await asked();

	button('Replace').click();
	await vi.waitFor(() => expect(onChanged).toHaveBeenCalledWith(replaced));

	expect(ipc.replaceAttachment).toHaveBeenCalledWith(shown.id);
	expect(ipc.keepBothAttachments).not.toHaveBeenCalled();

	return unmount(component);
});

/** The way out, by its button or by Escape, lets the file waiting in Rust go
 * and changes nothing. */
it('lets the file go when the reader does not add it', async () => {
	for (const way of ['button', 'escape']) {
		const { component, entry: shown, onChanged } = await asked();

		if (way === 'button') button('Don’t add it').click();
		else
			button('Keep both').dispatchEvent(
				new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })
			);
		flushSync();

		expect(host.querySelector('[data-confirm]'), way).toBeNull();
		expect(ipc.withdrawAttachment, way).toHaveBeenCalledWith(shown.id);
		expect(ipc.keepBothAttachments).not.toHaveBeenCalled();
		expect(ipc.replaceAttachment).not.toHaveBeenCalled();
		expect(onChanged).not.toHaveBeenCalled();

		unmount(component);
		ipc.withdrawAttachment.mockClear();
	}
});

/**
 * Replacing is a removal first, and earlier versions can hold the file there in
 * place the way they hold any other. That used to end in a sentence and a
 * button called "Right you are", with the chosen file already gone. The file is
 * still waiting now, so the question comes back without the answer that was
 * refused, saying why in words, and keeping both is still one press away.
 */
it('asks again without the refused answer when earlier versions hold the file', async () => {
	ipc.replaceAttachment.mockRejectedValue({
		code: 'attachmentInHistory',
		message: 'earlier versions of an entry still hold that file in place'
	});
	const both = entry({ attachments: [attachment({ name: SCAN })] });
	ipc.keepBothAttachments.mockResolvedValue(both);
	const { component, entry: shown, onChanged, onFailure } = await asked();

	button('Replace').click();
	await vi.waitFor(() => expect(question()).toContain('Earlier versions are holding'));
	flushSync();

	expect(question()).toContain(`“${SCAN}” that is here in place, so it can’t be replaced.`);
	expect(question()).toContain('its trash offers to clear the versions in the way');
	expect(host.textContent).not.toContain('Right you are');
	expect(onFailure).not.toHaveBeenCalled();
	expect(ipc.withdrawAttachment).not.toHaveBeenCalled();

	const answers = [...host.querySelectorAll('[data-confirm] button')];
	expect(answers.map((each) => each.textContent?.trim())).toEqual(['Don’t add it', 'Keep both']);
	expect(document.activeElement).toBe(button('Keep both'));

	button('Keep both').click();
	await vi.waitFor(() => expect(onChanged).toHaveBeenCalledWith(both));
	expect(ipc.keepBothAttachments).toHaveBeenCalledWith(shown.id);

	return unmount(component);
});

/**
 * The refusal says to take the file there off first, and its trash does that
 * without closing the question. Once it has gone there is nothing to keep both
 * of or to replace, and the question says so rather than offering either.
 */
it('offers the name itself once the file there has gone', async () => {
	ipc.replaceAttachment.mockRejectedValue({
		code: 'attachmentInHistory',
		message: 'earlier versions of an entry still hold that file in place'
	});
	const only = entry({ attachments: [attachment({ name: SCAN })] });
	ipc.keepBothAttachments.mockResolvedValue(only);
	ipc.addAttachment.mockReset();
	ipc.addAttachment.mockResolvedValue(taken());
	const first = entry({ attachments: [attachment({ name: SCAN })] });
	const props = reactive({
		entry: first,
		path: [group({ name: 'Work' })],
		versions: [],
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onFieldRemoved: vi.fn(),
		onFailure: vi.fn()
	});
	const component = mount(EntryView, { target: host, props });
	flushSync();

	icon('Add a file').click();
	await vi.waitFor(() => expect(host.querySelector('[data-confirm]')).not.toBeNull());
	button('Replace').click();
	await vi.waitFor(() => expect(question()).toContain('Earlier versions are holding'));

	// The file there, and the versions holding it, taken off by its own trash.
	props.entry = { ...first, attachments: [] };
	flushSync();

	expect(question()).toContain(`“${SCAN}” is no longer on this entry`);
	const answers = [...host.querySelectorAll('[data-confirm] button')];
	expect(answers.map((each) => each.textContent?.trim())).toEqual(['Don’t add it', 'Add it']);

	button('Add it').click();
	await vi.waitFor(() => expect(props.onChanged).toHaveBeenCalledWith(only));
	expect(ipc.keepBothAttachments).toHaveBeenCalledWith(first.id);
	expect(ipc.withdrawAttachment).not.toHaveBeenCalled();

	return unmount(component);
});

/** Any other refusal is an error to read, and the question goes - so the file
 * it was about goes too, rather than waiting in Rust on nobody. */
it('lets the file go when an answer is refused for any other reason', async () => {
	for (const answer of ['Replace', 'Keep both'] as const) {
		ipc.replaceAttachment.mockRejectedValue({ code: 'io', message: 'the disk is full' });
		ipc.keepBothAttachments.mockRejectedValue({ code: 'io', message: 'the disk is full' });
		const { component, entry: shown, onFailure, onChanged } = await asked();

		button(answer).click();
		await vi.waitFor(() =>
			expect(onFailure).toHaveBeenCalledWith(expect.objectContaining({ code: 'io' }))
		);
		await vi.waitFor(() => expect(ipc.withdrawAttachment).toHaveBeenCalledWith(shown.id));
		flushSync();

		expect(host.querySelector('[data-confirm]'), answer).toBeNull();
		expect(onChanged).not.toHaveBeenCalled();

		unmount(component);
		ipc.withdrawAttachment.mockClear();
	}
});

/**
 * The pane is handed one entry after another. A question about the entry that
 * was open goes with it, and the file it was about is let go by that entry's
 * id - never answered against the next one.
 */
it('lets the file go when the pane shows another entry or none', async () => {
	ipc.addAttachment.mockReset();
	ipc.addAttachment.mockResolvedValue(taken());
	const first = entry({ attachments: [attachment({ name: SCAN })] });
	const props = reactive({
		entry: first,
		path: [group({ name: 'Work' })],
		versions: [],
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onFieldRemoved: vi.fn(),
		onFailure: vi.fn()
	});
	const component = mount(EntryView, { target: host, props });
	flushSync();

	icon('Add a file').click();
	await vi.waitFor(() => expect(host.querySelector('[data-confirm]')).not.toBeNull());

	props.entry = entry({ attachments: [attachment({ name: SCAN })] });
	flushSync();

	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(ipc.withdrawAttachment).toHaveBeenCalledTimes(1);
	expect(ipc.withdrawAttachment).toHaveBeenCalledWith(first.id);

	// And a pane that goes away altogether, which is how a lock and a close
	// both look from here.
	icon('Add a file').click();
	await vi.waitFor(() => expect(host.querySelector('[data-confirm]')).not.toBeNull());
	const second = props.entry.id;
	unmount(component);
	expect(ipc.withdrawAttachment).toHaveBeenLastCalledWith(second);
	expect(ipc.keepBothAttachments).not.toHaveBeenCalled();
	expect(ipc.replaceAttachment).not.toHaveBeenCalled();
});

/**
 * The same entry drawn again after a change - a title written while the
 * question was open - is still the entry the question is about. The pane used
 * to take every answer it was waiting on away with any change at all, and the
 * reader's file with it.
 */
it('keeps the question while the entry it is about changes', async () => {
	ipc.addAttachment.mockReset();
	ipc.addAttachment.mockResolvedValue(taken());
	const first = entry({ attachments: [attachment({ name: SCAN })] });
	const props = reactive({
		entry: first,
		path: [group({ name: 'Work' })],
		versions: [],
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onFieldRemoved: vi.fn(),
		onFailure: vi.fn()
	});
	const component = mount(EntryView, { target: host, props });
	flushSync();

	icon('Add a file').click();
	await vi.waitFor(() => expect(host.querySelector('[data-confirm]')).not.toBeNull());

	props.entry = { ...first, fields: [field({ name: 'Title', value: 'Passport', empty: false })] };
	flushSync();

	expect(question()).toContain(`This entry already has “${SCAN}”`);
	expect(ipc.withdrawAttachment).not.toHaveBeenCalled();

	return unmount(component);
});

/** An answer that arrives for an entry the pane has left is not drawn over
 * the next one, and the file it was about is let go. */
it('asks nothing when the pane has moved on by the time the answer comes', async () => {
	let answer: (value: Attached) => void = () => {};
	ipc.addAttachment.mockReset();
	ipc.addAttachment.mockReturnValue(new Promise<Attached>((resolve) => (answer = resolve)));
	const first = entry({ attachments: [attachment({ name: SCAN })] });
	const props = reactive({
		entry: first,
		path: [group({ name: 'Work' })],
		versions: [],
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onFieldRemoved: vi.fn(),
		onFailure: vi.fn()
	});
	const component = mount(EntryView, { target: host, props });
	flushSync();

	icon('Add a file').click();
	props.entry = entry({ attachments: [attachment({ name: SCAN })] });
	flushSync();
	answer(taken());

	await vi.waitFor(() => expect(ipc.withdrawAttachment).toHaveBeenCalledWith(first.id));
	flushSync();
	expect(host.querySelector('[data-confirm]')).toBeNull();

	return unmount(component);
});

/** A file whose name is free is simply on the entry, and the entry that comes
 * back is the change the window writes. Pressing the button again takes away a
 * question still open from the last press. */
it('puts a file whose name is free straight on, and a new press starts over', async () => {
	const { component, onChanged } = await asked();

	const added = entry({ attachments: [attachment({ name: 'passport-2.pdf' })] });
	ipc.addAttachment.mockResolvedValue({ outcome: 'added', entry: added });
	icon('Add a file').click();
	flushSync();
	expect(host.querySelector('[data-confirm]')).toBeNull();

	await vi.waitFor(() => expect(onChanged).toHaveBeenCalledWith(added));
	expect(host.querySelector('[data-confirm]')).toBeNull();

	return unmount(component);
});

/** The names in the question are out of somebody's database and off their
 * disk, and they are written as text whatever they hold. */
it('writes the names in the question as text and never as markup', async () => {
	const hostile = '<img src=x onerror="alert(1)">.pdf';
	const { component } = await asked({ name: hostile, free: '<script>alert(1)</script> 2.pdf' });

	expect(host.querySelector('img')).toBeNull();
	expect(host.querySelector('script')).toBeNull();
	expect(question()).toContain(hostile);
	expect(question()).toContain('<script>alert(1)</script> 2.pdf');

	return unmount(component);
});

/** The row for a field of the reader's own that the database protects. */
function own(label: string): HTMLInputElement {
	const found = host.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`);
	if (!found) throw new Error(`no field labelled ${label}`);
	return found;
}

function icon(label: string): HTMLButtonElement {
	const found = host.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
	if (!found) throw new Error(`no control labelled ${label}`);
	return found;
}

const TOKEN = 'sk-live-9f3a2b';

/**
 * A protected field of the reader's own used to be a mask and an eye and
 * nothing else: no way to change the value, and no way to copy it but to reveal
 * it and press Cmd+C - an ordinary pasteboard write with none of the markers
 * Coffer's own copy carries, which is how a secret lands in Maccy.
 */
it('copies a protected own field through rust rather than off the screen', async () => {
	const onCopy = vi.fn();
	const component = mount(EntryView, {
		target: host,
		props: {
			entry: entry({
				fields: [
					field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false })
				]
			}),
			path: [group({ name: 'Work' })],
			versions: [],
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy,
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onFieldRemoved: vi.fn(),
			onFailure: vi.fn()
		}
	});
	flushSync();

	icon('Copy API token').click();
	expect(onCopy).toHaveBeenCalledTimes(1);
	// By name, so the value is fetched, written to the pasteboard and forgotten
	// inside Rust. Nothing of it crosses the boundary.
	expect(onCopy.mock.calls[0][1]).toBe('API token');
	expect(screen()).not.toContain(TOKEN);

	return unmount(component);
});

/**
 * Showing a value is not an edit. Writing one back costs a key derivation, a
 * rewrite of the whole file and one of the ten snapshots beside it, so a reader
 * who looked at a token must not have spent a recovery point on it.
 */
it('writes a protected own field back only when the reader typed in it', async () => {
	ipc.reveal.mockResolvedValue(TOKEN);
	const onChanged = vi.fn();
	const component = mount(EntryView, {
		target: host,
		props: {
			entry: entry({
				fields: [
					field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false })
				]
			}),
			path: [group({ name: 'Work' })],
			versions: [],
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy: vi.fn(),
			onChanged,
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onFieldRemoved: vi.fn(),
			onFailure: vi.fn()
		}
	});
	flushSync();

	icon('Show API token').click();
	await vi.waitFor(() => expect(own('API token').value).toBe(TOKEN));
	flushSync();

	// On the screen, and only there.
	expect(attributes().join(' ')).not.toContain(TOKEN);
	expect(host.innerHTML).not.toContain(TOKEN);

	// Looked at and put away: nothing is written.
	own('API token').dispatchEvent(new FocusEvent('blur'));
	await vi.waitFor(() => expect(own('API token').value).toBe(''));
	expect(ipc.setField).not.toHaveBeenCalled();

	// Typed in: written, with the protection it arrived with.
	icon('Show API token').click();
	await vi.waitFor(() => expect(own('API token').value).toBe(TOKEN));
	own('API token').value = 'sk-live-rotated';
	own('API token').dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
	own('API token').dispatchEvent(new FocusEvent('blur'));
	flushSync();

	await vi.waitFor(() => expect(ipc.setField).toHaveBeenCalledTimes(1));
	expect(ipc.setField).toHaveBeenCalledWith(
		expect.any(String),
		'API token',
		'sk-live-rotated',
		true
	);

	return unmount(component);
});

/**
 * Escape closes the field, and the blur that follows the input going hidden
 * must not then write an empty value over a real one. The two events arrive in
 * that order, and only `Secret` knows the field is no longer the reader's.
 */
it('writes nothing when the reader escapes out of a protected own field', async () => {
	ipc.reveal.mockResolvedValue(TOKEN);
	const component = mount(EntryView, {
		target: host,
		props: {
			entry: entry({
				fields: [
					field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false })
				]
			}),
			path: [group({ name: 'Work' })],
			versions: [],
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy: vi.fn(),
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onFieldRemoved: vi.fn(),
			onFailure: vi.fn()
		}
	});
	flushSync();

	icon('Show API token').click();
	await vi.waitFor(() => expect(own('API token').value).toBe(TOKEN));
	own('API token').value = 'half a to';
	own('API token').dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
	own('API token').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
	flushSync();
	own('API token').dispatchEvent(new FocusEvent('blur'));
	flushSync();

	expect(ipc.setField).not.toHaveBeenCalled();
	expect(screen()).not.toContain('half a to');

	return unmount(component);
});

/** A database Coffer cannot write back still shows and copies. It just does not
 * offer to change anything. */
it('offers a look and a copy of a protected own field it cannot write', async () => {
	ipc.reveal.mockResolvedValue(TOKEN);
	const component = mount(EntryView, {
		target: host,
		props: {
			entry: entry({
				fields: [
					field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false })
				]
			}),
			path: [group({ name: 'Work' })],
			versions: [],
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: true,
			onCopy: vi.fn(),
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onFieldRemoved: vi.fn(),
			onFailure: vi.fn()
		}
	});
	flushSync();

	expect(own('API token').readOnly).toBe(true);
	icon('Show API token').click();
	await vi.waitFor(() => expect(own('API token').value).toBe(TOKEN));

	own('API token').value = 'typed anyway';
	own('API token').dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
	own('API token').dispatchEvent(new FocusEvent('blur'));
	flushSync();
	expect(ipc.setField).not.toHaveBeenCalled();

	return unmount(component);
});

/** The pane with its callbacks as mocks the test can read. */
function pane(over: Parameters<typeof entry>[0], readOnly = false) {
	const props = {
		entry: entry(over),
		path: [group({ name: 'Work' })],
		versions: [],
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onFieldRemoved: vi.fn(),
		onFailure: vi.fn()
	};
	const component = mount(EntryView, { target: host, props });
	flushSync();
	return { component, ...props };
}

/**
 * A file is the one thing on an entry nothing brings back: files are not kept
 * in an entry's versions and the vault is written the moment one goes. The
 * trash used to sit beside the export at the same size and take the file on
 * one press. It asks now, in the file's own row, and says what it is asking
 * about in size as well as by name.
 */
it("asks in the file's row before a file is removed", async () => {
	ipc.removeAttachment.mockReset();
	ipc.removeAttachment.mockResolvedValue(entry());
	const { component, onChanged } = pane({
		attachments: [
			attachment({ name: 'passport.pdf', fileName: 'passport.pdf', size: 1.2 * 1024 * 1024 }),
			attachment({ name: 'id_ed25519' })
		]
	});

	icon('Remove passport.pdf').click();
	flushSync();

	const asked = host.querySelector('[data-confirm]');
	expect(asked?.textContent).toContain(
		'Remove passport.pdf (1.2 MB)? Files are not kept in Versions, so this can’t be undone.'
	);
	// In the row it is about, and only there.
	expect(asked?.closest('.bg-surface2')?.textContent).toContain('passport.pdf');
	expect(asked?.closest('.bg-surface2')?.textContent).not.toContain('id_ed25519');
	expect(ipc.removeAttachment).not.toHaveBeenCalled();

	button('Keep it').click();
	flushSync();
	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(ipc.removeAttachment).not.toHaveBeenCalled();

	icon('Remove passport.pdf').click();
	flushSync();
	button('Remove').click();
	await vi.waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));
	expect(ipc.removeAttachment).toHaveBeenCalledTimes(1);
	expect(ipc.removeAttachment).toHaveBeenCalledWith(expect.any(String), 'passport.pdf');
	expect(host.querySelector('[data-confirm]')).toBeNull();

	return unmount(component);
});

/**
 * The way out of losing a file for good is to have a copy of it. Saving one
 * runs the same export the row offers, and the question stays where it is: the
 * reader chose to keep a copy, not to change their mind.
 */
it('saves a copy first and leaves the question open', async () => {
	ipc.removeAttachment.mockReset();
	ipc.removeAttachment.mockResolvedValue(entry());
	ipc.exportAttachment.mockResolvedValue(undefined);
	const { component } = pane({
		attachments: [attachment({ name: '../scan.pdf', fileName: 'scan.pdf', size: 2048 })]
	});

	icon('Remove ../scan.pdf').click();
	flushSync();
	button('Save a copy first…').click();
	await vi.waitFor(() => expect(ipc.exportAttachment).toHaveBeenCalledTimes(1));
	// By the name the database holds; Rust turns it into a safe one for the panel.
	expect(ipc.exportAttachment).toHaveBeenCalledWith(expect.any(String), '../scan.pdf');
	flushSync();

	expect(host.querySelector('[data-confirm]')?.textContent).toContain(
		'Remove ../scan.pdf (2.0 KB)?'
	);
	expect(ipc.removeAttachment).not.toHaveBeenCalled();

	button('Remove').click();
	await vi.waitFor(() => expect(ipc.removeAttachment).toHaveBeenCalledTimes(1));

	return unmount(component);
});

/** A save panel the reader could not use is a failure like any other, and the
 * file is still there to be asked about. */
it('keeps the question when the copy could not be saved', async () => {
	ipc.exportAttachment.mockRejectedValue({ code: 'io', message: 'the disk is full' });
	const { component, onFailure } = pane({ attachments: [attachment({ name: 'id_ed25519' })] });

	icon('Remove id_ed25519').click();
	flushSync();
	button('Save a copy first…').click();
	await vi.waitFor(() =>
		expect(onFailure).toHaveBeenCalledWith(expect.objectContaining({ code: 'io' }))
	);
	flushSync();

	expect(host.querySelector('[data-confirm]')).not.toBeNull();
	expect(ipc.removeAttachment).not.toHaveBeenCalled();

	return unmount(component);
});

/** The pane is handed one entry after another. A question about a file on the
 * entry that was open would otherwise be answered against the next one. */
it('asks nothing about a file once another entry is open', () => {
	const props = reactive({
		entry: entry({ attachments: [attachment({ name: 'id_ed25519' })] }),
		path: [group({ name: 'Work' })],
		versions: [],
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onFieldRemoved: vi.fn(),
		onFailure: vi.fn()
	});
	const component = mount(EntryView, { target: host, props });
	flushSync();

	icon('Remove id_ed25519').click();
	flushSync();
	expect(host.querySelector('[data-confirm]')).not.toBeNull();

	props.entry = entry({ attachments: [attachment({ name: 'id_ed25519' })] });
	flushSync();
	expect(host.querySelector('[data-confirm]')).toBeNull();

	return unmount(component);
});

/**
 * The export and the trash were two sixteen-pixel icons twelve pixels apart,
 * and a press meant for one landed on the other. Each is now a box a fingertip
 * wide, and the trash stands a further gap away.
 */
it('gives the file buttons room, and keeps the trash apart from the export', () => {
	const { component } = pane({ attachments: [attachment({ name: 'id_ed25519' })] });

	const out = icon('Write id_ed25519 out');
	const trash = icon('Remove id_ed25519');
	for (const each of [out, trash]) {
		expect(each.className).toContain('h-7');
		expect(each.className).toContain('w-7');
	}
	expect(out.nextElementSibling).toBe(trash);
	expect(trash.className).toContain('ml-3');

	return unmount(component);
});

/**
 * Removing a field of the reader's own is a change like any other, and the
 * window is told which field on which entry once the change has been handed
 * over - after, because what it offers back depends on what the save did.
 */
it('tells the window which field came off, once the change is in', async () => {
	let settle: () => void = () => {};
	const onChanged = vi.fn(() => new Promise<void>((resolve) => (settle = resolve)));
	ipc.removeField.mockResolvedValue(entry());
	const props = {
		entry: entry({
			fields: [field({ name: 'PIN', kind: 'custom', protected: true, value: null, empty: false })]
		}),
		path: [group({ name: 'Work' })],
		versions: [],
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged,
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onFieldRemoved: vi.fn(),
		onFailure: vi.fn()
	};
	const component = mount(EntryView, { target: host, props });
	flushSync();

	icon('Remove the field PIN').click();
	await vi.waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));
	expect(ipc.removeField).toHaveBeenCalledWith(props.entry.id, 'PIN');
	expect(props.onFieldRemoved, 'the window heard before the save was done').not.toHaveBeenCalled();

	settle();
	await vi.waitFor(() => expect(props.onFieldRemoved).toHaveBeenCalledWith(props.entry.id, 'PIN'));

	return unmount(component);
});

/** A removal the vault refused is nothing to offer back. */
it('says nothing came off when the removal was refused', async () => {
	ipc.removeField.mockRejectedValue({ code: 'readOnly', message: 'this database is read only' });
	const { component, onFieldRemoved, onFailure } = pane({
		fields: [field({ name: 'PIN', kind: 'custom', value: '1234', empty: false })]
	});

	icon('Remove the field PIN').click();
	await vi.waitFor(() => expect(onFailure).toHaveBeenCalled());
	expect(onFieldRemoved).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * The trash on a field's row sat twelve pixels from Copy at the same size, on
 * every row, and a press meant for the value took the field. It is the last
 * thing in the row now, a fingertip wide and a step further off, and it is
 * there only while the row is under the pointer or holds the keyboard's focus.
 */
it("keeps a field's trash at the far end of its row, out of sight until it is wanted", () => {
	const { component } = pane({
		fields: [
			field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false }),
			field({ name: 'Port', kind: 'custom', value: '2202', empty: false })
		]
	});

	for (const name of ['API token', 'Port']) {
		const trash = icon(`Remove the field ${name}`);
		const row = trash.parentElement;
		expect(row?.lastElementChild, `the trash is not the last thing on the ${name} row`).toBe(trash);
		expect(row?.className).toContain('group');
		for (const rule of [
			'opacity-0',
			'group-hover:opacity-100',
			'group-focus-within:opacity-100',
			'h-7',
			'w-7',
			'ml-2'
		]) {
			expect(trash.className, `${name}: ${rule}`).toContain(rule);
		}
	}
	// On the protected row, the button before the trash is the copy.
	expect(icon('Remove the field API token').previousElementSibling).toBe(icon('Copy API token'));

	return unmount(component);
});
