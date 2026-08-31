import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { attachment, entry, field, group } from '$lib/fixtures';
import { reactive } from '$lib/props.svelte';
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
			readOnly: false,
			onCopy: vi.fn(),
			onChanged,
			onVersions: vi.fn(),
			onClose: vi.fn(),
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
			onFailure
		}
	});
	flushSync();

	host.querySelector<HTMLButtonElement>('[aria-label="Remove id_ed25519"]')?.click();
	await vi.waitFor(() => expect(host.textContent).toContain('2 earlier versions still hold'));
	flushSync();

	// A refusal the reader can act on is not an error message.
	expect(onFailure).not.toHaveBeenCalled();
	expect(onChanged).not.toHaveBeenCalled();

	// One call, not two: the versions go only if the file then goes.
	button('Clear the versions and remove it').click();
	await vi.waitFor(() => expect(ipc.removeAttachmentAndVersions).toHaveBeenCalledTimes(1));
	await vi.waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));
	expect(onVersions).toHaveBeenCalledWith([]);

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
		onFailure: vi.fn()
	});
	const component = mount(EntryView, { target: host, props });
	flushSync();

	host.querySelector<HTMLButtonElement>('[aria-label="Remove id_ed25519"]')?.click();
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
