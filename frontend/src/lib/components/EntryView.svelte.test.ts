import { flushSync, mount, unmount, type ComponentProps } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { chosen as picked, forget } from '$lib/context.svelte';
import { attachment, drawing, entry, field, generated, group, kinds } from '$lib/fixtures';
import { typing } from '$lib/keys';
import type { Attached, Clash, Entry, Generated, Purpose, Recipe } from '$lib/model';
import { reactive } from '$lib/props.svelte';
import type { Stubbed } from '$lib/stubbed';
import EntryView from './EntryView.svelte';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('$lib/ipc'), async (real) =>
	Object.assign(ipc, (await import('$lib/stubbed')).stubbed(await real()))
);

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
	ipc.generator.mockResolvedValue(drawing());
	ipc.contextMenu.mockResolvedValue(undefined);
});

afterEach(() => {
	forget();
	host.remove();
	localStorage.clear();
	// A selection one test made is still standing in the next one otherwise,
	// and a stand-in for the selection has to go before it can be cleared.
	vi.restoreAllMocks();
	document.getSelection()?.removeAllRanges();
});

/**
 * What the window hands the pane, with what a test is about put over it: the
 * entry, and the callbacks whose calls it reads. One place, so that a prop the
 * pane comes to need is given a default once rather than at every mount.
 */
function props<Over extends Partial<ComponentProps<typeof EntryView>>>(over: Over) {
	return {
		root: group({ name: 'Root' }),
		path: [group({ name: 'Work' })],
		history: null,
		now: new Date('2026-08-29T14:30:00Z'),
		readOnly: false,
		suggested: kinds().suggested,
		onCopy: vi.fn(),
		onChanged: vi.fn(),
		onVersions: vi.fn(),
		onClose: vi.fn(),
		onDuplicate: vi.fn(),
		onDelete: vi.fn(),
		onPutBack: vi.fn(),
		onMove: vi.fn(),
		onFieldRemoved: vi.fn(),
		onFailure: vi.fn(),
		...over
	};
}

function show(entryOver: Parameters<typeof entry>[0]) {
	return mount(EntryView, {
		target: host,
		props: props({ entry: entry(entryOver) })
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

/** A right-click on `target`, and the menu the window asked Rust for: what it
 * is about, and the number an item chosen from it comes back with. */
function menuOn(target: Element): { event: MouseEvent; serial: number; subject: unknown } {
	const before = ipc.contextMenu.mock.calls.length;
	const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
	target.dispatchEvent(event);
	const call = ipc.contextMenu.mock.calls.at(-1);
	if (ipc.contextMenu.mock.calls.length === before || !call)
		return { event, serial: -1, subject: null };
	return { event, serial: call[0] as number, subject: call[1] };
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
	ipc.generatePassword.mockResolvedValue(generated('Made-Password-123'));
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
		props: props({
			entry: entry({
				attachments: [attachment({ name: '../../escape.txt', fileName: 'escape.txt', size: 12 })]
			}),
			onChanged
		})
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
		props: props({
			entry: entry({ attachments: [attachment({ name: 'id_ed25519' })] }),
			onChanged,
			onVersions,
			onFailure
		})
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
		props: props({
			entry: entry({
				fields: [
					field({ name: 'Password', kind: 'password', value: null, empty: false }),
					field({ name: 'Notes', kind: 'notes', value: 'root access', empty: false })
				]
			}),
			onFailure
		})
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
	const handed = reactive(
		props({ entry: entry({ attachments: [attachment({ name: 'id_ed25519' })] }) })
	);
	const component = mount(EntryView, { target: host, props: handed });
	flushSync();

	host.querySelector<HTMLButtonElement>('[aria-label="Remove id_ed25519"]')?.click();
	flushSync();
	button('Remove').click();
	await vi.waitFor(() => expect(host.textContent).toContain('2 earlier versions still hold'));
	flushSync();

	handed.entry = entry({ attachments: [attachment({ name: 'other.pem' })] });
	flushSync();

	expect(host.textContent).not.toContain('2 earlier versions still hold');

	return unmount(component);
});

/** A database Coffer will not write back has values to read and copy, and
 * nothing at all to change. */
it('offers no change on a database it cannot write', () => {
	const component = mount(EntryView, {
		target: host,
		props: props({
			entry: entry({
				fields: [
					field({ name: 'Title', kind: 'title', value: 'node-3', empty: false }),
					field({ name: 'Password', kind: 'password', value: null, empty: false })
				],
				tags: ['prod'],
				attachments: [attachment({ name: 'id_ed25519' })]
			}),
			readOnly: true
		})
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
it('offers a way out of the entry and a copy of it, and neither is the way to delete one', () => {
	const onClose = vi.fn();
	const onDelete = vi.fn();
	const onDuplicate = vi.fn();
	const component = mount(EntryView, {
		target: host,
		props: props({
			entry: entry({ fields: [field({ name: 'Title', kind: 'title', value: 'node-3' })] }),
			onClose,
			onDelete,
			onDuplicate
		})
	});
	flushSync();

	const header = host.querySelector('header');
	// The line above the title opens a list of folders, which moves the entry
	// and loses nothing; Duplicate makes another, which loses nothing either.
	const buttons = [...(header?.querySelectorAll('button:not([aria-haspopup])') ?? [])].map(
		(each) => each.getAttribute('aria-label') ?? each.textContent?.trim()
	);
	expect(buttons, 'something that loses the entry is back beside the close').toEqual([
		'Duplicate',
		'Close this entry'
	]);
	const copy = button('Duplicate');
	expect(copy.title).toBe('Duplicate · ⌘D');
	expect(copy.getAttribute('aria-keyshortcuts')).toBe('Meta+D');
	copy.click();
	flushSync();
	expect(onDuplicate).toHaveBeenCalledTimes(1);
	expect(onDelete, 'the copy deleted the entry').not.toHaveBeenCalled();

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
		props: props({
			entry: entry({ fields: [field({ name: 'Title', kind: 'title', value: 'node-3' })] }),
			readOnly: true,
			onClose
		})
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
		props: props({
			entry: entry({ attachments: [attachment({ name: 'id_ed25519' })] }),
			onChanged,
			onFailure
		})
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
 * A file name from another client's database can hold a right-to-left
 * override. Bare in the question, it would turn the rest of it around - both
 * sizes and the warning the reader has to read to choose - so each name is
 * isolated, and the sentence after it reads as written.
 */
it('keeps a right-to-left override in a file name off the rest of the question', async () => {
	const { component } = await asked({ name: 'evil\u202efdp.exe', free: 'evil\u202efdp 2.exe' });

	expect(question()).toContain(
		'This entry already has “\u2068evil\u202efdp.exe\u2069” (1.2 MB). Keep both to add the new one (840 KB) as “\u2068evil\u202efdp 2.exe\u2069”. A replaced file is not kept in Versions'
	);

	return unmount(component);
});

/**
 * Every phone calls every scan the same thing, and the second page of a
 * passport used to take the place of the first without a word. Now nothing is
 * on the entry until the reader has been asked, in sizes as well as names, and
 * the answer that loses nothing is the one the focus is on.
 */
it('asks before a file goes on under a name the entry already has', async () => {
	const { component, onChanged, onFailure } = await asked();

	expect(question()).toContain(`This entry already has “\u2068${SCAN}\u2069” (1.2 MB).`);
	expect(question()).toContain(
		'Keep both to add the new one (840 KB) as “\u2068Scanned Document 2.pdf\u2069”.'
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

	expect(question()).toContain(
		`“\u2068${SCAN}\u2069” that is here in place, so it can’t be replaced.`
	);
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
	const handed = reactive(props({ entry: first }));
	const component = mount(EntryView, { target: host, props: handed });
	flushSync();

	icon('Add a file').click();
	await vi.waitFor(() => expect(host.querySelector('[data-confirm]')).not.toBeNull());
	button('Replace').click();
	await vi.waitFor(() => expect(question()).toContain('Earlier versions are holding'));

	// The file there, and the versions holding it, taken off by its own trash.
	handed.entry = { ...first, attachments: [] };
	flushSync();

	expect(question()).toContain(`“\u2068${SCAN}\u2069” is no longer on this entry`);
	const answers = [...host.querySelectorAll('[data-confirm] button')];
	expect(answers.map((each) => each.textContent?.trim())).toEqual(['Don’t add it', 'Add it']);

	button('Add it').click();
	await vi.waitFor(() => expect(handed.onChanged).toHaveBeenCalledWith(only));
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
	const handed = reactive(props({ entry: first }));
	const component = mount(EntryView, { target: host, props: handed });
	flushSync();

	icon('Add a file').click();
	await vi.waitFor(() => expect(host.querySelector('[data-confirm]')).not.toBeNull());

	handed.entry = entry({ attachments: [attachment({ name: SCAN })] });
	flushSync();

	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(ipc.withdrawAttachment).toHaveBeenCalledTimes(1);
	expect(ipc.withdrawAttachment).toHaveBeenCalledWith(first.id);

	// And a pane that goes away altogether, which is how a lock and a close
	// both look from here.
	icon('Add a file').click();
	await vi.waitFor(() => expect(host.querySelector('[data-confirm]')).not.toBeNull());
	const second = handed.entry.id;
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
	const handed = reactive(props({ entry: first }));
	const component = mount(EntryView, { target: host, props: handed });
	flushSync();

	icon('Add a file').click();
	await vi.waitFor(() => expect(host.querySelector('[data-confirm]')).not.toBeNull());

	handed.entry = { ...first, fields: [field({ name: 'Title', value: 'Passport', empty: false })] };
	flushSync();

	expect(question()).toContain(`This entry already has “\u2068${SCAN}\u2069”`);
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
	const handed = reactive(props({ entry: first }));
	const component = mount(EntryView, { target: host, props: handed });
	flushSync();

	icon('Add a file').click();
	handed.entry = entry({ attachments: [attachment({ name: SCAN })] });
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
		props: props({
			entry: entry({
				fields: [
					field({ name: 'API token', kind: 'custom', protected: true, value: null, empty: false })
				]
			}),
			onCopy
		})
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
	expect(host.textContent).toContain('Save the new value of “\u2068API token\u2069”?');
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
 * selection dragged out of the window goes wherever it is dropped. On a
 * revealed value, wherever in the pane it is, the menu is Coffer's - Copy,
 * through Rust, and Hide - and the drag does not start. What Rust is asked
 * for names the field, and holds nothing of the value.
 */
it('asks for Coffer’s menu and never WebKit’s on a revealed value, and sends none of it', async () => {
	const {
		component,
		entry: shown,
		onCopy
	} = pane({
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

	const names = ['UserName', 'Password', 'Notes', 'PIN'];
	for (const [at, node] of nodes.entries()) {
		const { event, subject } = menuOn(node);
		expect(event.defaultPrevented, `${names[at]}: WebKit's menu`).toBe(true);
		expect(subject).toEqual({ kind: 'value', entry: shown.id, field: names[at], range: null });
		const drag = new MouseEvent('dragstart', { bubbles: true, cancelable: true });
		node.dispatchEvent(drag);
		expect(drag.defaultPrevented, `${names[at]}: a drag`).toBe(true);
	}
	expect(ipc.contextMenu).toHaveBeenCalledTimes(4);
	expect(JSON.stringify(ipc.contextMenu.mock.calls)).not.toContain(SECRET);

	// Copy from the menu over the password: that field, through Rust.
	const copying = menuOn(nodes[1]);
	picked(copying.serial, { item: 'copyValue', entry: shown.id, field: 'Password', range: null });
	expect(onCopy).toHaveBeenLastCalledWith(shown.id, 'Password', null);

	// Hide from the menu over the PIN takes that value off the screen, and no
	// other.
	const hiding = menuOn(nodes[3]);
	picked(hiding.serial, { item: 'hideField', entry: shown.id, field: 'PIN' });
	flushSync();
	expect(nodes[3].textContent).toBe('');
	expect(nodes.slice(0, 3).map((node) => node.textContent)).toEqual([SECRET, SECRET, SECRET]);

	return unmount(component);
});

/** Every item of a hidden field's menu is one of the row's own buttons: the
 * eye, the copy, Change, Make one and the trash, which takes the field off the
 * way its trash does. */
it('answers a hidden field’s menu with the row’s own buttons', async () => {
	ipc.removeField.mockResolvedValue(entry());
	const {
		component,
		entry: shown,
		onCopy
	} = pane({
		fields: [field({ name: 'PIN', kind: 'custom', protected: true, value: null, empty: false })]
	});
	const name = icon('Rename PIN');

	let menu = menuOn(name);
	expect(menu.event.defaultPrevented).toBe(true);
	expect(menu.subject).toEqual({ kind: 'field', entry: shown.id, field: 'PIN', shown: false });
	picked(menu.serial, { item: 'showField', entry: shown.id, field: 'PIN' });
	await vi.waitFor(() => expect(ipc.reveal).toHaveBeenCalledWith(shown.id, 'PIN'));
	flushSync();
	expect(menuOn(name).subject, 'Hide is offered once it is shown').toMatchObject({ shown: true });

	menu = menuOn(name);
	picked(menu.serial, { item: 'copyField', entry: shown.id, field: 'PIN' });
	expect(onCopy).toHaveBeenLastCalledWith(shown.id, 'PIN', null);

	menu = menuOn(name);
	picked(menu.serial, { item: 'changeField', entry: shown.id, field: 'PIN' });
	flushSync();
	expect(changer('New value of PIN')).not.toBeNull();

	menu = menuOn(changer('New value of PIN'));
	expect(menu.event.defaultPrevented, 'the Change field lost WebKit’s menu').toBe(false);
	expect(menu.serial, 'the Change field is the reader’s typing').toBe(-1);

	return unmount(component);
});

it('makes a value for a hidden field and takes it off from the row’s menu', async () => {
	ipc.removeField.mockResolvedValue(entry());
	const { component, entry: shown } = pane({
		fields: [field({ name: 'PIN', kind: 'custom', protected: true, value: null, empty: false })]
	});

	let menu = menuOn(icon('Rename PIN'));
	picked(menu.serial, { item: 'makeOne', entry: shown.id, field: 'PIN' });
	flushSync();
	expect(host.querySelector('[aria-label="Generator for PIN"]')).not.toBeNull();

	menu = menuOn(icon('Keep PIN hidden'));
	picked(menu.serial, { item: 'removeField', entry: shown.id, field: 'PIN' });
	await vi.waitFor(() => expect(ipc.removeField).toHaveBeenCalledWith(shown.id, 'PIN', false));

	return unmount(component);
});

/** The password's row is its name, its value and its buttons, and its menu
 * offers what the buttons do. Its Change field is the reader's typing, and
 * keeps WebKit's menu for it. */
it('answers the password row’s menu with Show, Copy, Change and Make one', async () => {
	const {
		component,
		entry: shown,
		onCopy
	} = pane({
		fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })]
	});
	const label = [...host.querySelectorAll('span')].find(
		(each) => each.textContent.trim() === 'Password'
	);
	if (!label) throw new Error('the password row has no name');

	let menu = menuOn(label);
	expect(menu.subject).toEqual({ kind: 'field', entry: shown.id, field: 'Password', shown: false });
	picked(menu.serial, { item: 'showField', entry: shown.id, field: 'Password' });
	await vi.waitFor(() => expect(value()).toBe(SECRET));

	menu = menuOn(button('Hide'));
	expect(menu.subject).toMatchObject({ shown: true });
	picked(menu.serial, { item: 'hideField', entry: shown.id, field: 'Password' });
	flushSync();
	expect(value()).toBe('');

	menu = menuOn(button('Copy'));
	picked(menu.serial, { item: 'copyField', entry: shown.id, field: 'Password' });
	expect(onCopy).toHaveBeenLastCalledWith(shown.id, 'Password', null);

	menu = menuOn(label);
	picked(menu.serial, { item: 'makeOne', entry: shown.id, field: 'Password' });
	flushSync();
	expect(host.querySelector('[aria-label="Password generator"]')).not.toBeNull();

	menu = menuOn(label);
	picked(menu.serial, { item: 'changeField', entry: shown.id, field: 'Password' });
	flushSync();
	const typing = changer('New password');
	const typed = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
	const asked = ipc.contextMenu.mock.calls.length;
	typing.dispatchEvent(typed);
	expect(typed.defaultPrevented, 'the Change field lost WebKit’s menu').toBe(false);
	expect(ipc.contextMenu).toHaveBeenCalledTimes(asked);

	return unmount(component);
});

/** A file's menu is its two buttons: the save panel, and the question in its
 * row before it goes. Nothing is removed from the menu alone. */
it('saves a file from its menu through the panel, and asks before removing it', () => {
	ipc.exportAttachment.mockResolvedValue(undefined);
	const { component, entry: shown } = pane({
		attachments: [attachment({ name: '../../escape.txt', fileName: 'escape.txt' })]
	});
	const card = icon('Remove ../../escape.txt').closest('[role="presentation"]');
	if (!card) throw new Error('the file has no row');

	let menu = menuOn(card);
	flushSync();
	expect(menu.subject).toEqual({ kind: 'file', entry: shown.id, name: '../../escape.txt' });
	expect(card.hasAttribute('data-menu'), 'the row is marked while the menu is open').toBe(true);
	picked(menu.serial, { item: 'saveFile', entry: shown.id, name: '../../escape.txt' });
	expect(ipc.exportAttachment).toHaveBeenCalledWith(shown.id, '../../escape.txt');

	menu = menuOn(card);
	picked(menu.serial, { item: 'removeFile', entry: shown.id, name: '../../escape.txt' });
	flushSync();
	expect(host.querySelector('[data-confirm]')?.textContent).toContain('can’t be undone');
	expect(ipc.removeAttachment).not.toHaveBeenCalled();

	return unmount(component);
});

/** The menu was about a field of one entry, and the pane is on another by the
 * time the item comes - with a field of the same name. Nothing happens to
 * either. The row of a field of the reader's own is drawn again for the next
 * entry, so the row the menu was asked on has left the screen and that is
 * what drops its item (`chosen` in `context.svelte.ts`); the password's row
 * and a file's stay, and their answers compare the ids (`about`). */
it('changes nothing from an item chosen for an entry the pane no longer shows', () => {
	const first = entry({
		fields: [field({ name: 'PIN', kind: 'custom', protected: true, value: null, empty: false })],
		attachments: [attachment({ name: 'scan.pdf' })]
	});
	const handed = reactive(props({ entry: first }));
	const component = mount(EntryView, { target: host, props: handed });
	flushSync();
	const own = menuOn(icon('Rename PIN'));
	const label = [...host.querySelectorAll('span')].find(
		(each) => each.textContent.trim() === 'Password'
	);
	if (!label) throw new Error('the password row has no name');

	handed.entry = entry({
		fields: [field({ name: 'PIN', kind: 'custom', protected: true, value: null, empty: false })],
		attachments: [attachment({ name: 'scan.pdf' })]
	});
	flushSync();
	picked(own.serial, { item: 'removeField', entry: first.id, field: 'PIN' });
	const password = menuOn(label);
	picked(password.serial, { item: 'changeField', entry: first.id, field: 'Password' });
	const card = icon('Remove scan.pdf').closest('[role="presentation"]');
	if (!card) throw new Error('the file has no row');
	const file = menuOn(card);
	picked(file.serial, { item: 'removeFile', entry: first.id, name: 'scan.pdf' });
	flushSync();

	expect(ipc.removeField).not.toHaveBeenCalled();
	expect(host.querySelector('textarea[aria-label="New password"]')).toBeNull();
	expect(host.querySelector('[data-confirm]')).toBeNull();

	return unmount(component);
});

/** The question that deletes an entry for good is put on its bin card while
 * it is in the bin, and at the foot of the pane while it is not. An entry put
 * back under the card's question is asked about nowhere: at the foot the
 * question would stand over a button that moves the entry to the bin. */
it('drops the bin card’s question when the entry leaves the bin under it', () => {
	const binned = entry({
		fields: titled('Bank'),
		binned: { since: '2026-08-27T10:00:00Z', within: null, from: null },
		deletion: 'forever'
	});
	const handed = reactive(props({ entry: binned }));
	const component = mount(EntryView, { target: host, props: handed });
	flushSync();
	button('Delete forever…').click();
	flushSync();
	expect(host.querySelector('[data-binned] [data-confirm]')).not.toBeNull();

	handed.entry = { ...binned, binned: null, deletion: 'bin' };
	flushSync();
	expect(host.querySelector('[data-binned]')).toBeNull();
	expect(host.querySelector('[data-confirm]'), 'asked at the foot').toBeNull();
	button('Move to Recycle Bin').click();
	expect(handed.onDelete).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/** A vault Coffer will not write back still shows its menus - a value is
 * still copied and shown - and nothing chosen from one changes anything. */
it('changes nothing from a menu in a pane that cannot be written', () => {
	const { component, entry: shown } = pane(
		{
			fields: [field({ name: 'Password', kind: 'password', value: null, empty: false })],
			attachments: [attachment({ name: 'scan.pdf' })]
		},
		true
	);
	const card = [...host.querySelectorAll('[role="presentation"]')].find((each) =>
		each.textContent?.includes('scan.pdf')
	);
	if (!card) throw new Error('the file has no row');
	const label = [...host.querySelectorAll('span')].find(
		(each) => each.textContent.trim() === 'Password'
	);
	if (!label) throw new Error('the password row has no name');

	const file = menuOn(card);
	expect(file.event.defaultPrevented).toBe(true);
	picked(file.serial, { item: 'removeFile', entry: shown.id, name: 'scan.pdf' });
	const password = menuOn(label);
	picked(password.serial, { item: 'changeField', entry: shown.id, field: 'Password' });
	flushSync();

	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(host.querySelector('textarea[aria-label="New password"]')).toBeNull();

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
	const handed = props({ entry: entry(over), readOnly });
	const component = mount(EntryView, { target: host, props: handed });
	flushSync();
	return { component, ...handed };
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
		'Remove “\u2068passport.pdf\u2069” (1.2 MB)? Files are not kept in Versions, so this can’t be undone.'
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
		'Remove “\u2068../scan.pdf\u2069” (2.0 KB)?'
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
	const handed = reactive(
		props({ entry: entry({ attachments: [attachment({ name: 'id_ed25519' })] }) })
	);
	const component = mount(EntryView, { target: host, props: handed });
	flushSync();

	icon('Remove id_ed25519').click();
	flushSync();
	expect(host.querySelector('[data-confirm]')).not.toBeNull();

	handed.entry = entry({ attachments: [attachment({ name: 'id_ed25519' })] });
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
	const handed = props({
		entry: entry({
			fields: [field({ name: 'PIN', kind: 'custom', protected: true, value: null, empty: false })]
		}),
		onChanged
	});
	const component = mount(EntryView, { target: host, props: handed });
	flushSync();

	icon('Remove the field PIN').click();
	await vi.waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));
	expect(ipc.removeField).toHaveBeenCalledWith(handed.entry.id, 'PIN', false);
	expect(handed.onFieldRemoved, 'the window heard before the save was done').not.toHaveBeenCalled();

	settle();
	await vi.waitFor(() =>
		expect(handed.onFieldRemoved).toHaveBeenCalledWith(handed.entry.id, 'PIN', false)
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
		'Remove “\u2068PIN\u2069”? This vault keeps no version to bring it back from, so this can’t be undone.'
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
	const handed = reactive(props({ entry: entry({ fields: [pin] }) }));
	const component = mount(EntryView, { target: host, props: handed });
	flushSync();

	icon('Remove the field PIN').click();
	handed.entry = entry({ fields: [pin] });
	flushSync();
	refusing.reject(FOR_GOOD);
	await new Promise((resolve) => setTimeout(resolve, 0));
	flushSync();

	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(handed.onFailure).not.toHaveBeenCalled();
	expect(handed.onFieldRemoved).not.toHaveBeenCalled();

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
	// Before the trash, on every row, the copy. The lock that hides the value
	// or shows it is in the name's column, before the name: in the value's row
	// it took the width a revealed value is read in.
	for (const name of ['API token', 'Port']) {
		const trash = icon(`Remove the field ${name}`);
		expect(trash.previousElementSibling, `${name}: the copy is not beside the trash`).toBe(
			icon(`Copy ${name}`)
		);
		const lock = icon(`Keep ${name} hidden`);
		expect(trash.parentElement?.contains(lock), `${name}: the lock is in the value's row`).toBe(
			false
		);
		const column = lock.parentElement;
		expect(column?.firstElementChild, `${name}: the lock is not first in its column`).toBe(lock);
		expect(lock.nextElementSibling).toBe(icon(`Rename ${name}`));
		expect(column?.className).toContain('w-24');
	}

	return unmount(component);
});

/** The pane on an entry, with the tree around it that names folders. */
function deleting(over: Parameters<typeof entry>[0], readOnly = false, root = group()) {
	const handed = reactive(props({ entry: entry(over), root, readOnly }));
	const component = mount(EntryView, { target: host, props: handed });
	flushSync();
	return { component, props: handed };
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
		'Delete “\u2068Bank\u2069” forever? This can’t be undone.'
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
	const binned = { since: null, within: null, from: null };
	const { component, props } = deleting({ fields: titled('Bank'), binned, deletion: 'forever' });
	button('Delete forever…').click();
	flushSync();
	expect(host.querySelector('[data-confirm]')?.textContent).toContain('“\u2068Bank\u2069”');

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
			binned: { since: '2026-08-27T10:00:00Z', within: null, from: personal.id },
			deletion: 'forever'
		},
		false,
		tree
	);

	expect(host.querySelector('[data-binned]')?.textContent).toContain(
		'In the Recycle Bin since 27 Aug · was in “\u2068Personal\u2069”'
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
		'Delete “\u2068Bank\u2069” forever? This can’t be undone.'
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
				binned: { since: '2026-08-27T10:00:00Z', within: null, from },
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
			binned: { since: null, within: null, from: hostile.id },
			deletion: 'forever'
		},
		false,
		group({ sections: [hostile] })
	);

	expect(host.querySelector('img')).toBeNull();
	expect(host.querySelector('[data-binned]')?.textContent).toContain(
		'In the Recycle Bin · was in “\u2068<img src=x onerror="alert(1)">\u2069”'
	);

	return unmount(component);
});

/** A database Coffer will not write back says what is in its bin and offers
 * nothing it would only refuse. */
it('offers no way out of the bin on a database it cannot write', () => {
	const { component } = deleting(
		{
			fields: titled('Bank'),
			binned: { since: '2026-08-27T10:00:00Z', within: null, from: personal.id },
			deletion: 'forever'
		},
		true,
		tree
	);

	expect(host.querySelector('[data-binned]')?.textContent).toContain(
		'was in “\u2068Personal\u2069”'
	);
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
 * The Escape that cancels a conversion is the input method's too, and arrives
 * the same way. Read as the field's, it closed the Change and threw away
 * everything typed into it.
 */
it('keeps a new value on the Escape that cancels a conversion', () => {
	const { component } = pane({ fields: [PASSWORD] });

	button('Change').click();
	flushSync();
	enter(changer('New password'), 'half converted');
	for (const composition of [{ isComposing: true }, { keyCode: 229 }]) {
		returned(changer('New password'), { key: 'Escape', ...composition });
		flushSync();
		expect(changer('New password').value, JSON.stringify(composition)).toBe('half converted');
	}

	returned(changer('New password'), { key: 'Escape' });
	flushSync();
	expect(host.querySelector('[aria-label="New password"]')).toBeNull();
	expect(ipc.setField).not.toHaveBeenCalled();

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
	const column = change.closest('.col-start-2');
	const row = column?.parentElement;
	const [name, value] = [...(row?.children ?? [])];
	expect(name?.textContent?.trim()).toBe('API token');
	expect(value?.contains(icon('Copy API token')), 'the value is not the second column').toBe(true);
	expect(row?.className).toContain('grid-cols-');
	expect(column?.className).toContain('col-start-2');
	for (const offset of [
		change.className,
		change.parentElement?.className ?? '',
		column?.className ?? ''
	]) {
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

/** The field the name of a new field is typed in. */
function named(): HTMLInputElement {
	const found = host.querySelector<HTMLInputElement>(
		'input[aria-label="The name of the new field"]'
	);
	if (!found) throw new Error('no name is being asked for');
	return found;
}

/** The field a new name for one of the reader's own fields is typed in. */
function renaming(name: string): HTMLInputElement {
	const found = host.querySelector<HTMLInputElement>(`input[aria-label="New name for ${name}"]`);
	if (!found) throw new Error(`no new name is being asked for ${name}`);
	return found;
}

/** The reader's own fields by name, in the order the pane draws them. */
function drawnOrder(): string[] {
	return [...host.querySelectorAll<HTMLButtonElement>('button[aria-label^="Rename "]')].map(
		(each) => each.title
	);
}

/**
 * The pane with a window behind it that does what the vault screen does with
 * a change: the entry Rust answered with is drawn at once, and the save after
 * it takes a moment longer.
 */
function landing(over: Parameters<typeof entry>[0]) {
	const opened = deleting(over);
	opened.props.onChanged.mockImplementation(async (changed: Entry) => {
		opened.props.entry = changed;
		await new Promise((resolve) => setTimeout(resolve, 0));
	});
	return opened;
}

/** A field of the reader's own the database keeps hidden, with a value in it. */
const hiddenField = (name: string) =>
	field({ name, kind: 'custom', protected: true, value: null, empty: false });

/**
 * A field somebody adds to a password entry is far more often a secret than
 * not, so a new one is hidden unless the reader says otherwise while naming
 * it. The pill that says so is pressed without leaving the name, so pressing
 * it never makes a field with half a name; and the choice is for that one
 * field, so the next starts hidden again, on this entry or the next one.
 */
it('names a new field hidden unless the reader turns Hidden off, for that field alone', async () => {
	ipc.setField.mockReset();
	ipc.setField.mockResolvedValue(entry());
	const { component, props } = deleting({ fields: [PASSWORD] });

	icon('Add a field').click();
	flushSync();
	await Promise.resolve();
	const name = named();
	expect(document.activeElement).toBe(name);
	const pill = button('Hidden');
	expect(pill.getAttribute('aria-pressed'), 'a new field starts in the open').toBe('true');
	expect(pill.nextElementSibling).toBe(button('Multi-line'));

	const press = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
	pill.dispatchEvent(press);
	expect(press.defaultPrevented, 'the pill takes the focus off the name').toBe(true);
	// Tabbing from the name onto the pill is not leaving the two of them.
	pill.focus();
	flushSync();
	pill.click();
	flushSync();
	expect(pill.getAttribute('aria-pressed')).toBe('false');
	expect(ipc.setField, 'the pill finished the name').not.toHaveBeenCalled();
	expect(named()).toBe(name);

	name.focus();
	name.value = 'Card expiry';
	returned(name);
	flushSync();
	expect(ipc.setField).toHaveBeenLastCalledWith(
		props.entry.id,
		'Card expiry',
		'',
		false,
		expect.any(Number)
	);

	icon('Add a field').click();
	flushSync();
	expect(button('Hidden').getAttribute('aria-pressed'), 'the last choice outlived its field').toBe(
		'true'
	);
	named().value = 'CVC';
	returned(named());
	flushSync();
	expect(ipc.setField).toHaveBeenLastCalledWith(
		props.entry.id,
		'CVC',
		'',
		true,
		expect.any(Number)
	);

	// Turned off, and another entry shown before the name was finished.
	icon('Add a field').click();
	flushSync();
	button('Hidden').click();
	flushSync();
	props.entry = entry({ fields: [PASSWORD] });
	flushSync();
	expect(host.querySelector('[aria-label="The name of the new field"]')).toBeNull();
	icon('Add a field').click();
	flushSync();
	expect(button('Hidden').getAttribute('aria-pressed'), 'a choice crossed to another entry').toBe(
		'true'
	);
	named().value = 'PIN';
	returned(named());
	flushSync();
	expect(ipc.setField).toHaveBeenLastCalledWith(
		props.entry.id,
		'PIN',
		'',
		true,
		expect.any(Number)
	);
	expect(ipc.setField).toHaveBeenCalledTimes(3);

	return unmount(component);
});

/**
 * Leaving the pill for somewhere else is leaving the name, and the field is
 * made the way the pill says at that moment - not the way it said when the
 * row opened.
 */
it('makes the field the way the pill says when the reader leaves the pill for elsewhere', async () => {
	ipc.setField.mockReset();
	ipc.setField.mockResolvedValue(entry());
	const { component, props } = deleting({ fields: [PASSWORD] });

	icon('Add a field').click();
	flushSync();
	await Promise.resolve();
	named().value = 'Card expiry';
	const pill = button('Hidden');
	pill.focus();
	pill.click();
	flushSync();
	expect(ipc.setField).not.toHaveBeenCalled();

	icon('Add a file').focus();
	flushSync();
	expect(ipc.setField).toHaveBeenCalledTimes(1);
	expect(ipc.setField).toHaveBeenCalledWith(
		props.entry.id,
		'Card expiry',
		'',
		false,
		expect.any(Number)
	);
	expect(host.querySelector('[aria-label="The name of the new field"]')).toBeNull();

	return unmount(component);
});

/**
 * A row put away without a name - Escape, or the plus pressed again - made no
 * field, so the choice made in it was about nothing. The next field is named
 * from the start again: hidden, on one line. Carried over, it made the next
 * field, typically a PIN, in the open with nobody having said so.
 */
it('starts the next new field hidden after a row was put away with Hidden off', () => {
	ipc.setField.mockReset();
	ipc.setField.mockResolvedValue(entry());

	for (const way of ['Escape', 'the plus'] as const) {
		const { component, props } = deleting({ fields: [PASSWORD] });
		icon('Add a field').click();
		flushSync();
		button('Hidden').click();
		button('Multi-line').click();
		flushSync();
		named().value = 'Card expiry';
		if (way === 'Escape') {
			named().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
		} else {
			icon('Never mind the new field').click();
		}
		flushSync();
		expect(host.querySelector('[aria-label="The name of the new field"]'), way).toBeNull();
		expect(ipc.setField, `${way} made a field`).not.toHaveBeenCalled();

		icon('Add a field').click();
		flushSync();
		expect
			.soft(button('Hidden').getAttribute('aria-pressed'), `${way}: Hidden was left off`)
			.toBe('true');
		expect
			.soft(button('Multi-line').getAttribute('aria-pressed'), `${way}: Multi-line was left on`)
			.toBe('false');
		named().value = 'PIN';
		returned(named());
		flushSync();
		expect
			.soft(ipc.setField, `${way}: the next field was made in the open`)
			.toHaveBeenCalledWith(props.entry.id, 'PIN', '', true, expect.any(Number));
		expect(props.onFailure).not.toHaveBeenCalled();

		unmount(component);
		ipc.setField.mockClear();
	}
});

/**
 * The row goes other ways than Escape and the plus without making anything: a
 * name finished empty, by Return or by leaving it; a name the entry already
 * has, which is refused; the pane moving to another entry. Each is as much
 * the end of what was chosen in it. Multi-line left on is worth as much as
 * Hidden left off: the next field named is written in lines nobody asked for,
 * and its Change takes Return as a new line rather than as Save.
 */
it('starts the next new field hidden and on one line however the last row went without a field', () => {
	ipc.setField.mockReset();
	ipc.setField.mockResolvedValue(entry());

	for (const way of [
		'Return on nothing',
		'leaving nothing',
		'a name the entry has',
		'another entry'
	] as const) {
		const { component, props } = deleting({ fields: [PASSWORD] });
		icon('Add a field').click();
		flushSync();
		button('Hidden').click();
		button('Multi-line').click();
		flushSync();
		if (way === 'Return on nothing') {
			named().value = '   ';
			returned(named());
		} else if (way === 'leaving nothing') {
			named().value = '';
			leave(named(), icon('Add a file'));
		} else if (way === 'a name the entry has') {
			named().value = 'Password';
			returned(named());
		} else {
			props.entry = entry({ fields: [PASSWORD] });
		}
		flushSync();
		expect(host.querySelector('[aria-label="The name of the new field"]'), way).toBeNull();
		expect(ipc.setField, `${way} made a field`).not.toHaveBeenCalled();
		if (way === 'a name the entry has') {
			expect(props.onFailure).toHaveBeenCalledWith(expect.objectContaining({ code: 'refused' }));
		} else {
			expect(props.onFailure).not.toHaveBeenCalled();
		}

		icon('Add a field').click();
		flushSync();
		expect
			.soft(button('Hidden').getAttribute('aria-pressed'), `${way}: Hidden was left off`)
			.toBe('true');
		expect
			.soft(button('Multi-line').getAttribute('aria-pressed'), `${way}: Multi-line was left on`)
			.toBe('false');
		named().value = 'PIN';
		returned(named());
		flushSync();
		expect
			.soft(ipc.setField, `${way}: the next field was made in the open`)
			.toHaveBeenCalledWith(props.entry.id, 'PIN', '', true, expect.any(Number));

		props.entry = { ...props.entry, fields: [PASSWORD, hiddenField('PIN')] };
		flushSync();
		icon('Change PIN').click();
		flushSync();
		expect
			.soft(changer('New value of PIN').getAttribute('rows'), `${way}: the next field is in lines`)
			.toBe('1');

		changer('New value of PIN').dispatchEvent(
			new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })
		);
		unmount(component);
		ipc.setField.mockClear();
	}
});

/**
 * A field named with Return is one the reader is about to give a value, so
 * the value takes the focus when the field arrives. Once: the value drawn
 * again later - hidden, then shown - does not take it back from wherever the
 * reader has put it since.
 */
it('puts the reader in the value of a field named with Return, and only once', async () => {
	ipc.setField.mockReset();
	const { component, props } = landing({ fields: [PASSWORD] });
	const made: Entry = {
		...props.entry,
		fields: [
			PASSWORD,
			field({ name: 'Door code', kind: 'custom', protected: true, value: null, empty: true })
		]
	};
	ipc.setField.mockResolvedValue(made);

	icon('Add a field').click();
	flushSync();
	await Promise.resolve();
	named().value = 'Door code';
	returned(named());

	await vi.waitFor(() =>
		expect(document.activeElement).toBe(host.querySelector('textarea[aria-label="Door code"]'))
	);
	expect(ipc.setField).toHaveBeenCalledWith(made.id, 'Door code', '', true, expect.any(Number));
	await vi.waitFor(() => expect(props.onChanged).toHaveBeenCalledTimes(1));
	await new Promise((resolve) => setTimeout(resolve, 0));

	const tags = button('+ tag');
	tags.focus();
	props.entry = { ...made, fields: [PASSWORD, hiddenField('Door code')] };
	flushSync();
	props.entry = {
		...made,
		fields: [PASSWORD, field({ name: 'Door code', kind: 'custom', value: '4711', empty: false })]
	};
	flushSync();
	expect(host.querySelector('textarea[aria-label="Door code"]')).not.toBeNull();
	expect(document.activeElement, 'the value took the focus a second time').toBe(tags);
	expect(props.onFailure).not.toHaveBeenCalled();

	return unmount(component);
});

/** A name finished by clicking somewhere else leaves the focus where the
 * reader clicked, when the field arrives as much as before. */
it('leaves the focus where the reader clicked when a new name is finished elsewhere', async () => {
	ipc.setField.mockReset();
	const { component, props } = landing({ fields: [PASSWORD] });
	ipc.setField.mockResolvedValue({
		...props.entry,
		fields: [
			PASSWORD,
			field({ name: 'Door code', kind: 'custom', protected: true, value: null, empty: true })
		]
	});

	icon('Add a field').click();
	flushSync();
	await Promise.resolve();
	named().value = 'Door code';
	const elsewhere = icon('Add a file');
	elsewhere.focus();
	flushSync();

	await vi.waitFor(() => expect(props.onChanged).toHaveBeenCalledTimes(1));
	flushSync();
	expect(ipc.setField).toHaveBeenCalledWith(
		props.entry.id,
		'Door code',
		'',
		true,
		expect.any(Number)
	);
	expect(host.querySelector('textarea[aria-label="Door code"]')).not.toBeNull();
	expect(document.activeElement, 'the new field took the focus from the click').toBe(elsewhere);

	return unmount(component);
});

/**
 * Rust can be slow to answer - a save of the last change still running - and
 * the reader does not wait for it. Named with Return and then gone on to type
 * the login, they keep typing the login when the new field arrives: taking
 * the focus there would write half a login and put the rest of it into the
 * new field.
 */
it('leaves the focus in a field the reader went on to type in before the new one arrived', async () => {
	const writing = Promise.withResolvers<Entry>();
	const { component, props } = landing({
		fields: [PASSWORD, field({ name: 'UserName', kind: 'username', value: 'alice', empty: false })]
	});
	const made: Entry = {
		...props.entry,
		fields: [
			...props.entry.fields,
			field({ name: 'Door code', kind: 'custom', protected: true, value: null, empty: true })
		]
	};
	// Any write after the new field's is answered with the same entry, so a
	// login written behind the reader's back stays on the screen to be seen.
	ipc.setField.mockReset();
	ipc.setField.mockReturnValueOnce(writing.promise).mockResolvedValue(made);

	icon('Add a field').click();
	flushSync();
	await Promise.resolve();
	named().value = 'Door code';
	returned(named());
	flushSync();

	const login = host.querySelector('[aria-label="Login"]') as HTMLInputElement;
	login.focus();
	enter(login, 'ali');
	writing.resolve(made);
	await vi.waitFor(() =>
		expect(host.querySelector('textarea[aria-label="Door code"]')).not.toBeNull()
	);
	await new Promise((resolve) => setTimeout(resolve, 0));
	flushSync();

	try {
		expect.soft(document.activeElement, 'the new field took the focus from the login').toBe(login);
		expect
			.soft(ipc.setField, 'half a login was written')
			.not.toHaveBeenCalledWith(props.entry.id, 'UserName', 'ali', false, expect.any(Number));
		expect.soft(ipc.setField).toHaveBeenCalledTimes(1);
	} finally {
		// What was typed is put back, so nothing is left to be told to Rust
		// once this test is over.
		login.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
		await unmount(component);
	}
});

/**
 * Notes a database protects are prose like any others, and their Change is
 * written in lines whatever they hold now: Return starts the next line and
 * only Cmd+Return saves. They go back protected. The first note on an entry
 * of such a database is typed where it stands, and goes back protected too.
 */
it('changes protected notes in lines, and writes them back protected', async () => {
	ipc.setField.mockReset();
	ipc.setField.mockResolvedValue(entry());
	const { component, entry: shown } = pane({
		fields: [field({ name: 'Notes', kind: 'notes', value: null, empty: false, protected: true })]
	});

	expect(host.querySelector('textarea[aria-label="Notes"]')).toBeNull();
	expect(host.querySelector('[aria-label="Copy notes"]'), 'a copy of the open kind').toBeNull();
	icon('Change Notes').click();
	flushSync();
	const typed = changer('New value of Notes');
	expect(typed.getAttribute('rows')).toBe('4');
	expect(typed.getAttribute('wrap')).toBe('soft');
	enter(typed, 'gate code 4711');
	expect(returned(typed).defaultPrevented, 'Return saved a note of one line').toBe(false);
	enter(typed, 'gate code 4711\nalarm 0000');
	expect(ipc.setField).not.toHaveBeenCalled();

	returned(typed, { metaKey: true });
	await vi.waitFor(() =>
		expect(ipc.setField).toHaveBeenCalledWith(
			shown.id,
			'Notes',
			'gate code 4711\nalarm 0000',
			true,
			expect.any(Number)
		)
	);
	expect(ipc.setField).toHaveBeenCalledTimes(1);
	unmount(component);

	const first = pane({
		fields: [field({ name: 'Notes', kind: 'notes', value: null, empty: true, protected: true })]
	});
	expect(host.querySelector('[aria-label="Change Notes"]')).toBeNull();
	const notes = host.querySelector('textarea[aria-label="Notes"]') as HTMLTextAreaElement;
	enter(notes, 'the first note');
	notes.dispatchEvent(new Event('blur'));
	expect(ipc.setField).toHaveBeenLastCalledWith(
		first.entry.id,
		'Notes',
		'the first note',
		true,
		expect.any(Number)
	);

	return unmount(first.component);
});

/** A database Coffer will not write back still reads and copies protected
 * notes, and offers no Change for them. */
it('offers no Change for protected notes in a database it cannot write', () => {
	const {
		component,
		onCopy,
		entry: shown
	} = pane(
		{
			fields: [field({ name: 'Notes', kind: 'notes', value: null, empty: false, protected: true })]
		},
		true
	);

	expect(host.querySelector('[aria-label="Change Notes"]')).toBeNull();
	icon('Copy Notes').click();
	expect(onCopy).toHaveBeenCalledWith(shown.id, 'Notes', null);

	return unmount(component);
});

const ALARM = 'alarm code 4711';
const PORT = '2202';

/**
 * Selecting a value by hand and pressing Cmd+C is the system's copy: an
 * ordinary pasteboard write that no clipboard manager is told to skip and
 * nothing clears. The notes, the address and every field of the reader's own
 * have a copy of their own, through Rust, by name - including an address
 * Coffer will not open, which is still somebody's to paste.
 */
it("copies the notes, the address and a field of the reader's own by name through Rust", () => {
	const {
		component,
		entry: shown,
		onCopy
	} = pane({
		fields: [
			field({
				name: 'URL',
				kind: 'url',
				value: 'javascript:alert(1)',
				empty: false,
				openable: false
			}),
			field({ name: 'Notes', kind: 'notes', value: ALARM, empty: false }),
			field({ name: 'Port', kind: 'custom', value: PORT, empty: false })
		]
	});

	expect(host.querySelector('[aria-label="Open this address"]')).toBeNull();
	icon('Copy address').click();
	expect(onCopy).toHaveBeenLastCalledWith(shown.id, 'URL');
	icon('Copy notes').click();
	expect(onCopy).toHaveBeenLastCalledWith(shown.id, 'Notes');
	icon('Copy Port').click();
	expect(onCopy).toHaveBeenLastCalledWith(shown.id, 'Port', null);
	expect(onCopy).toHaveBeenCalledTimes(3);

	// The values themselves never travel with the press.
	const sent = JSON.stringify(onCopy.mock.calls);
	for (const each of ['javascript:', ALARM, PORT]) expect(sent).not.toContain(each);

	return unmount(component);
});

/** Nothing to copy is nothing offered: an empty address, empty notes, an empty
 * field of the reader's own, and an entry that arrived without the first two. */
it("offers no copy of an address, notes or a field of the reader's own that is empty", () => {
	for (const fields of [
		[
			field({ name: 'URL', kind: 'url', value: '', empty: true }),
			field({ name: 'Notes', kind: 'notes', value: '', empty: true }),
			field({ name: 'Port', kind: 'custom', value: '', empty: true })
		],
		[field({ name: 'Port', kind: 'custom', value: '', empty: true })]
	]) {
		const { component } = pane({ fields });
		for (const label of ['Copy address', 'Copy notes', 'Copy Port']) {
			expect(host.querySelector(`[aria-label="${label}"]`), label).toBeNull();
		}
		unmount(component);
	}
});

/**
 * The address's copy stands after the button that opens it, the last thing in
 * the row the way the login's is. One the database protects is copied from
 * its own row and gets no second copy of the open kind; one in a database
 * Coffer cannot write is still read, and so still copied.
 */
it('puts the address copy after its opener, and leaves a protected address to its own', () => {
	const open = pane({
		fields: [
			field({
				name: 'URL',
				kind: 'url',
				value: 'https://bank.example',
				empty: false,
				openable: true
			})
		]
	});
	expect(icon('Open this address').nextElementSibling).toBe(icon('Copy address'));
	unmount(open.component);

	const kept = pane({ fields: [field({ name: 'URL', kind: 'url', value: null, empty: false })] });
	expect(host.querySelector('[aria-label="Copy address"]')).toBeNull();
	icon('Copy URL').click();
	expect(kept.onCopy).toHaveBeenCalledWith(kept.entry.id, 'URL', null);
	unmount(kept.component);

	const read = pane(
		{
			fields: [
				field({ name: 'URL', kind: 'url', value: 'https://bank.example', empty: false }),
				field({ name: 'Notes', kind: 'notes', value: ALARM, empty: false }),
				field({ name: 'Port', kind: 'custom', value: PORT, empty: false })
			]
		},
		true
	);
	for (const label of ['Copy address', 'Copy notes', 'Copy Port']) {
		expect(host.querySelector(`[aria-label="${label}"]`), label).not.toBeNull();
	}

	return unmount(read.component);
});

/**
 * A database may protect the title. The heading then draws the mask, and the
 * title is read, copied and changed in a row of its own above the login, the
 * way any protected value is: never in the heading, where shown it would be
 * the name of the window.
 */
it('reads, copies and changes a protected title in its own row, never in the heading', async () => {
	const NAME = 'Numbered account';
	ipc.reveal.mockResolvedValue(NAME);
	ipc.setField.mockReset();
	ipc.setField.mockResolvedValue(entry());
	const {
		component,
		entry: shown,
		onCopy
	} = pane({
		fields: [
			field({ name: 'Title', kind: 'title', value: null, empty: false, protected: true }),
			field({ name: 'UserName', kind: 'username', value: 'alice', empty: false })
		]
	});

	const header = host.querySelector('header');
	expect(header?.querySelector('use[href="#redact"]'), 'the heading is not masked').not.toBeNull();
	expect(header?.querySelector('input, textarea'), 'the heading can be typed into').toBeNull();
	const labels = [...host.querySelectorAll('span')];
	const titleRow = labels.find((each) => each.textContent?.trim() === 'Title');
	const loginRow = labels.find((each) => each.textContent?.trim() === 'Login');
	if (!titleRow || !loginRow) throw new Error('there is no row for the title or the login');
	expect(
		titleRow.compareDocumentPosition(loginRow) & Node.DOCUMENT_POSITION_FOLLOWING,
		'the title row is not above the login'
	).toBeTruthy();

	icon('Show title').click();
	await vi.waitFor(() => expect(screen()).toContain(NAME));
	expect(ipc.reveal).toHaveBeenCalledWith(shown.id, 'Title');
	expect(header?.textContent, 'the shown title reached the heading').not.toContain(NAME);
	expect(attributes().join(' ')).not.toContain(NAME);

	icon('Copy title').click();
	expect(onCopy).toHaveBeenCalledWith(shown.id, 'Title', null);

	icon('Change Title').click();
	flushSync();
	const typed = changer('New value of Title');
	expect(typed.getAttribute('rows'), 'a title is changed in lines').toBe('1');
	enter(typed, 'Savings');
	expect(returned(typed).defaultPrevented).toBe(true);
	await vi.waitFor(() =>
		expect(ipc.setField).toHaveBeenCalledWith(
			shown.id,
			'Title',
			'Savings',
			true,
			expect.any(Number)
		)
	);
	expect(ipc.setField).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/** A database Coffer cannot write, or an entry in the bin, still reads and
 * copies a protected title and offers no way to change it. */
it('offers no Change for a protected title it cannot write', () => {
	const binned = { since: null, within: null, from: null };
	for (const [over, readOnly] of [
		[{}, true],
		[{ binned, deletion: 'forever' as const }, false]
	] as const) {
		const { component } = pane(
			{
				...over,
				fields: [
					field({ name: 'Title', kind: 'title', value: null, empty: false, protected: true })
				]
			},
			readOnly
		);
		expect(host.querySelector('[aria-label="Show title"]')).not.toBeNull();
		expect(host.querySelector('[aria-label="Copy title"]')).not.toBeNull();
		expect(host.querySelector('[aria-label="Change Title"]')).toBeNull();
		unmount(component);
	}
});

/**
 * A protected title that is empty has nothing to hide and nothing to reveal,
 * and is typed into the heading like any other title. It is written back
 * protected: the database asked for that, and an empty value is no reason to
 * put the next one into the file as plain text.
 */
it('types a protected title that is empty into the heading, and writes it protected', () => {
	ipc.setField.mockReset();
	ipc.setField.mockResolvedValue(entry());
	const { component, entry: shown } = pane({
		fields: [field({ name: 'Title', kind: 'title', value: null, empty: true, protected: true })]
	});

	const header = host.querySelector('header');
	expect(
		header?.querySelector('use[href="#redact"]'),
		'nothing is hidden behind the mask'
	).toBeNull();
	for (const label of ['Show title', 'Copy title', 'Change Title']) {
		expect(host.querySelector(`[aria-label="${label}"]`), label).toBeNull();
	}
	const title = header?.querySelector('input[aria-label="Title"]') as HTMLInputElement;
	expect(title).not.toBeNull();
	enter(title, 'Bank');
	title.dispatchEvent(new Event('blur'));

	expect(ipc.setField).toHaveBeenCalledTimes(1);
	expect(ipc.setField).toHaveBeenCalledWith(shown.id, 'Title', 'Bank', true, expect.any(Number));

	return unmount(component);
});

/**
 * Rust hands the reader's own fields over in the order of their bytes, which
 * puts "Code 10" before "Code 2", every capital before every small letter and
 * every accent after the end of the alphabet. The pane draws them the way a
 * reader looks for them, and the same way however Rust happens to send them.
 */
it("draws the fields of the reader's own in reading order, not in the order Rust sends", () => {
	const ETAGERE = '\u00c9tag\u00e8re';
	const OMEGA = '\u03a9mega';
	const bytewise = [
		'Code 1',
		'Code 10',
		'Code 2',
		'PIN',
		'Pin',
		'Zone',
		'apple',
		'pin',
		ETAGERE,
		OMEGA
	];
	expect([...bytewise].sort(), 'the fixture is not in the order Rust sends').toEqual(bytewise);
	const { component, props } = deleting({
		fields: [
			PASSWORD,
			...bytewise.map((name) => field({ name, kind: 'custom', value: name, empty: false })),
			field({ name: 'Notes', kind: 'notes', value: 'a note', empty: false })
		]
	});

	const reading = [
		'apple',
		'Code 1',
		'Code 2',
		'Code 10',
		ETAGERE,
		'PIN',
		'Pin',
		'pin',
		'Zone',
		OMEGA
	];
	expect(drawnOrder()).toEqual(reading);

	props.entry = { ...props.entry, fields: [...props.entry.fields].reverse() };
	flushSync();
	expect(drawnOrder(), 'the order changed with the order Rust sent').toEqual(reading);

	return unmount(component);
});

/**
 * Whether a field is hidden is the lock's to say, on every row, at any time:
 * Rust moves the value from one kind of storage to the other and hands back
 * the entry, which is what the window is given.
 */
it("hides a field of the reader's own or shows it from its lock", async () => {
	const nowHidden = entry();
	const nowOpen = entry();
	ipc.setProtection.mockReset();
	ipc.setProtection.mockResolvedValueOnce(nowHidden).mockResolvedValueOnce(nowOpen);
	const {
		component,
		entry: shown,
		onChanged,
		onFailure
	} = pane({
		fields: [
			field({ name: 'Expiry', kind: 'custom', value: '12/29', empty: false }),
			hiddenField('CVC')
		]
	});

	const open = icon('Keep Expiry hidden');
	const kept = icon('Keep CVC hidden');
	expect(open.getAttribute('aria-pressed')).toBe('false');
	expect(open.querySelector('use')?.getAttribute('href')).toBe('#i-unlock');
	expect(kept.getAttribute('aria-pressed')).toBe('true');
	expect(kept.querySelector('use')?.getAttribute('href')).toBe('#i-lock');

	open.click();
	await vi.waitFor(() => expect(onChanged).toHaveBeenCalledWith(nowHidden));
	expect(ipc.setProtection).toHaveBeenCalledWith(shown.id, 'Expiry', true);

	kept.click();
	await vi.waitFor(() => expect(onChanged).toHaveBeenCalledWith(nowOpen));
	expect(ipc.setProtection).toHaveBeenLastCalledWith(shown.id, 'CVC', false);
	expect(ipc.setProtection).toHaveBeenCalledTimes(2);
	expect(ipc.setField, 'the value was written to move it').not.toHaveBeenCalled();
	expect(onFailure).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * A second press while the first is on its way would undo it before either
 * landed, so the lock takes one press at a time. A refusal is said, changes
 * nothing, and leaves the lock to be pressed again.
 */
it('takes one press of a lock at a time, and says so when Rust refuses it', async () => {
	const answer = Promise.withResolvers<Entry>();
	ipc.setProtection.mockReset();
	ipc.setProtection.mockReturnValue(answer.promise);
	const {
		component,
		entry: shown,
		onChanged,
		onFailure
	} = pane({
		fields: [field({ name: 'Expiry', kind: 'custom', value: '12/29', empty: false })]
	});

	const lock = icon('Keep Expiry hidden');
	lock.click();
	lock.click();
	await vi.waitFor(() => expect(ipc.setProtection).toHaveBeenCalledTimes(1));
	flushSync();
	expect(lock.disabled, 'the lock can be pressed while its answer is on its way').toBe(true);
	lock.click();
	await Promise.resolve();
	expect(ipc.setProtection).toHaveBeenCalledTimes(1);

	answer.reject({ code: 'readOnly', message: 'this database is read only' });
	await vi.waitFor(() =>
		expect(onFailure).toHaveBeenCalledWith(expect.objectContaining({ code: 'readOnly' }))
	);
	flushSync();
	expect(onChanged).not.toHaveBeenCalled();
	expect(lock.disabled, 'a refused press left the lock stuck').toBe(false);
	expect(lock.getAttribute('aria-pressed')).toBe('false');

	ipc.setProtection.mockResolvedValue(entry());
	lock.click();
	await vi.waitFor(() => expect(ipc.setProtection).toHaveBeenCalledTimes(2));
	expect(ipc.setProtection).toHaveBeenLastCalledWith(shown.id, 'Expiry', true);

	return unmount(component);
});

/**
 * Pressing the lock leaves the value being typed, which writes it. That value
 * lands before the protection changes; the other way round, the entry drawn
 * last is the one from before the press, and the lock says the opposite of
 * what the file holds.
 */
it('lets a value typed into a field land before its lock changes how it is kept', async () => {
	const writing = Promise.withResolvers<Entry>();
	ipc.setField.mockReset();
	ipc.setField.mockReturnValue(writing.promise);
	ipc.setProtection.mockReset();
	ipc.setProtection.mockResolvedValue(entry());
	const { component, entry: shown } = pane({
		fields: [field({ name: 'Expiry', kind: 'custom', value: '12/29', empty: false })]
	});

	const typed = host.querySelector('textarea[aria-label="Expiry"]') as HTMLTextAreaElement;
	enter(typed, '01/31');
	typed.dispatchEvent(new Event('blur'));
	icon('Keep Expiry hidden').click();
	await new Promise((resolve) => setTimeout(resolve, 0));

	expect(ipc.setField).toHaveBeenCalledWith(shown.id, 'Expiry', '01/31', false, expect.any(Number));
	expect(ipc.setProtection, 'the protection moved under a value on its way').not.toHaveBeenCalled();

	writing.resolve(entry());
	await vi.waitFor(() => expect(ipc.setProtection).toHaveBeenCalledWith(shown.id, 'Expiry', true));
	expect(ipc.setField.mock.invocationCallOrder[0]).toBeLessThan(
		ipc.setProtection.mock.invocationCallOrder[0]
	);

	return unmount(component);
});

/**
 * The value the lock waits for can take a while to land, and the reader does
 * not wait with it: by the time it has, the pane may be on the next entry,
 * with a field of the same name kept the other way. The press was about the
 * field it was made on. Read when the answer was ready, the entry was the
 * next one, and the lock hid or showed a field nobody had pressed anything on.
 */
it('changes how a field is kept on the entry its lock was pressed on, after the pane has moved on', async () => {
	const writing = Promise.withResolvers<Entry>();
	ipc.setField.mockReset();
	ipc.setField.mockReturnValue(writing.promise);
	ipc.setProtection.mockReset();
	ipc.setProtection.mockResolvedValue(entry());
	const { component, props } = deleting({
		fields: [field({ name: 'Expiry', kind: 'custom', value: '12/29', empty: false })]
	});
	const pressed = props.entry.id;

	const typed = host.querySelector('textarea[aria-label="Expiry"]') as HTMLTextAreaElement;
	enter(typed, '01/31');
	typed.dispatchEvent(new Event('blur'));
	icon('Keep Expiry hidden').click();
	await new Promise((resolve) => setTimeout(resolve, 0));
	expect(ipc.setProtection, 'the lock did not wait for the value').not.toHaveBeenCalled();

	const next = entry({ fields: [hiddenField('Expiry')] });
	props.entry = next;
	flushSync();
	writing.resolve(entry());

	await vi.waitFor(() => expect(ipc.setProtection).toHaveBeenCalledTimes(1));
	expect(ipc.setProtection).toHaveBeenCalledWith(pressed, 'Expiry', true);
	expect(ipc.setProtection, 'the press went to the entry on the screen').not.toHaveBeenCalledWith(
		next.id,
		expect.anything(),
		expect.anything()
	);
	flushSync();
	const lock = icon('Keep Expiry hidden');
	expect(lock.getAttribute('aria-pressed'), "the next entry's field moved").toBe('true');
	expect(lock.disabled, "the next entry's lock is held by the last one's press").toBe(false);
	expect(props.onFailure).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * A new value being typed in a field's Change is on its way to that field,
 * under that name and that protection. Neither the lock nor the name moves
 * under it, and the other rows' do.
 */
it("holds a field's lock and name still while its Change is open", () => {
	ipc.setProtection.mockReset();
	ipc.setProtection.mockResolvedValue(entry());
	const { component } = pane({
		fields: [
			hiddenField('CVC'),
			field({ name: 'Expiry', kind: 'custom', value: '12/29', empty: false })
		]
	});

	icon('Change CVC').click();
	flushSync();
	expect(icon('Keep CVC hidden').disabled).toBe(true);
	icon('Keep CVC hidden').click();
	expect(host.querySelector('[aria-label="Rename CVC"]'), 'the name can be changed').toBeNull();
	expect(host.querySelector('span[title="CVC"]')?.textContent).toBe('CVC');
	expect(icon('Keep Expiry hidden').disabled).toBe(false);
	expect(host.querySelector('[aria-label="Rename Expiry"]')).not.toBeNull();
	expect(ipc.setProtection).not.toHaveBeenCalled();

	button('Cancel').click();
	flushSync();
	expect(icon('Keep CVC hidden').disabled).toBe(false);
	expect(host.querySelector('[aria-label="Rename CVC"]')).not.toBeNull();

	return unmount(component);
});

/**
 * The name is pressed to be changed, which takes the value and its protection
 * with it rather than asking for them again. The field opens with the name in
 * it and selected, Return finishes it - once, though the field leaving the
 * screen is a blur as well - and spaces at either end are no part of a name.
 */
it("renames a field of the reader's own from its name, once, without the spaces around it", async () => {
	const renamed = entry();
	ipc.renameField.mockReset();
	ipc.renameField.mockResolvedValue(renamed);
	const { component, entry: shown, onChanged } = pane({ fields: [hiddenField('PIN')] });

	const name = icon('Rename PIN');
	expect(name.title).toBe('PIN');
	name.click();
	flushSync();
	const typed = renaming('PIN');
	expect(typed.value).toBe('PIN');
	expect(document.activeElement).toBe(typed);
	expect([typed.selectionStart, typed.selectionEnd]).toEqual([0, 3]);

	typed.value = '  Card PIN  ';
	expect(returned(typed).defaultPrevented).toBe(true);
	typed.dispatchEvent(new FocusEvent('blur'));
	await vi.waitFor(() => expect(onChanged).toHaveBeenCalledWith(renamed));

	expect(ipc.renameField).toHaveBeenCalledTimes(1);
	expect(ipc.renameField).toHaveBeenCalledWith(shown.id, 'PIN', 'Card PIN');
	expect(host.querySelector('[aria-label="New name for PIN"]')).toBeNull();

	return unmount(component);
});

/** Leaving the name finishes it the way Return does; nothing, or the name it
 * already had, changes nothing whichever way it is finished. */
it('renames nothing for an empty name or the name the field already has', async () => {
	ipc.renameField.mockReset();
	ipc.renameField.mockResolvedValue(entry());
	const { component, entry: shown } = pane({ fields: [hiddenField('PIN')] });

	for (const typed of ['', '   ', 'PIN', '  PIN  ']) {
		for (const finish of ['Return', 'leaving'] as const) {
			icon('Rename PIN').click();
			flushSync();
			const input = renaming('PIN');
			input.value = typed;
			if (finish === 'Return') returned(input);
			else input.dispatchEvent(new FocusEvent('blur'));
			flushSync();
			expect(
				host.querySelector('[aria-label="New name for PIN"]'),
				`${finish} "${typed}"`
			).toBeNull();
		}
	}
	await Promise.resolve();
	expect(ipc.renameField).not.toHaveBeenCalled();

	icon('Rename PIN').click();
	flushSync();
	renaming('PIN').value = 'Card PIN';
	renaming('PIN').dispatchEvent(new FocusEvent('blur'));
	await vi.waitFor(() => expect(ipc.renameField).toHaveBeenCalledWith(shown.id, 'PIN', 'Card PIN'));

	return unmount(component);
});

/**
 * Escape puts the name back and is the name's alone: the window reads Escape
 * as "close the entry", and a name put back is not an entry put away. The
 * Escape that cancels an input method's conversion is the input method's.
 */
it('puts the name back on Escape without closing the entry', () => {
	ipc.renameField.mockReset();
	const elsewhere = vi.fn();
	window.addEventListener('keydown', elsewhere);
	const { component } = pane({ fields: [hiddenField('PIN')] });

	try {
		icon('Rename PIN').click();
		flushSync();
		const input = renaming('PIN');
		input.value = 'Card PIN';
		for (const composition of [{ isComposing: true }, { keyCode: 229 }]) {
			returned(input, { key: 'Escape', ...composition });
			returned(input, composition);
			flushSync();
			expect(renaming('PIN').value, JSON.stringify(composition)).toBe('Card PIN');
		}
		elsewhere.mockClear();

		input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
		flushSync();
		expect(elsewhere, 'the Escape closed the entry as well').not.toHaveBeenCalled();
		expect(host.querySelector('[aria-label="New name for PIN"]')).toBeNull();
		expect(icon('Rename PIN').textContent?.trim()).toBe('PIN');
		expect(ipc.renameField).not.toHaveBeenCalled();
	} finally {
		window.removeEventListener('keydown', elsewhere);
	}

	return unmount(component);
});

/**
 * A name the entry already gives another field is Rust's to refuse, and the
 * refusal is said. Nothing moved: the field is still under its own name, with
 * its value, and the one whose name was asked for is untouched.
 */
it('keeps the old name when Rust refuses the new one, and says why', async () => {
	const REFUSED = { code: 'refused', message: 'that name is already taken on this entry' };
	ipc.renameField.mockReset();
	ipc.renameField.mockRejectedValue(REFUSED);
	const {
		component,
		entry: shown,
		onChanged,
		onFailure
	} = pane({
		fields: [hiddenField('PIN'), field({ name: 'Code', kind: 'custom', value: '7', empty: false })]
	});

	icon('Rename PIN').click();
	flushSync();
	renaming('PIN').value = 'Code';
	returned(renaming('PIN'));
	await vi.waitFor(() => expect(onFailure).toHaveBeenCalledWith(REFUSED));
	flushSync();

	expect(ipc.renameField).toHaveBeenCalledWith(shown.id, 'PIN', 'Code');
	expect(onChanged).not.toHaveBeenCalled();
	expect(host.querySelector('[aria-label^="New name for"]')).toBeNull();
	expect(drawnOrder()).toEqual(['Code', 'PIN']);
	expect(host.querySelector('[aria-label="Show PIN"]')).not.toBeNull();

	return unmount(component);
});

/**
 * A refused name finished with Return closes the field it was typed in, and
 * the focus fell out of the pane with it - where the next Escape closes the
 * entry. It goes back to the name that stayed, which is where the reader
 * pressed to begin with; the name was a plain label while the refusal was on
 * its way, so the button it goes to is a new one. A reader who has put the
 * focus somewhere else in the meantime keeps it there.
 */
it('gives the focus back to the name a refused rename leaves in place', async () => {
	const REFUSED = { code: 'refused', message: 'that name is already taken on this entry' };
	ipc.renameField.mockReset();
	ipc.renameField.mockRejectedValue(REFUSED);
	const { component, props } = landing({
		fields: [hiddenField('PIN'), field({ name: 'Code', kind: 'custom', value: '7', empty: false })]
	});

	icon('Rename PIN').click();
	flushSync();
	renaming('PIN').value = 'Code';
	returned(renaming('PIN'));
	await vi.waitFor(() => expect(props.onFailure).toHaveBeenCalledWith(REFUSED));
	await vi.waitFor(() => expect(document.activeElement).toBe(icon('Rename PIN')));
	expect(drawnOrder()).toEqual(['Code', 'PIN']);
	expect(props.onChanged).not.toHaveBeenCalled();

	const refusing = Promise.withResolvers<Entry>();
	ipc.renameField.mockReturnValue(refusing.promise);
	icon('Rename PIN').click();
	flushSync();
	renaming('PIN').value = 'Code';
	returned(renaming('PIN'));
	await vi.waitFor(() => expect(ipc.renameField).toHaveBeenCalledTimes(2));
	const tags = button('+ tag');
	tags.focus();
	refusing.reject(REFUSED);
	await vi.waitFor(() => expect(props.onFailure).toHaveBeenCalledTimes(2));
	await new Promise((resolve) => setTimeout(resolve, 0));
	flushSync();

	expect(document.activeElement, 'the refusal took the focus from where the reader put it').toBe(
		tags
	);
	expect(icon('Rename PIN').textContent?.trim()).toBe('PIN');

	return unmount(component);
});

/** Clicking the name to change it leaves the value being typed, which writes
 * it; the rename goes after that value has landed, under the name it was
 * typed for. */
it('renames a field only once the value typed into it has landed', async () => {
	const writing = Promise.withResolvers<Entry>();
	ipc.setField.mockReset();
	ipc.setField.mockReturnValue(writing.promise);
	ipc.renameField.mockReset();
	ipc.renameField.mockResolvedValue(entry());
	const { component, entry: shown } = pane({
		fields: [field({ name: 'Port', kind: 'custom', value: PORT, empty: false })]
	});

	const typed = host.querySelector('textarea[aria-label="Port"]') as HTMLTextAreaElement;
	enter(typed, '2203');
	typed.dispatchEvent(new Event('blur'));
	icon('Rename Port').click();
	flushSync();
	renaming('Port').value = 'SSH port';
	returned(renaming('Port'));
	await new Promise((resolve) => setTimeout(resolve, 0));

	expect(ipc.setField).toHaveBeenCalledWith(shown.id, 'Port', '2203', false, expect.any(Number));
	expect(ipc.renameField, 'the name moved under a value on its way').not.toHaveBeenCalled();

	writing.resolve(entry());
	await vi.waitFor(() =>
		expect(ipc.renameField).toHaveBeenCalledWith(shown.id, 'Port', 'SSH port')
	);

	return unmount(component);
});

/**
 * The same wait for a rename: the value typed into the field lands first, and
 * the reader may be on the next entry by then, with a field of the same name.
 * The new name was typed for the field it was typed on. Given to the entry on
 * the screen instead, it renamed a field on an entry nobody had touched.
 */
it('renames the field on the entry the name was typed on, after the pane has moved on', async () => {
	const writing = Promise.withResolvers<Entry>();
	ipc.setField.mockReset();
	ipc.setField.mockReturnValue(writing.promise);
	ipc.renameField.mockReset();
	ipc.renameField.mockResolvedValue(entry());
	const { component, props } = deleting({
		fields: [field({ name: 'Port', kind: 'custom', value: PORT, empty: false })]
	});
	const typedOn = props.entry.id;

	const typed = host.querySelector('textarea[aria-label="Port"]') as HTMLTextAreaElement;
	enter(typed, '2203');
	typed.dispatchEvent(new Event('blur'));
	icon('Rename Port').click();
	flushSync();
	renaming('Port').value = 'SSH port';
	returned(renaming('Port'));
	await new Promise((resolve) => setTimeout(resolve, 0));
	expect(ipc.renameField, 'the rename did not wait for the value').not.toHaveBeenCalled();

	const next = entry({
		fields: [field({ name: 'Port', kind: 'custom', value: '22', empty: false })]
	});
	props.entry = next;
	flushSync();
	writing.resolve(entry());

	await vi.waitFor(() => expect(ipc.renameField).toHaveBeenCalledTimes(1));
	expect(ipc.renameField).toHaveBeenCalledWith(typedOn, 'Port', 'SSH port');
	expect(ipc.renameField, 'the name went to the entry on the screen').not.toHaveBeenCalledWith(
		next.id,
		expect.anything(),
		expect.anything()
	);
	flushSync();
	expect(drawnOrder()).toEqual(['Port']);
	expect(
		host.querySelector('[aria-label="Rename Port"]'),
		"the next entry's name is held by the last one's rename"
	).not.toBeNull();
	expect(props.onFailure).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * A name finished with Return is a reader working from the keyboard. The row
 * under the old name goes when the new name lands, and the one under the new
 * name takes the focus as it is drawn; the focus used to fall out of the pane,
 * where the next Escape closes the entry. Only then: not for a name finished
 * by going somewhere else, and not from a reader who has gone on elsewhere
 * while the rename was on its way.
 */
it('gives the focus to the new name once a rename finished with Return has landed', async () => {
	ipc.renameField.mockReset();
	const { component, props } = landing({ fields: [PASSWORD, hiddenField('PIN')] });
	const under = (name: string): Entry => ({
		...props.entry,
		fields: [PASSWORD, hiddenField(name)]
	});

	ipc.renameField.mockResolvedValueOnce(under('Card PIN'));
	icon('Rename PIN').click();
	flushSync();
	renaming('PIN').value = 'Card PIN';
	returned(renaming('PIN'));
	await vi.waitFor(() => expect(document.activeElement).toBe(icon('Rename Card PIN')));
	expect(host.querySelector('[aria-label="Rename PIN"]')).toBeNull();

	const slow = Promise.withResolvers<Entry>();
	ipc.renameField.mockReturnValueOnce(slow.promise);
	icon('Rename Card PIN').click();
	flushSync();
	renaming('Card PIN').value = 'Door PIN';
	returned(renaming('Card PIN'));
	await vi.waitFor(() => expect(ipc.renameField).toHaveBeenCalledTimes(2));
	const tags = button('+ tag');
	tags.focus();
	slow.resolve(under('Door PIN'));
	await vi.waitFor(() => expect(drawnOrder()).toEqual(['Door PIN']));
	await new Promise((resolve) => setTimeout(resolve, 0));
	flushSync();
	expect(document.activeElement, 'the new name took the focus from where the reader put it').toBe(
		tags
	);

	tags.blur();
	ipc.renameField.mockResolvedValueOnce(under('Gate PIN'));
	icon('Rename Door PIN').click();
	flushSync();
	renaming('Door PIN').value = 'Gate PIN';
	renaming('Door PIN').dispatchEvent(new FocusEvent('blur'));
	await vi.waitFor(() => expect(drawnOrder()).toEqual(['Gate PIN']));
	await new Promise((resolve) => setTimeout(resolve, 0));
	flushSync();
	expect(document.activeElement, 'a name finished elsewhere took the focus').not.toBe(
		icon('Rename Gate PIN')
	);
	expect(ipc.renameField).toHaveBeenCalledTimes(3);
	expect(props.onFailure).not.toHaveBeenCalled();

	return unmount(component);
});

/** Nothing here may be changed in a database Coffer cannot write, or in the
 * bin, and that includes a name: it is drawn, whole in its title, and not
 * offered. */
it('offers no rename where nothing may be changed', () => {
	const binned = { since: null, within: null, from: null };
	for (const [over, readOnly] of [
		[{}, true],
		[{ binned, deletion: 'forever' as const }, false]
	] as const) {
		const { component } = pane({ ...over, fields: [hiddenField('Recovery email')] }, readOnly);
		expect(host.querySelector('[aria-label="Rename Recovery email"]')).toBeNull();
		expect(host.querySelector('span[title="Recovery email"]')?.textContent).toBe('Recovery email');
		expect(host.querySelector('[aria-label="Keep Recovery email hidden"]')).toBeNull();
		unmount(component);
	}
});

/**
 * Lines are not in the file - a value is in lines because it has a break -
 * so a field named to be written in lines is the pane's to remember, by
 * name. A new name is the same field: its Change is still four lines tall
 * under it. The name it had is nobody's in lines any more.
 */
it('keeps a field named to be in lines in lines once it is renamed, and only under the new name', async () => {
	ipc.setField.mockReset();
	ipc.setField.mockResolvedValue(entry());
	ipc.renameField.mockReset();
	const { component, props } = deleting({ fields: [PASSWORD] });

	icon('Add a field').click();
	flushSync();
	named().value = 'Codes';
	button('Multi-line').click();
	flushSync();
	returned(named());
	await vi.waitFor(() =>
		expect(ipc.setField).toHaveBeenCalledWith(props.entry.id, 'Codes', '', true, expect.any(Number))
	);
	props.entry = { ...props.entry, fields: [PASSWORD, hiddenField('Codes')] };
	flushSync();

	const renamed: Entry = { ...props.entry, fields: [PASSWORD, hiddenField('Recovery codes')] };
	ipc.renameField.mockResolvedValue(renamed);
	icon('Rename Codes').click();
	flushSync();
	renaming('Codes').value = 'Recovery codes';
	returned(renaming('Codes'));
	await vi.waitFor(() => expect(props.onChanged).toHaveBeenCalledWith(renamed));
	props.entry = renamed;
	flushSync();
	await Promise.resolve();
	flushSync();

	icon('Change Recovery codes').click();
	flushSync();
	const typed = changer('New value of Recovery codes');
	expect(typed.getAttribute('rows'), 'the rename put the field on one line').toBe('4');
	enter(typed, 'first code');
	expect(returned(typed).defaultPrevented, 'Return saved the first code alone').toBe(false);
	typed.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
	flushSync();

	props.entry = { ...renamed, fields: [...renamed.fields, hiddenField('Codes')] };
	flushSync();
	icon('Change Codes').click();
	flushSync();
	expect(changer('New value of Codes').getAttribute('rows'), 'the old name kept its lines').toBe(
		'1'
	);
	expect(ipc.setField).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/**
 * A hidden field of the reader's own is given a made value the way the
 * password is: Make one opens the generator under its row, and the value goes
 * straight in, hidden, from the press that puts it there. A field kept in the
 * open is not offered one, and neither is anything that cannot be written.
 */
it("makes a value for a hidden field of the reader's own and writes it hidden", async () => {
	const MADE = '4096-1123-8807';
	ipc.generatePassword.mockReset();
	ipc.generatePassword.mockResolvedValue(generated(MADE));
	ipc.setField.mockReset();
	ipc.setField.mockResolvedValue(entry());
	const {
		component,
		entry: shown,
		onFailure
	} = pane({
		fields: [
			hiddenField('PIN'),
			field({ name: 'Backup PIN', kind: 'custom', protected: true, value: null, empty: true }),
			field({ name: 'Region', kind: 'custom', value: 'eu-central', empty: false })
		]
	});

	expect(host.querySelector('[aria-label="Make one for Backup PIN"]')).not.toBeNull();
	expect(host.querySelector('[aria-label="Make one for Region"]')).toBeNull();

	const make = icon('Make one for PIN');
	make.click();
	flushSync();
	expect(make.getAttribute('aria-expanded')).toBe('true');
	await vi.waitFor(() => expect(button('Put it in the field').disabled).toBe(false));
	expect(ipc.generatePassword).toHaveBeenCalledWith(expect.any(Object), 'field');
	expect(attributes().join(' ')).not.toContain(MADE);
	expect(written()).not.toContain(MADE);
	expect(ipc.setField, 'a made value was written before it was put in').not.toHaveBeenCalled();

	button('Put it in the field').click();
	flushSync();
	await vi.waitFor(() =>
		expect(ipc.setField).toHaveBeenCalledWith(shown.id, 'PIN', MADE, true, expect.any(Number))
	);
	expect(ipc.setField).toHaveBeenCalledTimes(1);
	expect(host.querySelector('[aria-label="Generator for PIN"]')).toBeNull();
	expect(host.textContent).not.toContain('Password generator');
	expect(screen()).not.toContain(MADE);
	await vi.waitFor(() => expect(document.activeElement).toBe(icon('Make one for PIN')));
	expect(onFailure).not.toHaveBeenCalled();
	unmount(component);

	const read = pane({ fields: [hiddenField('PIN')] }, true);
	expect(host.querySelector('[aria-label="Make one for PIN"]')).toBeNull();

	return unmount(read.component);
});

/**
 * A Change and the generator are two ways to give a field a new value, and a
 * made value put in while a typed one waits would be written over by it a
 * moment later. Opening the Change puts the generator away.
 */
it("puts a field's generator away when its Change is opened", async () => {
	ipc.generatePassword.mockReset();
	ipc.generatePassword.mockResolvedValue(generated('4096-1123-8807'));
	const { component } = pane({ fields: [hiddenField('PIN')] });

	icon('Make one for PIN').click();
	flushSync();
	await vi.waitFor(() => expect(button('Put it in the field').disabled).toBe(false));
	icon('Change PIN').click();
	flushSync();

	expect(host.querySelector('[aria-label="Generator for PIN"]')).toBeNull();
	expect(host.textContent).not.toContain('Password generator');
	expect(host.querySelector('[aria-label="Make one for PIN"]')).toBeNull();
	expect(changer('New value of PIN')).not.toBeNull();
	expect(ipc.setField).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * Nothing one entry's field rows were doing is still there under the next
 * entry's fields of the same names: a name being changed would be given to
 * the wrong entry's field, and a made value put into it. A value that was
 * being made for the entry that was open arrives to nowhere.
 */
it("closes a rename and a generator in a field's row when another entry is shown", async () => {
	const making = Promise.withResolvers<Generated>();
	ipc.generatePassword.mockReset();
	ipc.generatePassword.mockReturnValue(making.promise);
	ipc.renameField.mockReset();
	ipc.setField.mockReset();
	const fields = () => [
		hiddenField('PIN'),
		field({ name: 'Region', kind: 'custom', value: 'eu-central', empty: false })
	];
	const { component, props } = deleting({ fields: fields() });

	icon('Make one for PIN').click();
	flushSync();
	await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalled());
	icon('Rename Region').click();
	flushSync();
	renaming('Region').value = 'Zone';

	props.entry = entry({ fields: fields() });
	flushSync();

	expect(host.querySelector('[aria-label="Generator for PIN"]')).toBeNull();
	expect(host.textContent).not.toContain('Password generator');
	expect(host.querySelector('[aria-label="New name for Region"]')).toBeNull();
	expect(icon('Make one for PIN').getAttribute('aria-expanded')).toBe('false');
	expect(icon('Rename Region').textContent?.trim()).toBe('Region');

	making.resolve(generated('Made-For-The-Last-One'));
	await new Promise((resolve) => setTimeout(resolve, 0));
	flushSync();
	expect(screen()).not.toContain('Made-For-The-Last-One');
	expect(ipc.setField).not.toHaveBeenCalled();
	expect(ipc.renameField).not.toHaveBeenCalled();
	expect(props.onFailure).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * The password's generator and a field's each remember a recipe of their own,
 * so a PIN set up under a field is not what the next password is made from,
 * and the two can be open at once. Each asks Rust as what it is, makes its
 * value from its own recipe, and puts it in its own field: the password's
 * made value under the PIN, or the other way round, is a secret written where
 * nobody looks for it, over one that is gone.
 */
it("asks the password's generator and a field's as what each is, and puts each one's value in its own field", async () => {
	const PIN: Recipe = { length: 4, alphabets: ['digits'], similar: true, avoid: '' };
	const forField = drawing({ recipe: PIN, shortest: 4, pin: true, lookAlikes: '' });
	ipc.generator.mockReset();
	ipc.generator.mockImplementation((purpose: Purpose) =>
		Promise.resolve(purpose === 'field' ? forField : drawing())
	);
	ipc.generatePassword.mockReset();
	ipc.generatePassword.mockImplementation((_recipe: unknown, purpose: Purpose) =>
		Promise.resolve(
			purpose === 'field'
				? generated('4711', { generator: forField })
				: generated('Made-Password-123')
		)
	);
	ipc.setField.mockReset();
	ipc.setField.mockResolvedValue(entry());
	const {
		component,
		entry: shown,
		onFailure
	} = pane({
		fields: [PASSWORD, hiddenField('PIN')]
	});

	const forPassword = [...host.querySelectorAll('button')].find(
		(each) => each.textContent?.trim() === 'Make one' && !each.hasAttribute('aria-label')
	);
	forPassword?.click();
	icon('Make one for PIN').click();
	flushSync();
	const put = (panel: string) => {
		const found = [
			...(host.querySelector(`[role="group"][aria-label="${panel}"]`)?.querySelectorAll('button') ??
				[])
		].find((each) => each.textContent?.trim() === 'Put it in the field');
		if (!found) throw new Error(`no ${panel} is open`);
		return found;
	};
	await vi.waitFor(() => {
		expect(put('Password generator').disabled).toBe(false);
		expect(put('Generator for PIN').disabled).toBe(false);
	});

	expect(ipc.generator).toHaveBeenCalledWith('password');
	expect(ipc.generator).toHaveBeenCalledWith('field');
	expect(ipc.generator).toHaveBeenCalledTimes(2);
	expect(ipc.generatePassword).toHaveBeenCalledWith(drawing().recipe, 'password');
	expect(ipc.generatePassword).toHaveBeenCalledWith(PIN, 'field');
	expect(ipc.generatePassword).toHaveBeenCalledTimes(2);

	put('Generator for PIN').click();
	flushSync();
	await vi.waitFor(() => expect(ipc.setField).toHaveBeenCalledTimes(1));
	expect(ipc.setField).toHaveBeenCalledWith(shown.id, 'PIN', '4711', true, expect.any(Number));
	expect(host.querySelector('[aria-label="Generator for PIN"]')).toBeNull();

	put('Password generator').click();
	flushSync();
	await vi.waitFor(() => expect(ipc.setField).toHaveBeenCalledTimes(2));
	expect(ipc.setField).toHaveBeenLastCalledWith(
		shown.id,
		'Password',
		'Made-Password-123',
		true,
		expect.any(Number)
	);
	expect(screen()).not.toContain('4711');
	expect(screen()).not.toContain('Made-Password-123');
	expect(onFailure).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * The line above the title is where the entry is and the way to move it, in an
 * entry Coffer can write. In the bin, and in a vault it will not write back, it
 * is the line the mockup draws and nothing more: nothing there moves.
 */
it('the line above the title moves the entry, and is plain text in the bin and in a vault it cannot write', () => {
	const work = group({ name: 'Work' });
	const banking = group({ name: 'Banking' });
	const root = group({ name: 'Root', sections: [work, banking] });
	const onMove = vi.fn();
	const component = mount(EntryView, {
		target: host,
		props: props({ entry: entry({ group: work.id }), root, path: [work], onMove })
	});
	flushSync();

	const line = host.querySelector<HTMLButtonElement>('header button[aria-haspopup="listbox"]');
	expect(line?.textContent).toContain('Work');
	expect(line?.getAttribute('aria-expanded')).toBe('false');
	line?.click();
	flushSync();
	expect(line?.getAttribute('aria-expanded')).toBe('true');
	const filter = host.querySelector<HTMLInputElement>('input[role="combobox"]');
	expect(document.activeElement).toBe(filter);
	expect(host.querySelector('[aria-current="location"]')?.textContent).toContain('Work');

	filter?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
	flushSync();
	filter?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
	flushSync();
	expect(onMove).toHaveBeenCalledWith(banking.id);
	expect(host.querySelector('input[role="combobox"]'), 'the list stayed open').toBeNull();
	// The entry stays in the pane wherever it goes, and the keys stay on the
	// line that moved it rather than falling to the top of the window.
	expect(document.activeElement, 'the focus fell out of the pane').toBe(line);
	unmount(component);

	for (const over of [
		{ readOnly: true },
		{ entry: entry({ group: work.id, binned: { since: null, within: null, from: work.id } }) }
	]) {
		const still = mount(EntryView, {
			target: host,
			props: props({ entry: entry({ group: work.id }), root, path: [work], onMove, ...over })
		});
		flushSync();
		expect(host.querySelector('header button[aria-haspopup]'), JSON.stringify(over)).toBeNull();
		expect(host.querySelector('header')?.textContent).toContain('Work');
		unmount(still);
	}
	expect(onMove).toHaveBeenCalledTimes(1);
});

/** The mockup leaves the line out at the top of the vault. An entry that can
 * be moved says where it is there too, because the line is the way out of it. */
it('an entry at the top of the vault says so above its title', () => {
	const root = group({ name: 'Passwords', sections: [group({ name: 'Work' })] });
	const component = mount(EntryView, {
		target: host,
		props: props({ entry: entry({ group: root.id }), root, path: [] })
	});
	flushSync();

	const line = host.querySelector('header button[aria-haspopup="listbox"]');
	expect(line?.textContent).toContain('Top of the vault');
	expect(line?.textContent, 'the top group was named').not.toContain('Passwords');

	// Escape closes the list and gives the focus back to the line.
	(line as HTMLButtonElement | null)?.click();
	flushSync();
	host
		.querySelector('input[role="combobox"]')
		?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
	flushSync();
	expect(host.querySelector('input[role="combobox"]')).toBeNull();
	expect(document.activeElement).toBe(line);

	return unmount(component);
});

/**
 * The window hands the pane one entry and then another. A folder list opened
 * over the first is not open over the second, where a Return meant for the
 * first would move the second.
 */
it('a folder list open over one entry is not open over the next', () => {
	const work = group({ name: 'Work' });
	const banking = group({ name: 'Banking' });
	const root = group({ name: 'Root', sections: [work, banking] });
	const onMove = vi.fn();
	const handed = reactive(props({ entry: entry({ group: work.id }), root, path: [work], onMove }));
	const component = mount(EntryView, { target: host, props: handed });
	flushSync();

	host.querySelector<HTMLButtonElement>('header button[aria-haspopup="listbox"]')?.click();
	flushSync();
	const filter = host.querySelector<HTMLInputElement>('input[role="combobox"]');
	expect(filter).not.toBeNull();
	filter?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
	flushSync();

	handed.entry = entry({ group: work.id });
	flushSync();
	expect(
		host.querySelector('input[role="combobox"]'),
		'the list stayed over the next entry'
	).toBeNull();
	expect(
		host.querySelector('header button[aria-haspopup="listbox"]')?.getAttribute('aria-expanded')
	).toBe('false');
	filter?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
	document.activeElement?.dispatchEvent(
		new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })
	);
	flushSync();
	expect(onMove).not.toHaveBeenCalled();

	return unmount(component);
});

/** The suggestions under the name of a new field, by name. */
function suggestions(): string[] {
	return [...host.querySelectorAll<HTMLButtonElement>('button[aria-label^="Add the field "]')].map(
		(each) => each.textContent?.trim() ?? ''
	);
}

/** The names a reader adds most often are offered under the name of a new
 * field: only those the entry does not have yet, and only those that start
 * with what is typed, in either case. With none left, the line goes. */
it('suggests names for a new field, narrowed by what is typed and by what is there', async () => {
	const { component } = landing({
		fields: [PASSWORD, field({ name: 'PIN', kind: 'custom', value: '', empty: true })]
	});
	icon('Add a field').click();
	flushSync();
	await Promise.resolve();
	expect(host.textContent).toContain('Suggested');
	expect(suggestions(), 'a name the entry has was suggested').toEqual([
		'Account number',
		'Security answer'
	]);

	for (const [typed, offered] of [
		['acc', ['Account number']],
		['  SEC', ['Security answer']],
		['pin', []],
		['', ['Account number', 'Security answer']]
	] as const) {
		named().value = typed;
		named().dispatchEvent(new Event('input', { bubbles: true }));
		flushSync();
		expect(suggestions(), typed).toEqual(offered);
		expect(host.textContent?.includes('Suggested'), typed).toBe(offered.length > 0);
	}

	// Put away and opened again, nothing typed is remembered.
	named().value = 'acc';
	named().dispatchEvent(new Event('input', { bubbles: true }));
	icon('Never mind the new field').click();
	flushSync();
	icon('Add a field').click();
	flushSync();
	expect(suggestions()).toEqual(['Account number', 'Security answer']);

	return unmount(component);
});

/** A suggestion is a name already chosen: the field is made at once, hidden
 * or not as the suggestion says whatever the pill beside the name says, and
 * its value takes the focus. The typed half of a name is not made first. */
it('makes a suggested field the way it is suggested, and puts the reader in its value', async () => {
	ipc.setField.mockReset();
	const { component, props } = landing({ fields: [PASSWORD] });
	const made: Entry = {
		...props.entry,
		fields: [PASSWORD, field({ name: 'Account number', kind: 'custom', value: '', empty: true })]
	};
	ipc.setField.mockResolvedValue(made);

	icon('Add a field').click();
	flushSync();
	await Promise.resolve();
	named().value = 'Acc';
	named().dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
	const offer = icon('Add the field Account number');
	const press = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
	offer.dispatchEvent(press);
	expect(press.defaultPrevented, 'the suggestion took the focus off the name').toBe(true);
	offer.click();
	flushSync();

	expect(ipc.setField).toHaveBeenCalledTimes(1);
	expect(ipc.setField).toHaveBeenCalledWith(
		made.id,
		'Account number',
		'',
		false,
		expect.any(Number)
	);
	await vi.waitFor(() =>
		expect(document.activeElement).toBe(host.querySelector('textarea[aria-label="Account number"]'))
	);
	expect(host.querySelector('[aria-label="The name of the new field"]')).toBeNull();
	return unmount(component);
});

/** An entry just made opens with its name selected, to be typed over; one
 * opened any other way leaves the focus alone. A name the database protects
 * is drawn as the mask and takes nothing. */
it('puts the reader in the name of an entry just made, and only then', async () => {
	const named = [field({ name: 'Title', kind: 'title', value: 'Gmail copy', empty: false })];
	for (const [made, takes] of [
		[true, true],
		[false, false]
	] as const) {
		const component = mount(EntryView, {
			target: host,
			props: props({ entry: entry({ fields: named }), made })
		});
		flushSync();
		const title = host.querySelector<HTMLInputElement>('h1 input');
		expect(document.activeElement === title, String(made)).toBe(takes);
		if (takes) {
			expect(title?.selectionStart).toBe(0);
			expect(title?.selectionEnd).toBe('Gmail copy'.length);
		}
		title?.blur();
		await unmount(component);
	}

	const masked = mount(EntryView, {
		target: host,
		props: props({ entry: entry({ fields: titled(null) }), made: true })
	});
	flushSync();
	expect(document.activeElement, 'a protected name took the focus').toBe(document.body);
	return unmount(masked);
});

/** The fields an entry's kind writes in lines are written in lines from the
 * start, four lines tall, where Return starts a line: ten recovery codes are
 * not pasted into one. The rest of its fields of the reader's own are not. */
it('writes the fields its kind names in lines in lines while they are empty', () => {
	const component = mount(EntryView, {
		target: host,
		props: props({
			entry: entry({
				fields: [
					field({ name: 'Recovery codes', kind: 'custom', value: null, protected: true }),
					field({ name: 'Account', kind: 'custom', value: '' })
				]
			}),
			lined: ['Recovery codes']
		})
	});
	flushSync();
	const codes = host.querySelector<HTMLTextAreaElement>('textarea[aria-label="Recovery codes"]');
	expect(codes?.getAttribute('rows')).toBe('4');
	const returned_ = new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true });
	codes?.dispatchEvent(returned_);
	expect(returned_.defaultPrevented, 'Return finished the codes').toBe(false);
	expect(
		host.querySelector('textarea[aria-label="Account"]')?.getAttribute('rows'),
		'a field the kind does not write in lines was'
	).toBe('1');
	return unmount(component);
});
