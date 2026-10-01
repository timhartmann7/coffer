import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { attachment, entry, field, group } from '$lib/fixtures';
import { typing } from '$lib/keys';
import type { Attached, Clash } from '$lib/model';
import { reactive } from '$lib/props.svelte';
import EntryView from './EntryView.svelte';

const ipc = vi.hoisted(() => ({
	reveal: vi.fn(),
	openUrl: vi.fn(),
	setField: vi.fn(),
	draft: vi.fn(),
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
	ipc.draft.mockResolvedValue(undefined);
});

afterEach(() => {
	host.remove();
	localStorage.clear();
	// A selection one test made is still standing in the next one otherwise,
	// and a stand-in for the selection has to go before it can be cleared.
	vi.restoreAllMocks();
	document.getSelection()?.removeAllRanges();
});

function show(entryOver: Parameters<typeof entry>[0]) {
	return mount(EntryView, {
		target: host,
		props: {
			entry: entry(entryOver),
			root: group({ name: 'Root' }),
			path: [group({ name: 'Work' })],
			history: null,
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy: vi.fn(),
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onPutBack: vi.fn(),
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

/** What the node showing a secret has in it. */
function value(): string {
	const found = host.querySelector('[data-value]');
	if (!found) throw new Error('the pane has nowhere to show a value');
	return found.textContent ?? '';
}

/** Everything in the fields of the pane that can be typed into. */
function written(): string {
	return [...host.querySelectorAll('input, textarea')]
		.map((each) => (each as HTMLInputElement | HTMLTextAreaElement).value)
		.join(' ');
}

/** What a reader typing does: the value changes, and the field hears it. */
function enter(into: HTMLInputElement | HTMLTextAreaElement, typed: string) {
	into.value = typed;
	into.dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
}

/** The field a new value is written in, found by what it is called. */
function changer(label: string): HTMLTextAreaElement {
	const found = host.querySelector<HTMLTextAreaElement>(`textarea[aria-label="${label}"]`);
	if (!found) throw new Error(`no field for ${label}`);
	return found;
}

/** The focus leaving a field for somewhere outside it, with the two events a
 * browser sends for that. */
function leave(from: HTMLElement, to: EventTarget | null = null) {
	from.dispatchEvent(new FocusEvent('blur', { relatedTarget: to }));
	from.dispatchEvent(new FocusEvent('focusout', { bubbles: true, relatedTarget: to }));
	flushSync();
}

/** Selects part of a revealed value, the way a drag across it would. */
function choose(node: HTMLElement, from: number, to: number) {
	const text = node.firstChild;
	if (!text) throw new Error('nothing is shown to select');
	const range = document.createRange();
	range.setStart(text, from);
	range.setEnd(text, to);
	const selection = document.getSelection();
	selection?.removeAllRanges();
	selection?.addRange(range);
}

/** What the system's Copy or Cut menu item fires at the node holding the
 * selection. */
function clipboard(node: HTMLElement, kind: 'copy' | 'cut'): ClipboardEvent {
	const event = new ClipboardEvent(kind, { bubbles: true, cancelable: true });
	node.dispatchEvent(event);
	return event;
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

	// On the screen, and only there: the one text node of the one element a
	// value is written into. Not in an attribute, not in anything that can be
	// typed into, and nowhere a crash could write it down.
	const node = host.querySelector('[data-value]');
	expect(node?.childNodes).toHaveLength(1);
	expect(host.innerHTML.split(SECRET), 'the value is in the markup twice').toHaveLength(2);
	expect(attributes().join(' ')).not.toContain(SECRET);
	expect(written()).not.toContain(SECRET);
	expect(JSON.stringify(localStorage)).not.toContain(SECRET);
	expect(JSON.stringify(sessionStorage)).not.toContain(SECRET);
	expect(window.location.href).not.toContain(SECRET);
	expect(document.title).not.toContain(SECRET);

	button('Hide').click();
	flushSync();
	expect(screen()).not.toContain(SECRET);
	expect(host.innerHTML).not.toContain(SECRET);

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
 * Showing is reading. The value used to be revealed into the field it was
 * edited in, with the focus put there, so a guest being read the Wi-Fi
 * password out watched one stray key change it and the next click anywhere
 * save it. The value is text now, nothing that takes a key appears, and the
 * focus goes to the button that hides it again.
 */
it('focuses nothing editable on Show, so a stray key changes nothing', async () => {
	const component = show({
		fields: [
			field({ name: 'Title', kind: 'title', value: 'Home Wi-Fi', empty: false }),
			field({ name: 'Password', kind: 'password', value: null, empty: false })
		]
	});
	flushSync();
	const fields = host.querySelectorAll('input, textarea').length;

	button('Show').click();
	await vi.waitFor(() => expect(value()).toBe(SECRET));
	flushSync();

	expect(host.querySelector('[data-value]')?.tagName).toBe('SPAN');
	expect(host.querySelectorAll('input, textarea'), 'Show opened a field').toHaveLength(fields);
	expect(document.activeElement, 'the focus is not on the way to hide it').toBe(button('Hide'));
	expect(typing(document.activeElement)).toBe(false);

	// The keys a reader leans on while a password is being read out, and the
	// focus going somewhere else afterwards.
	for (const key of [' ', 'a', 'Backspace', 'Delete', 'v']) {
		document.activeElement?.dispatchEvent(
			new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })
		);
	}
	button('Hide').dispatchEvent(new FocusEvent('blur'));
	flushSync();

	expect(ipc.setField, 'looking at a password wrote it').not.toHaveBeenCalled();
	expect(value(), 'the value on the screen changed under the keys').toBe(SECRET);
	expect(host.textContent).toContain('Hides in');

	return unmount(component);
});

/** Changing is a press of its own, and only Save or Return writes. */
it('writes a new password on Save or Return, and nothing on Cancel or Escape', async () => {
	ipc.setField.mockResolvedValue(entry());
	const { component } = pane({
		fields: [
			field({ name: 'Password', kind: 'password', value: null, empty: false, protected: true })
		]
	});
	const elsewhere = vi.fn();
	window.addEventListener('keydown', elsewhere);

	try {
		button('Change').click();
		flushSync();
		expect(document.activeElement, 'Change did not put the reader in the new field').toBe(
			changer('New password')
		);
		enter(changer('New password'), 'a different one');
		button('Cancel').click();
		flushSync();
		expect(host.querySelector('[aria-label="New password"]')).toBeNull();

		button('Change').click();
		flushSync();
		const escaped = changer('New password');
		enter(escaped, 'a different one');
		escaped.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
		flushSync();
		expect(host.querySelector('[aria-label="New password"]')).toBeNull();
		expect(escaped.value, 'what was typed outlived the field').toBe('');
		expect(elsewhere, 'the Escape closed the entry as well').not.toHaveBeenCalled();

		expect(ipc.setField).not.toHaveBeenCalled();
		expect(written()).not.toContain('a different one');

		button('Change').click();
		flushSync();
		enter(changer('New password'), 'a different one');
		button('Save').click();
		await vi.waitFor(() =>
			expect(ipc.setField).toHaveBeenCalledWith(
				expect.any(String),
				'Password',
				'a different one',
				true,
				expect.any(Number)
			)
		);
		await vi.waitFor(() => expect(host.querySelector('[aria-label="New password"]')).toBeNull());

		button('Change').click();
		flushSync();
		enter(changer('New password'), 'a third one');
		const returned = new KeyboardEvent('keydown', {
			key: 'Enter',
			bubbles: true,
			cancelable: true
		});
		changer('New password').dispatchEvent(returned);
		expect(returned.defaultPrevented).toBe(true);
		await vi.waitFor(() =>
			expect(ipc.setField).toHaveBeenLastCalledWith(
				expect.any(String),
				'Password',
				'a third one',
				true,
				expect.any(Number)
			)
		);
		expect(ipc.setField).toHaveBeenCalledTimes(2);
	} finally {
		window.removeEventListener('keydown', elsewhere);
	}

	return unmount(component);
});

/**
 * Leaving the new password with something typed in it asks, in the row. The
 * click that took the focus away could have been meant for anything, so it
 * decides nothing: not writing half a password over the real one, and not
 * throwing away one the reader has just set on a website. The question leaves
 * the focus where the reader put it.
 */
it('asks about a new password left half way, and does what the answer says', async () => {
	ipc.setField.mockResolvedValue(entry());
	const { component } = pane({
		fields: [
			field({ name: 'Password', kind: 'password', value: null, empty: false, protected: true })
		]
	});

	button('Change').click();
	flushSync();
	const typed = changer('New password');
	enter(typed, 'half a pass');

	// A Tab onto its own Cancel is not leaving it; a Tab on out of there is.
	const cancel = button('Cancel');
	leave(typed, cancel);
	expect(host.textContent).not.toContain('Save the new password?');
	leave(cancel);

	expect(ipc.setField, 'leaving wrote it').not.toHaveBeenCalled();
	expect(host.textContent).toContain('Save the new password?');
	expect(typed.value, 'leaving threw it away').toBe('half a pass');
	expect(document.activeElement, 'the question took the focus').not.toBe(button('Discard'));
	expect(document.activeElement, 'the question took the focus').not.toBe(button('Save'));

	// Going back into the field is going on with it.
	typed.dispatchEvent(new FocusEvent('focus'));
	flushSync();
	expect(host.textContent).not.toContain('Save the new password?');

	leave(typed);
	button('Discard').click();
	flushSync();
	expect(ipc.setField).not.toHaveBeenCalled();
	expect(host.querySelector('[aria-label="New password"]')).toBeNull();
	expect(typed.value, 'a discarded password stayed in its field').toBe('');

	button('Change').click();
	flushSync();
	enter(changer('New password'), 'the whole new one');
	leave(changer('New password'));
	button('Save').click();
	await vi.waitFor(() =>
		expect(ipc.setField).toHaveBeenCalledWith(
			expect.any(String),
			'Password',
			'the whole new one',
			true,
			expect.any(Number)
		)
	);

	return unmount(component);
});

/** A field opened and left with nothing in it is put away, with nothing to
 * ask about; and so is one left while the window was only behind another. */
it('asks nothing about a new password nobody typed, or when the window went to the back', () => {
	const { component } = pane({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});

	button('Change').click();
	flushSync();
	leave(changer('New password'));
	expect(host.querySelector('[aria-label="New password"]')).toBeNull();
	expect(host.textContent).not.toContain('Save the new password?');

	button('Change').click();
	flushSync();
	enter(changer('New password'), 'mid-word');
	vi.spyOn(document, 'hasFocus').mockReturnValue(false);
	leave(changer('New password'));
	expect(host.textContent).not.toContain('Save the new password?');
	expect(changer('New password').value).toBe('mid-word');
	expect(ipc.setField).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * A new password half typed for an entry the pane no longer shows has nobody
 * left to ask. It is not written into the file unasked, and it does not go
 * quietly either: the window says it was not saved.
 */
it('says so when a new password half typed goes with its entry', () => {
	const { component, props } = deleting({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});

	button('Change').click();
	flushSync();
	const typed = changer('New password');
	enter(typed, 'half');
	props.entry = entry({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	flushSync();

	expect(props.onFailure).toHaveBeenCalledWith(
		expect.objectContaining({ message: 'The new password was not saved.' })
	);
	expect(ipc.setField).not.toHaveBeenCalled();
	expect(typed.value, 'the half typed password outlived its field').toBe('');
	expect(host.querySelector('[aria-label="New password"]')).toBeNull();

	// Nothing typed is nothing to report.
	button('Change').click();
	flushSync();
	props.entry = entry({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	flushSync();
	expect(props.onFailure).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/**
 * The generator's insert is an answer of its own and writes straight away.
 * What it must not do is leave a field behind with a password in it: a new
 * one half typed by hand beside it is put away as well, because the made one
 * was chosen over it.
 */
it('puts a made password straight in and leaves no field open behind it', async () => {
	ipc.generatePassword.mockResolvedValue('Made-Password-123');
	ipc.setField.mockResolvedValue(entry());
	const { component, onFailure } = pane({
		fields: [
			field({ name: 'Password', kind: 'password', value: null, empty: false, protected: true })
		]
	});

	button('Change').click();
	flushSync();
	enter(changer('New password'), 'by hand');
	button('Make one').click();
	flushSync();
	await vi.waitFor(() => expect(button('Put it in the field').disabled).toBe(false));
	button('Put it in the field').click();
	flushSync();

	await vi.waitFor(() =>
		expect(ipc.setField).toHaveBeenCalledWith(
			expect.any(String),
			'Password',
			'Made-Password-123',
			true,
			expect.any(Number)
		)
	);
	expect(ipc.setField).toHaveBeenCalledTimes(1);
	expect(host.querySelector('[aria-label="New password"]')).toBeNull();
	expect(written()).not.toContain('Made-Password-123');
	expect(written()).not.toContain('by hand');
	expect(onFailure).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * Nothing typed is no new password. A field opened and left alone, or typed in
 * and emptied again, offers no Save and takes Return as nothing - and what was
 * typed before it was emptied is taken back from Rust, so a lock does not
 * write the empty field over the password, or keep anything beside it. Leaving
 * the same empty field used to write nothing while Save and Return wrote an
 * empty password over the real one.
 */
it('writes nothing, and keeps nothing for a lock, from a new password field left empty', async () => {
	vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
	const component = show({
		fields: [
			field({ name: 'Password', kind: 'password', value: null, empty: false, protected: true })
		]
	});
	try {
		flushSync();
		button('Change').click();
		flushSync();
		expect(button('Save').disabled, 'Save was offered for nothing').toBe(true);

		const typed = changer('New password');
		enter(typed, 'the wrong one');
		expect(button('Save').disabled).toBe(false);
		vi.advanceTimersByTime(250);
		expect(drafted()).toEqual([['Password', 'the wrong one', true, expect.any(Number)]]);

		// Cmd+A and Delete, and off to fetch the right one.
		enter(typed, '');
		expect(button('Save').disabled, 'Save was offered for an emptied field').toBe(true);
		expect(markOf(typed), 'an emptied field says it is not saved').toBeNull();
		expect(drafted().at(-1)?.slice(0, 2), 'Rust still holds the wrong one').toEqual([
			'Password',
			null
		]);
		vi.advanceTimersByTime(10_000);
		expect(drafted().at(-1)?.slice(0, 2), 'the empty field was told as a draft').toEqual([
			'Password',
			null
		]);

		const returned = new KeyboardEvent('keydown', {
			key: 'Enter',
			bubbles: true,
			cancelable: true
		});
		typed.dispatchEvent(returned);
		button('Save').click();
		await vi.advanceTimersByTimeAsync(0);
		flushSync();

		expect(returned.defaultPrevented, 'Return put a line into the empty field').toBe(true);
		expect(ipc.setField).not.toHaveBeenCalled();
		expect(host.querySelector('[aria-label="New password"]'), 'Return closed it').not.toBeNull();
	} finally {
		vi.useRealTimers();
		unmount(component);
	}
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

	expect(target.textContent).toBe('');
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
		expect(target.textContent).toBe(SECRET);

		vi.advanceTimersByTime(29_000);
		flushSync();
		expect(target.textContent).toBe(SECRET);

		vi.advanceTimersByTime(1_000);
		flushSync();
		expect(target.textContent).toBe('');
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
			root: group({ name: 'Root' }),
			path: [group({ name: 'Work' })],
			history: null,
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy: vi.fn(),
			onChanged,
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onPutBack: vi.fn(),
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
 * The pane sends back the protection a field arrived with. The screen cannot
 * work that out from the value, because a protected value never crosses: it
 * reads it off the field it was given and sends it back unchanged.
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
	enter(login, 'bob');
	login.dispatchEvent(new Event('blur'));

	expect(ipc.setField).toHaveBeenCalledWith(
		expect.any(String),
		'UserName',
		'bob',
		false,
		expect.any(Number)
	);

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

	const plain = host.querySelector('[aria-label="Region"]') as HTMLTextAreaElement;
	enter(plain, 'eu-west');
	plain.dispatchEvent(new Event('blur'));
	expect(ipc.setField).toHaveBeenCalledWith(
		expect.any(String),
		'Region',
		'eu-west',
		false,
		expect.any(Number)
	);

	const kept = host.querySelector('[aria-label="API token"]') as HTMLTextAreaElement;
	enter(kept, 'a new token');
	kept.dispatchEvent(new Event('blur'));
	expect(ipc.setField).toHaveBeenCalledWith(
		expect.any(String),
		'API token',
		'a new token',
		true,
		expect.any(Number)
	);

	return unmount(component);
});

/**
 * The field of the reader's own that went wrong: ten recovery codes, one per
 * line, from another client. Clicking into it to select one code and clicking
 * away wrote all ten back as one line. It is drawn in lines, it writes nothing
 * when nothing was typed, an edit writes every line, and a paste into a field
 * on one line keeps its breaks.
 */
it("keeps the lines of a field of the reader's own through a click, an edit and a paste", () => {
	ipc.setField.mockResolvedValue(entry());
	const codes = ['1111-aaaa', '2222-bbbb', '3333-cccc'].join('\n');
	const windows = codes.replaceAll('\n', '\r\n');

	const component = show({
		fields: [
			field({ name: 'Recovery codes', kind: 'custom', value: codes, empty: false }),
			field({ name: 'From Windows', kind: 'custom', value: windows, empty: false }),
			field({ name: 'Backup', kind: 'custom', value: '', empty: true })
		]
	});
	flushSync();

	const lined = host.querySelector('[aria-label="Recovery codes"]') as HTMLTextAreaElement;
	expect(lined.tagName).toBe('TEXTAREA');
	expect(lined.value).toBe(codes);
	lined.dispatchEvent(new FocusEvent('focus'));
	lined.dispatchEvent(new FocusEvent('blur'));
	// A text area in WebKit reads a CRLF back as a line feed, so this field
	// differs from the vault's value the moment it is drawn.
	const crlf = host.querySelector('[aria-label="From Windows"]') as HTMLTextAreaElement;
	crlf.value = codes;
	crlf.dispatchEvent(new FocusEvent('focus'));
	crlf.dispatchEvent(new FocusEvent('blur'));
	expect(ipc.setField, 'a click through wrote the codes back').not.toHaveBeenCalled();

	const fixed = codes.replace('2222-bbbb', '2222-bbbc');
	enter(lined, fixed);
	lined.dispatchEvent(new FocusEvent('blur'));
	expect(ipc.setField).toHaveBeenCalledWith(
		expect.any(String),
		'Recovery codes',
		fixed,
		false,
		expect.any(Number)
	);

	const single = host.querySelector('[aria-label="Backup"]') as HTMLTextAreaElement;
	enter(single, codes);
	single.dispatchEvent(new FocusEvent('blur'));
	expect(ipc.setField).toHaveBeenLastCalledWith(
		expect.any(String),
		'Backup',
		codes,
		false,
		expect.any(Number)
	);

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
			root: group({ name: 'Root' }),
			path: [group({ name: 'Work' })],
			history: null,
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy: vi.fn(),
			onChanged,
			onVersions,
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onPutBack: vi.fn(),
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
			root: group({ name: 'Root' }),
			path: [group({ name: 'Work' })],
			history: null,
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy: vi.fn(),
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onPutBack: vi.fn(),
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
		root: group({ name: 'Root' }),
		path: [group({ name: 'Work' })],
		history: null,
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onPutBack: vi.fn(),
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
			root: group({ name: 'Root' }),
			path: [group({ name: 'Work' })],
			history: null,
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: true,
			onCopy: vi.fn(),
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onPutBack: vi.fn(),
			onFieldRemoved: vi.fn(),
			onFailure: vi.fn()
		}
	});
	flushSync();

	expect(host.textContent).toContain('node-3');
	expect(host.textContent).not.toContain('Move to Recycle Bin');
	expect(host.textContent).not.toContain('Delete forever');
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
 * The half minute is how long a value Coffer put on the screen stays there.
 * A new password being typed beside it is not that value: the old one goes
 * when its time is up, and what the reader is writing stays until they save
 * it or put it away.
 */
it('keeps a new password being typed when the old one hides', async () => {
	vi.useFakeTimers();
	try {
		ipc.setField.mockResolvedValue(entry());
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

		button('Change').click();
		flushSync();
		expect(value(), 'the old one went when the new one was asked for').toBe(SECRET);
		enter(changer('New password'), 'a password of my own');

		vi.advanceTimersByTime(60_000);
		flushSync();

		expect(value()).toBe('');
		expect(host.textContent).not.toContain('Hides in');
		expect(changer('New password').value).toBe('a password of my own');

		changer('New password').dispatchEvent(
			new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })
		);
		expect(ipc.setField).toHaveBeenCalledWith(
			expect.any(String),
			'Password',
			'a password of my own',
			true,
			expect.any(Number)
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
 * The trash used to stand sixteen pixels from the close, the same size and the
 * same grey, and a press meant for one took the other. The header now carries
 * the way out and nothing else; deleting is a labelled action at the foot of
 * the pane, after everything else in it.
 */
it('offers a way out of the entry, and it is not the way to delete one', () => {
	const onClose = vi.fn();
	const onDelete = vi.fn();
	const component = mount(EntryView, {
		target: host,
		props: {
			entry: entry({ fields: [field({ name: 'Title', kind: 'title', value: 'node-3' })] }),
			root: group({ name: 'Root' }),
			path: [group({ name: 'Work' })],
			history: null,
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy: vi.fn(),
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose,
			onDelete,
			onPutBack: vi.fn(),
			onFieldRemoved: vi.fn(),
			onFailure: vi.fn()
		}
	});
	flushSync();

	const header = host.querySelector('header');
	const buttons = [...(header?.querySelectorAll('button') ?? [])].map((each) =>
		each.getAttribute('aria-label')
	);
	expect(buttons, 'something that loses the entry is back beside the close').toEqual([
		'Close this entry'
	]);

	// The last button in the pane, in words.
	const all = [...host.querySelectorAll('button')];
	expect(all.at(-1)?.textContent?.trim()).toBe('Move to Recycle Bin');

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
			root: group({ name: 'Root' }),
			path: [group({ name: 'Work' })],
			history: null,
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: true,
			onCopy: vi.fn(),
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose,
			onDelete: vi.fn(),
			onPutBack: vi.fn(),
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
			root: group({ name: 'Root' }),
			path: [group({ name: 'Work' })],
			history: null,
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy: vi.fn(),
			onChanged,
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onPutBack: vi.fn(),
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
		root: group({ name: 'Root' }),
		path: [group({ name: 'Work' })],
		history: null,
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onPutBack: vi.fn(),
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
		root: group({ name: 'Root' }),
		path: [group({ name: 'Work' })],
		history: null,
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onPutBack: vi.fn(),
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
		root: group({ name: 'Root' }),
		path: [group({ name: 'Work' })],
		history: null,
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onPutBack: vi.fn(),
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
		root: group({ name: 'Root' }),
		path: [group({ name: 'Work' })],
		history: null,
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onPutBack: vi.fn(),
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

/** The window takes the pane down when another entry is chosen, and a panel
 * still open then answers a pane that is gone. It still holds the entry it was
 * last given, and a question drawn there is one nobody will ever see, so the
 * file waiting in Rust for its answer is let go. */
it('lets the chosen file go when the pane is gone by the time the answer comes', async () => {
	const answer = Promise.withResolvers<Attached>();
	ipc.addAttachment.mockReset();
	ipc.addAttachment.mockReturnValue(answer.promise);
	const first = entry({ attachments: [attachment({ name: SCAN })] });
	const component = show(first);
	flushSync();

	icon('Add a file').click();
	await unmount(component);
	answer.resolve(taken());

	await vi.waitFor(() => expect(ipc.withdrawAttachment).toHaveBeenCalledWith(first.id));
	expect(ipc.withdrawAttachment).toHaveBeenCalledTimes(1);
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

/** The node a protected field of the reader's own is shown in, in its own row. */
function own(label: string): HTMLElement {
	const found = icon(`Copy ${label}`).parentElement?.querySelector<HTMLElement>('[data-value]');
	if (!found) throw new Error(`no value shown for ${label}`);
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
			root: group({ name: 'Root' }),
			path: [group({ name: 'Work' })],
			history: null,
			now: new Date('2026-08-29T14:30:00Z'),
			readOnly: false,
			onCopy,
			onChanged: vi.fn(),
			onVersions: vi.fn(),
			onClose: vi.fn(),
			onDelete: vi.fn(),
			onPutBack: vi.fn(),
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
 * A protected field of the reader's own follows the password's rule. Showing
 * it is reading it; changing it is its own Change, on the line under the row,
 * with the old value still on the screen while the new one is typed.
 */
it('changes a protected own field only through its own Change', async () => {
	ipc.reveal.mockResolvedValue(TOKEN);
	ipc.setField.mockResolvedValue(entry());
	const { component } = pane({
		fields: [
			field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false })
		]
	});

	icon('Show API token').click();
	await vi.waitFor(() => expect(own('API token').textContent).toBe(TOKEN));
	flushSync();

	// On the screen, only there, and not in anything that takes a key.
	expect(attributes().join(' ')).not.toContain(TOKEN);
	expect(written()).not.toContain(TOKEN);
	expect(document.activeElement).toBe(icon('Hide API token'));
	icon('Hide API token').dispatchEvent(new FocusEvent('blur'));
	expect(ipc.setField).not.toHaveBeenCalled();

	icon('Change API token').click();
	flushSync();
	const typed = changer('New value of API token');
	expect(own('API token').textContent, 'the old value went when the new one was asked for').toBe(
		TOKEN
	);
	enter(typed, 'sk-live-rotated');
	typed.dispatchEvent(
		new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })
	);

	await vi.waitFor(() => expect(ipc.setField).toHaveBeenCalledTimes(1));
	expect(ipc.setField).toHaveBeenCalledWith(
		expect.any(String),
		'API token',
		'sk-live-rotated',
		true,
		expect.any(Number)
	);

	return unmount(component);
});

/** Escape puts a new value away unwritten, and leaving one half typed asks
 * about it by the field's own name. */
it('writes nothing when the reader escapes out of a protected own field', async () => {
	ipc.setField.mockResolvedValue(entry());
	const { component } = pane({
		fields: [
			field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false })
		]
	});

	icon('Change API token').click();
	flushSync();
	enter(changer('New value of API token'), 'half a to');
	changer('New value of API token').dispatchEvent(
		new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })
	);
	flushSync();
	expect(ipc.setField).not.toHaveBeenCalled();
	expect(written()).not.toContain('half a to');
	expect(host.querySelector('[aria-label="New value of API token"]')).toBeNull();

	icon('Change API token').click();
	flushSync();
	enter(changer('New value of API token'), 'half a to');
	leave(changer('New value of API token'));
	expect(host.textContent).toContain('Save the new value of “API token”?');
	expect(ipc.setField).not.toHaveBeenCalled();

	return unmount(component);
});

/** A database Coffer cannot write back still shows and copies. It just does not
 * offer to change anything. */
it('offers a look and a copy of a protected own field it cannot write', async () => {
	ipc.reveal.mockResolvedValue(TOKEN);
	const { component, onCopy } = pane(
		{
			fields: [
				field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false })
			]
		},
		true
	);

	expect(host.querySelector('[aria-label="Change API token"]')).toBeNull();
	icon('Show API token').click();
	await vi.waitFor(() => expect(own('API token').textContent).toBe(TOKEN));
	icon('Copy API token').click();
	expect(onCopy).toHaveBeenCalledWith(expect.any(String), 'API token', null);

	expect(host.querySelectorAll('textarea, input')).toHaveLength(0);
	expect(ipc.setField).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * Every copy of a revealed value goes through Rust. The system's own copy of a
 * selection is an ordinary pasteboard write - no concealed type, nothing
 * clearing it after a minute - so it is cancelled, and Rust is asked instead,
 * for exactly the part that was selected: one recovery code out of ten.
 */
it('hands a copy or a cut of a revealed value to Rust, the part selected and no more', async () => {
	const codes = ['1111-aaaa', '2222-bbbb', '3333-cccc', '4444-dddd'].join('\n');
	ipc.reveal.mockResolvedValue(codes);
	const { component, onCopy } = pane({
		fields: [
			field({ name: 'Recovery codes', kind: 'custom', protected: true, value: null, empty: false })
		]
	});

	icon('Show Recovery codes').click();
	const node = own('Recovery codes');
	await vi.waitFor(() => expect(node.textContent).toBe(codes));

	choose(node, 20, 29);
	const copied = clipboard(node, 'copy');
	expect(copied.defaultPrevented, 'the system copied a secret').toBe(true);
	expect(onCopy).toHaveBeenLastCalledWith(expect.any(String), 'Recovery codes', {
		from: 20,
		to: 29
	});

	const cut = clipboard(node, 'cut');
	expect(cut.defaultPrevented, 'the system cut a secret').toBe(true);
	expect(onCopy).toHaveBeenLastCalledWith(expect.any(String), 'Recovery codes', {
		from: 20,
		to: 29
	});

	// All of it selected is the whole field, which is how Rust is asked for one.
	choose(node, 0, codes.length);
	clipboard(node, 'copy');
	expect(onCopy).toHaveBeenLastCalledWith(expect.any(String), 'Recovery codes', null);

	// And the line breaks are on the screen as line breaks.
	expect(node.textContent).toBe(codes);
	expect(node.className).toContain('whitespace-pre');

	return unmount(component);
});

/**
 * The menu WebKit draws under the pointer offers Look Up, Translate, Search
 * and Share, each of which hands the value to another application, and a
 * selection dragged out of the window goes wherever it is dropped. Neither
 * starts on a revealed value, wherever in the pane that value is.
 */
it('draws no menu and starts no drag on a revealed value', async () => {
	const { component } = pane({
		fields: [
			field({ name: 'UserName', kind: 'username', value: null, empty: false }),
			field({ name: 'Password', kind: 'password', value: null, empty: false }),
			field({ name: 'Notes', kind: 'notes', value: null, empty: false }),
			field({ name: 'PIN', kind: 'custom', protected: true, value: null, empty: false })
		]
	});

	const nodes = [...host.querySelectorAll<HTMLElement>('[data-value]')];
	expect(nodes).toHaveLength(4);
	// The password's own Show is a word on a button rather than an eye.
	for (const eye of host.querySelectorAll<HTMLButtonElement>('button'))
		if (eye.textContent.trim() === 'Show' || eye.getAttribute('aria-label')?.startsWith('Show '))
			eye.click();
	await vi.waitFor(() => expect(nodes.every((node) => node.textContent === SECRET)).toBe(true));

	for (const node of nodes) {
		for (const kind of ['contextmenu', 'dragstart']) {
			const event = new MouseEvent(kind, { bubbles: true, cancelable: true });
			node.dispatchEvent(event);
			expect(event.defaultPrevented, `${kind} was left to the system`).toBe(true);
		}
	}

	return unmount(component);
});

/**
 * Show puts the focus on the row's own eye, so Cmd+C then copies that row's
 * value through Rust rather than the entry's password - and the window, which
 * would copy the password, hears that the key has been answered.
 */
it('copies the row the focus is on when Cmd+C is pressed there', async () => {
	ipc.reveal.mockResolvedValue(TOKEN);
	const { component, onCopy } = pane({
		fields: [
			field({ name: 'Notes', kind: 'notes', value: null, empty: false }),
			field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false })
		]
	});

	icon('Show API token').click();
	await vi.waitFor(() => expect(document.activeElement).toBe(icon('Hide API token')));

	const pressed = new KeyboardEvent('keydown', {
		key: 'c',
		metaKey: true,
		bubbles: true,
		cancelable: true
	});
	icon('Hide API token').dispatchEvent(pressed);
	expect(pressed.defaultPrevented).toBe(true);
	expect(onCopy).toHaveBeenCalledWith(expect.any(String), 'API token', null);

	icon('Copy Notes').dispatchEvent(
		new KeyboardEvent('keydown', { key: 'c', metaKey: true, bubbles: true, cancelable: true })
	);
	expect(onCopy).toHaveBeenLastCalledWith(expect.any(String), 'Notes', null);

	// A selection standing somewhere is the system's copy to fire, on the node
	// holding it, and not this row's.
	vi.spyOn(document, 'getSelection').mockReturnValue({ isCollapsed: false } as Selection);
	const selecting = new KeyboardEvent('keydown', {
		key: 'c',
		metaKey: true,
		bubbles: true,
		cancelable: true
	});
	icon('Hide API token').dispatchEvent(selecting);
	expect(selecting.defaultPrevented).toBe(false);
	expect(onCopy).toHaveBeenCalledTimes(2);

	return unmount(component);
});

/**
 * The step that names a new field offers to write it in lines. The format has
 * no such flag, so the choice decides only the first field the value is
 * written in: one that takes Return as the next line. Pressing the option is
 * not leaving the name.
 */
it('names a new field to be written in lines', async () => {
	const named = 'Recovery codes';
	const made = entry({
		fields: [field({ name: named, kind: 'custom', value: null, empty: true, protected: true })]
	});
	ipc.setField.mockResolvedValue(made);
	const { component, props } = deleting({ fields: [] });

	icon('Add a field').click();
	flushSync();
	const name = host.querySelector('[aria-label="The name of the new field"]') as HTMLInputElement;
	name.value = named;

	const option = [...host.querySelectorAll('button')].find(
		(each) => each.textContent?.trim() === 'Multi-line'
	);
	if (!option) throw new Error('there is no Multi-line option');
	const press = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
	option.dispatchEvent(press);
	expect(press.defaultPrevented, 'the option takes the focus off the name').toBe(true);
	// Moving between the name and the option is not leaving the two of them.
	name.dispatchEvent(new FocusEvent('focusout', { bubbles: true, relatedTarget: option }));
	option.click();
	flushSync();
	expect(option.getAttribute('aria-pressed')).toBe('true');
	expect(ipc.setField, 'the name was finished by the option').not.toHaveBeenCalled();

	name.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
	flushSync();
	expect(ipc.setField).toHaveBeenCalledWith(props.entry.id, named, '', true, expect.any(Number));

	props.entry = { ...made, id: props.entry.id };
	flushSync();
	const first = host.querySelector(`[aria-label="${named}"]`) as HTMLTextAreaElement;
	expect(first.tagName).toBe('TEXTAREA');
	expect(first.getAttribute('rows')).toBe('4');
	const returned = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
	first.dispatchEvent(returned);
	expect(returned.defaultPrevented, 'Return finished a field asked for in lines').toBe(false);

	return unmount(component);
});

/** The pane with its callbacks as mocks the test can read. */
function pane(over: Parameters<typeof entry>[0], readOnly = false) {
	const props = {
		entry: entry(over),
		root: group({ name: 'Root' }),
		path: [group({ name: 'Work' })],
		history: null,
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onPutBack: vi.fn(),
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
		root: group({ name: 'Root' }),
		path: [group({ name: 'Work' })],
		history: null,
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onPutBack: vi.fn(),
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
		root: group({ name: 'Root' }),
		path: [group({ name: 'Work' })],
		history: null,
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged,
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onPutBack: vi.fn(),
		onFieldRemoved: vi.fn(),
		onFailure: vi.fn()
	};
	const component = mount(EntryView, { target: host, props });
	flushSync();

	icon('Remove the field PIN').click();
	await vi.waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));
	expect(ipc.removeField).toHaveBeenCalledWith(props.entry.id, 'PIN', false);
	expect(props.onFieldRemoved, 'the window heard before the save was done').not.toHaveBeenCalled();

	settle();
	await vi.waitFor(() =>
		expect(props.onFieldRemoved).toHaveBeenCalledWith(props.entry.id, 'PIN', false)
	);
	expect(host.querySelector('[data-confirm]'), 'a removal that can be undone asked').toBeNull();

	return unmount(component);
});

/** What Rust answers a removal the vault keeps no version of. */
const FOR_GOOD = {
	code: 'forGood',
	message: 'this vault keeps no version to bring that field back from'
};

/**
 * A vault that keeps no versions, or a size limit the one a removal writes does
 * not fit, loses the field at the next save and leaves nothing to undo it
 * with. The first press takes nothing: Rust refuses it, and the row asks in
 * words, with the way out where the focus is. The field goes only on the red
 * answer, and the window is told it went for good.
 */
it('asks before a removal nothing could take back, and removes only on the red answer', async () => {
	ipc.removeField.mockReset();
	ipc.removeField.mockRejectedValueOnce(FOR_GOOD).mockResolvedValue(entry());
	const {
		component,
		entry: shown,
		onChanged,
		onFieldRemoved,
		onFailure
	} = pane({
		fields: [field({ name: 'PIN', kind: 'custom', protected: true, value: null, empty: false })]
	});

	icon('Remove the field PIN').click();
	await vi.waitFor(() => expect(question()).not.toBe(''));
	flushSync();
	expect(question()).toContain(
		'Remove “PIN”? This vault keeps no version to bring it back from, so this can’t be undone.'
	);
	expect(ipc.removeField).toHaveBeenCalledWith(shown.id, 'PIN', false);
	expect(onChanged).not.toHaveBeenCalled();
	expect(onFailure, 'a question was reported as a failure').not.toHaveBeenCalled();
	expect(onFieldRemoved).not.toHaveBeenCalled();
	expect(document.activeElement).toBe(button('Keep it'));
	expect(host.querySelector('[aria-label="Remove the field PIN"]')).toBeNull();

	button('Keep it').click();
	flushSync();
	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(ipc.removeField).toHaveBeenCalledTimes(1);

	ipc.removeField.mockRejectedValueOnce(FOR_GOOD);
	icon('Remove the field PIN').click();
	await vi.waitFor(() => expect(question()).not.toBe(''));
	flushSync();
	button('Remove').click();
	await vi.waitFor(() => expect(onFieldRemoved).toHaveBeenCalledWith(shown.id, 'PIN', true));
	expect(ipc.removeField).toHaveBeenLastCalledWith(shown.id, 'PIN', true);
	expect(onChanged).toHaveBeenCalledTimes(1);
	expect(host.querySelector('[data-confirm]')).toBeNull();

	return unmount(component);
});

/** The question is about the entry the press was on. An answer that arrives
 * once the pane shows another entry asks nobody, and nothing came off. */
it('asks nothing in an entry the pane has left', async () => {
	const refusing = Promise.withResolvers<never>();
	ipc.removeField.mockReset();
	ipc.removeField.mockReturnValueOnce(refusing.promise);
	const pin = field({ name: 'PIN', kind: 'custom', protected: true, value: null, empty: false });
	const props = reactive({
		entry: entry({ fields: [pin] }),
		root: group({ name: 'Root' }),
		path: [group({ name: 'Work' })],
		history: null,
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onPutBack: vi.fn(),
		onFieldRemoved: vi.fn(),
		onFailure: vi.fn()
	});
	const component = mount(EntryView, { target: host, props });
	flushSync();

	icon('Remove the field PIN').click();
	props.entry = entry({ fields: [pin] });
	flushSync();
	refusing.reject(FOR_GOOD);
	await new Promise((resolve) => setTimeout(resolve, 0));
	flushSync();

	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(props.onFailure).not.toHaveBeenCalled();
	expect(props.onFieldRemoved).not.toHaveBeenCalled();

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

/** The pane on an entry, with the tree around it that names folders. */
function deleting(over: Parameters<typeof entry>[0], readOnly = false, root = group()) {
	const props = reactive({
		entry: entry(over),
		root,
		path: [group({ name: 'Work' })],
		history: null,
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDelete: vi.fn(),
		onPutBack: vi.fn(),
		onFieldRemoved: vi.fn(),
		onFailure: vi.fn()
	});
	const component = mount(EntryView, { target: host, props });
	flushSync();
	return { component, props };
}

const titled = (title: string | null) => [
	field({ name: 'Title', kind: 'title', value: title, protected: title === null, empty: false })
];

/**
 * A move to the bin is taken back from the notice that follows it, so the
 * press goes at once: a question in front of something that can be undone is
 * a question people learn to click through, and then it is no use for the one
 * that cannot.
 */
it('moves an entry to the bin from the foot of the pane at once', () => {
	const { component, props } = deleting({ fields: titled('Bank'), deletion: 'bin' });

	button('Move to Recycle Bin').click();
	flushSync();

	expect(props.onDelete).toHaveBeenCalledTimes(1);
	expect(host.querySelector('[data-confirm]')).toBeNull();

	return unmount(component);
});

/**
 * An entry whose deletion is final - a vault that keeps no bin - says so
 * before it goes, names it, keeps it on the way out and on Escape, and goes
 * only on the red answer.
 */
it('asks before an entry goes for good, and keeps it on the way out', () => {
	const { component, props } = deleting({ fields: titled('Bank'), deletion: 'forever' });

	expect(host.textContent).not.toContain('Move to Recycle Bin');
	button('Delete forever…').click();
	flushSync();

	expect(host.querySelector('[data-confirm]')?.textContent).toContain(
		'Delete “Bank” forever? This can’t be undone.'
	);
	expect(document.activeElement).toBe(button('Keep it'));
	expect(props.onDelete).not.toHaveBeenCalled();

	button('Keep it').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
	flushSync();
	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(props.onDelete).not.toHaveBeenCalled();

	button('Delete forever…').click();
	flushSync();
	button('Delete forever').click();
	flushSync();
	expect(props.onDelete).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/** A title the database protects is not revealed to be put in a question, and
 * an empty one is not written as a pair of quotes around nothing. */
it('calls an entry with no title it can show "this entry"', () => {
	for (const title of [null, '']) {
		const { component } = deleting({ fields: titled(title), deletion: 'forever' });
		button('Delete forever…').click();
		flushSync();
		expect(host.querySelector('[data-confirm]')?.textContent).toContain(
			'Delete this entry forever?'
		);
		void unmount(component);
	}
});

/** A question about deleting one entry for good is never still open over the
 * next one, where its red answer would delete an entry nobody asked about. */
it('drops the question about deleting an entry when another is shown', () => {
	const { component, props } = deleting({ fields: titled('Bank'), deletion: 'forever' });
	button('Delete forever…').click();
	flushSync();
	expect(host.querySelector('[data-confirm]')).not.toBeNull();

	props.entry = entry({ fields: titled('Mail'), deletion: 'forever' });
	flushSync();

	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(props.onDelete).not.toHaveBeenCalled();

	return unmount(component);
});

/** The same for the card over an entry in the bin: its question belongs to the
 * entry it was asked about. */
it('drops the question about deleting an entry in the bin when another is shown', () => {
	const binned = { since: null, from: null };
	const { component, props } = deleting({ fields: titled('Bank'), binned, deletion: 'forever' });
	button('Delete forever…').click();
	flushSync();
	expect(host.querySelector('[data-confirm]')?.textContent).toContain('“Bank”');

	props.entry = entry({ fields: titled('Mail'), binned, deletion: 'forever' });
	flushSync();

	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(props.onDelete).not.toHaveBeenCalled();

	return unmount(component);
});

const personal = group({ name: 'Personal' });
const tree = group({ name: 'Root', sections: [personal] });

/**
 * An entry opened in the bin is read, not edited: every field is text, there
 * is nothing to add or take away, and the one thing on offer is to put it back
 * or let it go. The sentence over it says since when and where from.
 */
it('shows an entry in the bin read only, with the way back out on top', () => {
	const { component, props } = deleting(
		{
			fields: [
				...titled('Bank'),
				field({ name: 'UserName', kind: 'username', value: 'me', empty: false }),
				field({ name: 'Notes', kind: 'notes', value: 'a note', empty: false }),
				field({ name: 'PIN', kind: 'custom', value: '2202', empty: false })
			],
			tags: ['money'],
			attachments: [attachment({ name: 'statement.pdf', fileName: 'statement.pdf' })],
			binned: { since: '2026-08-27T10:00:00Z', from: personal.id },
			deletion: 'forever'
		},
		false,
		tree
	);

	expect(host.querySelector('[data-binned]')?.textContent).toContain(
		'In the Recycle Bin since 27 Aug · was in “Personal”'
	);
	const writable = [...host.querySelectorAll<HTMLInputElement>('input, textarea')].filter(
		(each) => !each.readOnly
	);
	expect(writable, 'a value in the bin can be written into').toHaveLength(0);
	for (const label of [
		'Add a field',
		'Add a file',
		'Remove statement.pdf',
		'Remove the field PIN',
		'Remove the tag money'
	]) {
		expect(host.querySelector(`[aria-label="${label}"]`), label).toBeNull();
	}
	expect(host.textContent).not.toContain('Move to Recycle Bin');
	// Reading is still reading: the file can still be written out.
	expect(host.querySelector('[aria-label="Write statement.pdf out"]')).not.toBeNull();

	button('Put back').click();
	expect(props.onPutBack).toHaveBeenCalledTimes(1);

	button('Delete forever…').click();
	flushSync();
	expect(host.querySelector('[data-confirm]')?.textContent).toContain(
		'Delete “Bank” forever? This can’t be undone.'
	);
	button('Delete forever').click();
	expect(props.onDelete).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/**
 * Where the bin knows where something came from it says so, the top of the
 * vault included. Where it does not, the sentence says where Put back takes
 * it instead of inventing a past.
 */
it('says where an entry goes back to, and does not invent where it came from', () => {
	for (const [from, said] of [
		[tree.id, 'In the Recycle Bin since 27 Aug · was at the top of the vault'],
		[null, 'In the Recycle Bin since 27 Aug · goes back to the top of the vault']
	] as const) {
		const { component } = deleting(
			{
				fields: titled('Bank'),
				binned: { since: '2026-08-27T10:00:00Z', from },
				deletion: 'forever'
			},
			false,
			tree
		);
		expect(host.querySelector('[data-binned]')?.textContent).toContain(said);
		void unmount(component);
	}
});

/** A folder name out of somebody's database is text in the sentence, however
 * much it looks like markup. */
it('writes the name of the folder an entry came from as text', () => {
	const hostile = group({ name: '<img src=x onerror="alert(1)">' });
	const { component } = deleting(
		{
			fields: titled('Bank'),
			binned: { since: null, from: hostile.id },
			deletion: 'forever'
		},
		false,
		group({ sections: [hostile] })
	);

	expect(host.querySelector('img')).toBeNull();
	expect(host.querySelector('[data-binned]')?.textContent).toContain(
		'In the Recycle Bin · was in “<img src=x onerror="alert(1)">”'
	);

	return unmount(component);
});

/** A database Coffer will not write back says what is in its bin and offers
 * nothing it would only refuse. */
it('offers no way out of the bin on a database it cannot write', () => {
	const { component } = deleting(
		{
			fields: titled('Bank'),
			binned: { since: '2026-08-27T10:00:00Z', from: personal.id },
			deletion: 'forever'
		},
		true,
		tree
	);

	expect(host.querySelector('[data-binned]')?.textContent).toContain('was in “Personal”');
	expect(host.textContent).not.toContain('Put back');
	expect(host.textContent).not.toContain('Delete forever');

	return unmount(component);
});

/** Every word Rust was told about typing, as (field, value, protected, number). */
function drafted(): [string, string | null, boolean, number][] {
	return ipc.draft.mock.calls.map((call) => [call[1], call[2], call[3], call[5]]);
}

/** Whether each word Rust was told about typing was about a new value typed in
 * a Change field, which a lock keeps beside the old one. */
function besides(): boolean[] {
	return ipc.draft.mock.calls.map((call) => call[4]);
}

/** The number the last value written carried. */
function lastWritten(): number {
	const call = ipc.setField.mock.lastCall;
	if (!call) throw new Error('nothing was written');
	return call[4] as number;
}

/** The mark inside the field an element is typed into, if it is drawn. */
function markOf(typed: HTMLElement): HTMLElement | null {
	return typed.parentElement?.querySelector<HTMLElement>('[data-unsaved]') ?? null;
}

/**
 * A lid closed half way through a login used to take the login with it: the
 * value reached Rust when the field was left, and a lock destroys the window
 * first. What is typed is told to Rust a moment after the last key, the field
 * says it is not saved yet, and leaving it writes it with a number newer than
 * any draft of it - so a draft still on its way can never be written over it.
 */
it('tells Rust what is typed before the field is left, and finishes it when it is', () => {
	vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
	ipc.setField.mockResolvedValue(entry());
	const component = show({
		fields: [
			field({ name: 'UserName', kind: 'username', value: 'alice', empty: false, protected: false })
		]
	});
	try {
		flushSync();
		const login = host.querySelector('[aria-label="Login"]') as HTMLInputElement;
		expect(markOf(login)).toBeNull();

		enter(login, 'bo');
		expect(markOf(login)?.textContent).toContain('Not saved yet');
		expect(markOf(login)?.className).toContain('bg-warn');
		expect(ipc.draft, 'every key was a message').not.toHaveBeenCalled();

		vi.advanceTimersByTime(250);
		expect(drafted()).toEqual([['UserName', 'bo', false, expect.any(Number)]]);
		expect(besides(), 'a login typed where it stands was told as a new value').toEqual([false]);

		enter(login, 'bob');
		vi.advanceTimersByTime(100);
		login.dispatchEvent(new Event('blur'));
		flushSync();

		expect(ipc.setField).toHaveBeenCalledWith(
			expect.any(String),
			'UserName',
			'bob',
			false,
			expect.any(Number)
		);
		const [[, , , drafts]] = drafted();
		expect(lastWritten(), 'the value is older than its own draft').toBeGreaterThan(drafts);
		expect(markOf(login), 'a written field still says it is not saved').toBeNull();

		vi.advanceTimersByTime(10_000);
		expect(ipc.draft, 'a draft followed the value that finished it').toHaveBeenCalledTimes(1);
	} finally {
		vi.useRealTimers();
		unmount(component);
	}
});

/** Escape puts the value back, and a field typed in and put back as it was
 * is left with nothing written: either way Rust is told to let go of the
 * draft it holds, in a word newer than the draft. */
it('lets go of what was typed on Escape, and of typing that came back to where it started', () => {
	vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
	const component = show({
		fields: [
			field({ name: 'UserName', kind: 'username', value: 'alice', empty: false, protected: false }),
			field({ name: 'URL', kind: 'url', value: 'https://a.example', empty: false })
		]
	});
	try {
		flushSync();
		const login = host.querySelector('[aria-label="Login"]') as HTMLInputElement;
		enter(login, 'mallory');
		vi.advanceTimersByTime(250);
		login.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
		flushSync();

		expect(login.value).toBe('alice');
		expect(markOf(login)).toBeNull();
		const [[, typed, , told], [field, back, , after]] = drafted();
		expect(typed).toBe('mallory');
		expect([field, back]).toEqual(['UserName', null]);
		expect(after).toBeGreaterThan(told);

		const address = host.querySelector('[aria-label="Address"]') as HTMLInputElement;
		enter(address, 'https://b.example');
		vi.advanceTimersByTime(250);
		enter(address, 'https://a.example');
		address.dispatchEvent(new Event('blur'));
		flushSync();

		expect(drafted().at(-1)?.slice(0, 2)).toEqual(['URL', null]);
		expect(markOf(address)).toBeNull();
		expect(ipc.setField, 'a value put back as it was was written').not.toHaveBeenCalled();
	} finally {
		vi.useRealTimers();
		unmount(component);
	}
});

/**
 * A new password half typed is the reader's too, and a lock that comes before
 * Save writes it rather than wiping it. It is told to Rust protected, and
 * Discard, Cancel and Save each end it: the first two with a word that takes
 * it back, Save with a write newer than it.
 */
it('tells Rust a new password as it is typed, and ends it on Discard, Cancel and Save', async () => {
	vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
	ipc.setField.mockResolvedValue(entry());
	const { component } = pane({
		fields: [
			field({ name: 'Password', kind: 'password', value: null, empty: false, protected: true })
		]
	});
	try {
		button('Change').click();
		flushSync();
		enter(changer('New password'), 'half a pass');
		expect(markOf(changer('New password'))).not.toBeNull();
		vi.advanceTimersByTime(250);
		expect(drafted()).toEqual([['Password', 'half a pass', true, expect.any(Number)]]);

		leave(changer('New password'));
		button('Discard').click();
		flushSync();
		expect(drafted().at(-1)?.slice(0, 2)).toEqual(['Password', null]);

		button('Change').click();
		flushSync();
		enter(changer('New password'), 'another');
		vi.advanceTimersByTime(250);
		button('Cancel').click();
		flushSync();
		expect(drafted().at(-1)?.slice(0, 2)).toEqual(['Password', null]);

		button('Change').click();
		flushSync();
		enter(changer('New password'), 'the whole new one');
		vi.advanceTimersByTime(250);
		const [, , , sent] = drafted().at(-1) ?? [];
		button('Save').click();
		await vi.waitFor(() => expect(ipc.setField).toHaveBeenCalledTimes(1));
		expect(lastWritten()).toBeGreaterThan(sent as number);

		const numbers = drafted().map(([, , , number]) => number);
		expect(numbers, 'the numbers went backwards').toEqual([...numbers].sort((a, b) => a - b));
		expect(besides(), 'a new password was told as the password itself').not.toContain(false);
		vi.advanceTimersByTime(10_000);
		expect(drafted().at(-1)?.[1], 'a draft followed the saved password').toBe('the whole new one');
	} finally {
		vi.useRealTimers();
		unmount(component);
	}
});

/** A new password half typed for an entry the pane no longer shows is reported
 * as not saved, and it is not left in Rust for a later lock to write into an
 * entry the reader has moved away from. */
it('lets go of a new password half typed when its entry goes', () => {
	vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
	const { component, props } = deleting({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	try {
		const left = props.entry.id;
		button('Change').click();
		flushSync();
		enter(changer('New password'), 'half');
		vi.advanceTimersByTime(250);

		props.entry = entry({
			fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
		});
		flushSync();

		expect(ipc.draft).toHaveBeenLastCalledWith(
			left,
			'Password',
			null,
			false,
			true,
			expect.any(Number)
		);
		vi.advanceTimersByTime(10_000);
		expect(ipc.draft).toHaveBeenCalledTimes(2);
	} finally {
		vi.useRealTimers();
		unmount(component);
	}
});

/** A password the pane shows masked, which a Change writes over. */
const PASSWORD = field({
	name: 'Password',
	kind: 'password',
	value: null,
	empty: false,
	protected: true
});

/** A Return pressed in a field, as the keyboard sends it. */
function returned(into: HTMLElement, over: KeyboardEventInit = {}): KeyboardEvent {
	const pressed = new KeyboardEvent('keydown', {
		key: 'Enter',
		bubbles: true,
		cancelable: true,
		...over
	});
	into.dispatchEvent(pressed);
	return pressed;
}

/**
 * The press that opened the field, pressed again, used to throw a new
 * password away without a word: it reads as "done" as easily as "never mind".
 * With something typed it asks what leaving asks, with the focus on the answer
 * that loses nothing; with nothing typed it puts the field away.
 */
it('asks rather than discarding when Change is pressed again over a new password', () => {
	const { component } = pane({ fields: [PASSWORD] });

	button('Change').click();
	flushSync();
	enter(changer('New password'), 'set on the website just now');
	button('Change').click();
	flushSync();

	expect(host.textContent).toContain('Save the new password?');
	expect(changer('New password').value, 'the press threw it away').toBe(
		'set on the website just now'
	);
	expect(document.activeElement, 'the question did not take the focus').toBe(button('Save'));
	expect(ipc.setField).not.toHaveBeenCalled();

	button('Discard').click();
	flushSync();
	button('Change').click();
	flushSync();
	button('Change').click();
	flushSync();
	expect(host.querySelector('[aria-label="New password"]'), 'an empty field stayed').toBeNull();

	return unmount(component);
});

/** Return and Save pressed together, or Return twice behind a slow save,
 * write the new password once. */
it('writes a new password once when Return and Save are pressed together', async () => {
	const saving = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.setField.mockReturnValue(saving.promise);
	const { component } = pane({ fields: [PASSWORD] });

	button('Change').click();
	flushSync();
	enter(changer('New password'), 'only once');
	returned(changer('New password'));
	returned(changer('New password'));
	button('Save').click();
	saving.resolve(entry());
	await vi.waitFor(() => expect(host.querySelector('[aria-label="New password"]')).toBeNull());

	expect(ipc.setField).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/**
 * The Return that confirms an input method's conversion belongs to the input
 * method. WebKit commonly sends it with `isComposing` already false and the
 * key code 229, and that Return saved a password half converted.
 */
it('saves nothing on the Return that ends a composition', () => {
	const { component } = pane({ fields: [PASSWORD] });

	button('Change').click();
	flushSync();
	enter(changer('New password'), 'half converted');
	for (const composition of [{ isComposing: true }, { keyCode: 229 }]) {
		expect(returned(changer('New password'), composition).defaultPrevented).toBe(false);
	}
	expect(ipc.setField).not.toHaveBeenCalled();
	expect(changer('New password').value).toBe('half converted');

	return unmount(component);
});

/**
 * Ten recovery codes are replaced in lines. Their Change opened on one line
 * with Return as Save, and the first code typed was saved over all ten. A
 * value in lines - which Rust says, without saying what the lines are - opens
 * its Change four lines tall, where Return starts the next line and Cmd+Return
 * saves.
 */
it('replaces a protected value in lines in a field written in lines', async () => {
	ipc.setField.mockResolvedValue(entry());
	const { component } = pane({
		fields: [
			{ ...PASSWORD, lines: true },
			field({
				name: 'Recovery codes',
				kind: 'custom',
				protected: true,
				value: null,
				empty: false,
				lines: true
			})
		]
	});

	for (const [opener, label] of [
		[button('Change'), 'New password'],
		[icon('Change Recovery codes'), 'New value of Recovery codes']
	] as const) {
		opener.click();
		flushSync();
		const typed = changer(label);
		expect(typed.getAttribute('rows'), label).toBe('4');
		expect(typed.getAttribute('wrap'), label).toBe('soft');
		enter(typed, '0451-7719');
		expect(returned(typed).defaultPrevented, `${label}: Return saved`).toBe(false);
	}
	expect(ipc.setField).not.toHaveBeenCalled();

	returned(changer('New value of Recovery codes'), { metaKey: true });
	await vi.waitFor(() =>
		expect(ipc.setField).toHaveBeenCalledWith(
			expect.any(String),
			'Recovery codes',
			'0451-7719',
			true,
			expect.any(Number)
		)
	);

	return unmount(component);
});

/** A field named to be written in lines is written in lines after its first
 * value too, for as long as the entry is open: it comes back protected, and
 * its next value is written in a Change. */
it('keeps writing a field named to be in lines in lines once it has a value', async () => {
	ipc.setField.mockResolvedValue(entry());
	const { component, props } = deleting({ fields: [PASSWORD] });

	icon('Add a field').click();
	flushSync();
	const name = host.querySelector('[aria-label="The name of the new field"]') as HTMLInputElement;
	name.value = 'Recovery codes';
	button('Multi-line').click();
	flushSync();
	returned(name);
	await vi.waitFor(() => expect(ipc.setField).toHaveBeenCalled());

	props.entry = {
		...props.entry,
		fields: [
			...props.entry.fields,
			field({ name: 'Recovery codes', kind: 'custom', protected: true, value: null, empty: false })
		]
	};
	flushSync();
	icon('Change Recovery codes').click();
	flushSync();

	expect(changer('New value of Recovery codes').getAttribute('rows')).toBe('4');
	enter(changer('New value of Recovery codes'), 'first code');
	expect(returned(changer('New value of Recovery codes')).defaultPrevented).toBe(false);

	return unmount(component);
});

/**
 * Save, Cancel and Escape put the field away, and the focus used to fall to
 * nothing: a keyboard reader lost their place, and the next Escape closed the
 * entry. It goes back to the press that opened the field. A field put away
 * because the reader clicked somewhere else leaves the focus there.
 */
it('gives the focus back to the Change that opened the field', async () => {
	ipc.setField.mockResolvedValue(entry());
	const { component } = pane({
		fields: [
			PASSWORD,
			field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false })
		]
	});

	button('Change').click();
	flushSync();
	enter(changer('New password'), 'never mind');
	changer('New password').dispatchEvent(
		new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })
	);
	flushSync();
	expect(document.activeElement, 'Escape').toBe(button('Change'));

	icon('Change API token').click();
	flushSync();
	button('Cancel').click();
	flushSync();
	expect(document.activeElement, 'Cancel').toBe(icon('Change API token'));

	icon('Change API token').click();
	flushSync();
	enter(changer('New value of API token'), TOKEN);
	button('Save').click();
	await vi.waitFor(() =>
		expect(host.querySelector('[aria-label="New value of API token"]')).toBeNull()
	);
	expect(document.activeElement, 'Save').toBe(icon('Change API token'));

	button('Change').click();
	flushSync();
	const typed = changer('New password');
	typed.blur();
	leave(typed);
	expect(host.querySelector('[aria-label="New password"]')).toBeNull();
	expect(document.activeElement, 'the focus was taken back from elsewhere').not.toBe(
		button('Change')
	);

	return unmount(component);
});

/**
 * The entry coming back changed - an edit to its title, a tag - is not the
 * reader leaving it. A new password being typed, and the question under it,
 * stay where they are; they used to go with every change that landed, and
 * said the password was not saved.
 */
it('keeps a new password being typed when the entry comes back changed', () => {
	const { component, props } = deleting({ fields: [PASSWORD] });

	button('Change').click();
	flushSync();
	enter(changer('New password'), 'half of it');
	leave(changer('New password'));
	props.entry = { ...props.entry, tags: ['changed'] };
	flushSync();

	expect(changer('New password').value).toBe('half of it');
	expect(host.textContent).toContain('Save the new password?');
	expect(props.onFailure).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * A pane taken down while its new password is on its way to the vault has
 * handed the password over, and did not lose it. It used to say "not saved"
 * all the same, and a reader who believed it set yet another password on the
 * website. It says nothing when the save goes through; when it does not, it
 * says plainly that the new password did not go in, and why.
 */
it('says nothing about a new password on its way when its pane goes, unless it is refused', async () => {
	for (const outcome of ['taken', 'refused'] as const) {
		ipc.setField.mockReset();
		const saving = Promise.withResolvers<ReturnType<typeof entry>>();
		ipc.setField.mockReturnValue(saving.promise);
		const { component, props } = deleting({ fields: [PASSWORD] });

		button('Change').click();
		flushSync();
		enter(changer('New password'), 'set on the website');
		returned(changer('New password'));
		expect(ipc.setField).toHaveBeenCalledTimes(1);

		props.entry = entry({ fields: [PASSWORD] });
		flushSync();
		expect(host.querySelector('[aria-label="New password"]')).toBeNull();
		expect(props.onFailure, `${outcome}: said before the answer`).not.toHaveBeenCalled();

		if (outcome === 'taken') {
			saving.resolve(entry());
			await vi.waitFor(() => expect(props.onChanged).toHaveBeenCalled());
			await Promise.resolve();
			expect(props.onFailure, 'a saved password was said to be lost').not.toHaveBeenCalled();
		} else {
			saving.reject({ code: 'noSuchEntry', message: 'there is no such entry in this database' });
			await vi.waitFor(() =>
				expect(props.onFailure).toHaveBeenCalledWith(
					expect.objectContaining({
						message: 'The new password was not saved: there is no such entry in this database.'
					})
				)
			);
			expect(props.onFailure).toHaveBeenCalledTimes(1);
		}
		await unmount(component);
	}
});

/**
 * The Change under a protected field of the reader's own starts where the
 * value starts because it is in the value's column, not because a margin was
 * worked out to match the width of the name: widen the name and the two still
 * start together.
 */
it("puts a field's Change in the column its value is in", () => {
	const { component } = pane({
		fields: [
			field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false })
		]
	});

	const change = icon('Change API token');
	const column = change.parentElement;
	const row = column?.parentElement;
	const [name, value] = [...(row?.children ?? [])];
	expect(name?.textContent?.trim()).toBe('API token');
	expect(value?.contains(icon('Copy API token')), 'the value is not the second column').toBe(true);
	expect(row?.className).toContain('grid-cols-');
	expect(column?.className).toContain('col-start-2');
	for (const offset of [change.className, column?.className ?? '']) {
		expect(offset, 'the Change is pushed into place by a margin').not.toMatch(/\bml-/);
	}

	return unmount(component);
});

/** A name or a tag typed through an input method is finished by the reader's
 * own Return, not by the one that confirms a conversion. */
it('makes no field and adds no tag on the Return that ends a composition', async () => {
	ipc.setField.mockResolvedValue(entry());
	ipc.setTags.mockResolvedValue(entry());
	const { component } = pane({ fields: [PASSWORD] });
	const compositions = [{ isComposing: true }, { keyCode: 229 }];

	icon('Add a field').click();
	flushSync();
	const name = host.querySelector('[aria-label="The name of the new field"]') as HTMLInputElement;
	name.value = 'Passport';
	for (const composition of compositions) {
		expect(returned(name, composition).defaultPrevented).toBe(false);
	}
	name.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
	flushSync();

	button('+ tag').click();
	flushSync();
	await Promise.resolve();
	const tag = host.querySelector('[aria-label="A new tag"]') as HTMLInputElement;
	tag.value = 'travel';
	for (const composition of compositions) {
		expect(returned(tag, composition).defaultPrevented).toBe(false);
	}
	expect(ipc.setField).not.toHaveBeenCalled();
	expect(ipc.setTags).not.toHaveBeenCalled();

	expect(returned(tag).defaultPrevented).toBe(true);
	expect(ipc.setTags).toHaveBeenCalledWith(expect.any(String), ['travel']);

	return unmount(component);
});
