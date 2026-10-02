import { flushSync, mount, unmount, type ComponentProps } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { settle } from '$lib/drafts';
import { drawing, entry, field, generated } from '$lib/fixtures';
import type { Field } from '$lib/model';
import { reactive } from '$lib/props.svelte';
import OwnField from './OwnField.svelte';

const ipc = vi.hoisted(() => ({
	reveal: vi.fn(),
	draft: vi.fn(),
	setField: vi.fn(),
	generator: vi.fn(),
	generatePassword: vi.fn(),
	// The same reading the real one does: a command rejects with the value Rust
	// serialised, and anything else is not one.
	asFailure: (thrown: unknown) =>
		thrown && typeof (thrown as { message?: unknown }).message === 'string'
			? (thrown as { code: string; message: string })
			: { code: 'other', message: 'Coffer could not finish that.' }
}));
vi.mock('$lib/ipc', () => ipc);

const SECRET = 'correct horse battery staple';
const MADE = 'q7Rk-2Vw9-Xm4T-Hp8L-Zn3B';

/** What the vault says when it could not finish something about a field. */
const BROKEN = { code: 'io', message: 'the disk is full' };

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.reveal.mockResolvedValue(SECRET);
	ipc.draft.mockResolvedValue(undefined);
	ipc.generator.mockResolvedValue(drawing());
	ipc.generatePassword.mockResolvedValue(generated(MADE));
});

// Everything, not just the host: a field a test put the focus in outside the
// row would otherwise hold it for the next test, which reads where it is.
afterEach(() => document.body.replaceChildren());

/** An entry of its own for every row, because what is typed is kept for the
 * life of the window and the window here is the whole file. */
let made = 0;
function next(): string {
	made += 1;
	return `entry-${made}`;
}

/** A field of the reader's own kept in the open. */
function visible(name: string, value: string): Field {
	return field({ name, kind: 'custom', value, empty: value === '' });
}

/** One the database protects, with something behind the mask. */
function masked(name: string): Field {
	return field({ name, kind: 'custom', protected: true, value: null, empty: false });
}

/** One the database protects with nothing in it: no value to reveal, and
 * written to where it stands. A field just named with Hidden on is this. */
function blank(name: string): Field {
	return field({ name, kind: 'custom', protected: true, value: null, empty: true });
}

type Props = ComponentProps<typeof OwnField>;

/** Draws one row. The props can be changed afterwards, which is how the pane
 * hands the row the field as it comes back from Rust. */
function show(over: Partial<Props> = {}) {
	const at = over.entry ?? next();
	const shown = over.field ?? visible('PIN', '4071');
	const props = reactive<Props>({
		entry: at,
		field: shown,
		draft: { entry: at, field: shown.name, protect: shown.protected },
		multiline: false,
		fresh: false,
		renamed: false,
		readOnly: false,
		removing: false,
		onCopy: vi.fn(),
		onWrite: vi.fn().mockResolvedValue(true),
		onPut: vi.fn().mockResolvedValue(undefined),
		onProtect: vi.fn().mockResolvedValue(undefined),
		onRename: vi.fn().mockResolvedValue(true),
		onRemove: vi.fn(),
		onKeep: vi.fn(),
		onFailure: vi.fn(),
		...over
	});
	const component = mount(OwnField, { target: host, props });
	flushSync();
	return { component, props };
}

/** What the pane does when it draws the row again: hands it an entry or a
 * field, and the place a value typed into it goes along with them. */
function hand(props: Props, over: { entry?: string; field?: Field }) {
	if (over.entry !== undefined) props.entry = over.entry;
	if (over.field !== undefined) props.field = over.field;
	props.draft = { entry: props.entry, field: props.field.name, protect: props.field.protected };
	flushSync();
}

/** Anything in the row by what it is called to a screen reader. */
function labelled(label: string): HTMLElement | null {
	return (
		[...host.querySelectorAll<HTMLElement>('[aria-label]')].find(
			(candidate) => candidate.getAttribute('aria-label') === label
		) ?? null
	);
}

function find(label: string): HTMLElement {
	const found = labelled(label);
	if (!found) throw new Error(`nothing labelled ${label}`);
	return found;
}

function button(text: string): HTMLButtonElement {
	const found = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === text
	);
	if (!found) throw new Error(`no button reading ${text}`);
	return found;
}

/** The lock beside the field called `name`. */
function lock(name: string): HTMLButtonElement {
	return find(`Keep ${name} hidden`) as HTMLButtonElement;
}

/** The name of the field called `name` as it is drawn, whether it can be
 * pressed to change it or not. */
function title(name: string): HTMLElement {
	const found = host.querySelector<HTMLElement>(`[title="${CSS.escape(name)}"]`);
	if (!found) throw new Error(`no name reading ${name}`);
	return found;
}

/** The field the value of `name` is written in where it stands. */
function value(name: string): HTMLTextAreaElement {
	const found = host.querySelector<HTMLTextAreaElement>(
		`textarea[aria-label="${CSS.escape(name)}"]`
	);
	if (!found) throw new Error(`no field for the value of ${name}`);
	return found;
}

/** A field elsewhere in the window, where the reader can put the focus. */
function outside(): HTMLInputElement {
	const elsewhere = document.createElement('input');
	document.body.appendChild(elsewhere);
	return elsewhere;
}

/** Every attribute anywhere in the row. A secret in one of these is a secret
 * in the accessibility tree, in a copied element and in a crash dump. */
function attributes(): Attr[] {
	return [...host.querySelectorAll('*')].flatMap((element) => [...element.attributes]);
}

/** Everything in the fields of the row that can be typed into: a property of
 * a node rather than an attribute or a text node. */
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

function key(at: HTMLElement, name: string, over: KeyboardEventInit = {}): KeyboardEvent {
	const pressed = new KeyboardEvent('keydown', {
		key: name,
		bubbles: true,
		cancelable: true,
		...over
	});
	at.dispatchEvent(pressed);
	return pressed;
}

/** Lets everything already on its way arrive, so that something that has not
 * happened by then is something that waits. */
function settled(): Promise<void> {
	return new Promise((done) => setTimeout(done, 0));
}

/** Opens the name of the field called `name` for writing. */
function rename(name: string): HTMLInputElement {
	find(`Rename ${name}`).click();
	flushSync();
	return find(`New name for ${name}`) as HTMLInputElement;
}

/** Opens the generator under the field called `name`, and waits for the
 * value it makes. */
async function generate(name: string) {
	find(`Make one for ${name}`).click();
	flushSync();
	await vi.waitFor(() => expect(host.textContent).toContain(MADE));
	flushSync();
}

/** Writes a value typed where it stands the way the pane does: through the
 * vault, finishing what was typed there, and taken once it has landed. */
function writes(at: string) {
	return vi.fn((typed: string) =>
		settle({ entry: at, field: 'PIN', protect: false }, (sequence) =>
			ipc.setField(at, 'PIN', typed, false, sequence)
		).then(() => true)
	);
}

/** Which of the presses that change the field called `name` would take a
 * press now. One that is not drawn, or is drawn disabled, would not. */
function offered(name: string) {
	const takes = (label: string) => {
		const found = labelled(label);
		return found instanceof HTMLButtonElement && !found.disabled;
	};
	return {
		lock: takes(`Keep ${name} hidden`),
		change: takes(`Change ${name}`),
		makeOne: takes(`Make one for ${name}`),
		rename: takes(`Rename ${name}`)
	};
}

/** Presses everything in the row that changes the field called `name`, the
 * way an impatient reader would, whether it is on offer or not. */
function pressEverything(name: string) {
	for (const label of [`Keep ${name} hidden`, `Change ${name}`, `Make one for ${name}`]) {
		labelled(label)?.click();
		flushSync();
	}
	title(name).click();
	flushSync();
}

/** What is open under the field called `name`. */
function opened(name: string) {
	return {
		change: labelled(`New value of ${name}`) !== null,
		generator: labelled(`Generator for ${name}`) !== null,
		rename: labelled(`New name for ${name}`) !== null
	};
}

const NOTHING_OPEN = { change: false, generator: false, rename: false };
const ALL_OFFERED = { lock: true, change: true, makeOne: true, rename: true };
const NONE_OFFERED = { lock: false, change: false, makeOne: false, rename: false };

/**
 * The lock is a switch: pressed, it asks for the field to be what it is not,
 * on the entry it is drawn under, and it says what the field is now. An empty
 * hidden field shows that in no other way, so the lock says it there too.
 */
it('asks for the opposite of what the field is, and says what it is', async () => {
	for (const shown of [visible('PIN', '4071'), visible('PIN', ''), masked('PIN'), blank('PIN')]) {
		const onProtect = vi.fn().mockResolvedValue(undefined);
		const { component, props } = show({ field: shown, onProtect });
		const said = JSON.stringify(shown);

		expect(lock('PIN').getAttribute('aria-pressed'), said).toBe(String(shown.protected));
		expect(lock('PIN').querySelector('use')?.getAttribute('href'), said).toBe(
			shown.protected ? '#i-lock' : '#i-unlock'
		);
		expect(lock('PIN').title, said).toBe(
			shown.protected
				? 'Hidden until shown. Press to keep it in the open.'
				: 'Kept in the open. Press to hide it.'
		);

		lock('PIN').click();
		await vi.waitFor(() => expect(onProtect, said).toHaveBeenCalledTimes(1));
		expect(onProtect, said).toHaveBeenCalledWith(props.entry, !shown.protected);

		await unmount(component);
	}
});

/**
 * The lock says whether the value is hidden, which an empty field shows no
 * other way, so it stands before the name in the name's column on every row.
 * In the value's row it took the width a revealed value is read in, and it
 * stood between the copy and the trash, where a press meant for one could
 * land on another.
 */
it('keeps the lock before the name, out of the row the value is read in', () => {
	for (const shown of [visible('PIN', '4071'), visible('PIN', ''), masked('PIN'), blank('PIN')]) {
		const { component } = show({ field: shown });
		const said = JSON.stringify(shown);
		const name = find('Rename PIN');
		const trash = find('Remove the field PIN');

		expect(lock('PIN').parentElement, said).toBe(name.parentElement);
		expect(lock('PIN').compareDocumentPosition(name), said).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
		expect(trash.parentElement?.contains(lock('PIN')), said).toBe(false);
		expect(trash.parentElement?.lastElementChild, `${said}: the trash is last`).toBe(trash);
		if (shown.value !== null && !shown.empty) {
			expect(find('Copy PIN').nextElementSibling, said).toBe(trash);
		}

		unmount(component);
	}
});

/**
 * Two presses before the first is answered are not two answers. The second
 * read the field as it was before the first landed and asked for the same
 * thing again - or, once the first had landed, undid it.
 */
it('takes one press of the lock until the first is answered', async () => {
	const answer = Promise.withResolvers<void>();
	const onProtect = vi.fn(() => answer.promise);
	const { component } = show({ field: visible('PIN', '4071'), onProtect });

	lock('PIN').click();
	lock('PIN').click();
	await vi.waitFor(() => expect(onProtect).toHaveBeenCalledTimes(1));
	flushSync();
	expect(lock('PIN').disabled, 'the lock offered a second press while the first was out').toBe(
		true
	);

	lock('PIN').click();
	await settled();
	expect(onProtect).toHaveBeenCalledTimes(1);

	// Answered, it is a lock again rather than one stuck where it was.
	answer.resolve();
	await vi.waitFor(() => expect(lock('PIN').disabled).toBe(false));
	lock('PIN').click();
	await vi.waitFor(() => expect(onProtect).toHaveBeenCalledTimes(2));

	return unmount(component);
});

/**
 * Until the lock's answer lands, the row drawn then may have no Change in it
 * or be another row altogether: a Change opened meanwhile went with
 * everything typed in it, a generator opened for a field coming back in the
 * open would make a password for plain text, and a rename sent meanwhile was
 * about a field Rust was moving. Answered or failed, the row is a row again
 * rather than one stuck where the press left it.
 */
it('offers no Change, no Make one, no second lock and no rename until the lock is answered', async () => {
	for (const ending of ['answered', 'failed'] as const) {
		const answer = Promise.withResolvers<void>();
		const onProtect = vi.fn(() => answer.promise);
		const { component } = show({ field: masked('PIN'), onProtect });
		expect(offered('PIN'), ending).toEqual(ALL_OFFERED);

		lock('PIN').click();
		await vi.waitFor(() => expect(onProtect, ending).toHaveBeenCalledTimes(1));
		flushSync();
		expect(offered('PIN'), ending).toEqual(NONE_OFFERED);
		expect(title('PIN').tagName, ending).toBe('SPAN');

		pressEverything('PIN');
		await settled();
		expect(opened('PIN'), ending).toEqual(NOTHING_OPEN);
		expect(onProtect, ending).toHaveBeenCalledTimes(1);

		if (ending === 'answered') answer.resolve();
		else answer.reject(BROKEN);
		await vi.waitFor(() => expect(offered('PIN'), ending).toEqual(ALL_OFFERED));

		await unmount(component);
	}
});

/**
 * A new name is on its way to the same field the lock moves and Change and
 * Make one write to. Until it lands, the field may come back under the new
 * name or the old one, so nothing is pressed meanwhile; taken, refused or
 * failed, everything comes back.
 */
it('offers no Change, no Make one, no lock and no second rename until a rename is answered', async () => {
	for (const ending of ['taken', 'refused', 'failed'] as const) {
		const answer = Promise.withResolvers<boolean>();
		const onRename = vi.fn(() => answer.promise);
		const { component } = show({ field: masked('PIN'), onRename });

		const input = rename('PIN');
		input.value = 'Card PIN';
		key(input, 'Enter');
		await vi.waitFor(() => expect(onRename, ending).toHaveBeenCalledTimes(1));
		flushSync();
		expect(offered('PIN'), ending).toEqual(NONE_OFFERED);
		expect(title('PIN').tagName, ending).toBe('SPAN');

		pressEverything('PIN');
		await settled();
		expect(opened('PIN'), ending).toEqual(NOTHING_OPEN);
		expect(onRename, ending).toHaveBeenCalledTimes(1);

		if (ending === 'failed') answer.reject(BROKEN);
		else answer.resolve(ending === 'taken');
		await vi.waitFor(() => expect(offered('PIN'), ending).toEqual(ALL_OFFERED));

		await unmount(component);
	}
});

/**
 * A lock or a rename that could not be finished is said, and not left to
 * reject where nobody hears it: a press that did nothing and said nothing
 * looks like one that worked.
 */
it('says so when the lock or a rename fails, rather than leaving it unheard', async () => {
	const onFailure = vi.fn();
	const onProtect = vi.fn().mockRejectedValue(BROKEN);
	const onRename = vi.fn().mockRejectedValue(BROKEN);
	const { component } = show({ field: visible('PIN', '4071'), onProtect, onRename, onFailure });

	lock('PIN').click();
	await vi.waitFor(() => expect(onProtect).toHaveBeenCalledTimes(1));
	await vi.waitFor(() => expect(onFailure).toHaveBeenCalledTimes(1));
	expect(onFailure).toHaveBeenCalledWith(BROKEN);

	await vi.waitFor(() => expect(offered('PIN').rename).toBe(true));
	const input = rename('PIN');
	input.value = 'Card PIN';
	key(input, 'Enter');
	await vi.waitFor(() => expect(onRename).toHaveBeenCalledTimes(1));
	await vi.waitFor(() => expect(onFailure).toHaveBeenCalledTimes(2));
	expect(onFailure).toHaveBeenLastCalledWith(BROKEN);

	return unmount(component);
});

/**
 * The generator stays up when the lock is pressed to show the field, and Put
 * it in the field pressed before the lock's answer lands wrote a made
 * password into a field on its way into the open, where anyone can read it.
 * That is what the generator is put away for once the field comes back in
 * the open; the press before it lands is no different.
 */
it('puts no made value in while the lock is on its way', async () => {
	const answer = Promise.withResolvers<void>();
	const onProtect = vi.fn(() => answer.promise);
	const onPut = vi.fn().mockResolvedValue(undefined);
	const { component, props } = show({ field: masked('PIN'), onProtect, onPut });

	await generate('PIN');
	lock('PIN').click();
	await vi.waitFor(() => expect(onProtect).toHaveBeenCalledWith(props.entry, false));
	flushSync();

	[...host.querySelectorAll('button')]
		.find((each) => each.textContent?.trim() === 'Put it in the field')
		?.click();
	flushSync();
	await settled();
	expect(onPut, 'a made value went into a field on its way into the open').not.toHaveBeenCalled();

	answer.resolve();
	await settled();
	hand(props, { field: visible('PIN', SECRET) });
	await settled();
	expect(onPut).not.toHaveBeenCalled();
	expect(host.textContent).not.toContain(MADE);

	return unmount(component);
});

/**
 * The field a value in the open is written in stays on the screen while a new
 * name is on its way. A value typed there and left went to Rust under the old
 * name beside the rename, and when the rename landed first the write made a
 * field of the old name again with the new value in it, beside the renamed
 * one with the old value.
 */
it('writes nothing typed into the value while a new name is on its way', async () => {
	const answer = Promise.withResolvers<boolean>();
	const onWrite = vi.fn().mockResolvedValue(true);
	const onRename = vi.fn(() => answer.promise);
	const { component } = show({ field: visible('PIN', '4071'), onWrite, onRename });

	const input = rename('PIN');
	input.value = 'Card PIN';
	key(input, 'Enter');
	await vi.waitFor(() => expect(onRename).toHaveBeenCalledTimes(1));
	flushSync();

	const typing = host.querySelector<HTMLTextAreaElement>('textarea[aria-label="PIN"]');
	if (typing) {
		enter(typing, '9314');
		typing.dispatchEvent(new FocusEvent('blur'));
	}
	await settled();
	expect(onWrite, 'a value went to Rust under the name being changed').not.toHaveBeenCalled();

	answer.resolve(true);
	return unmount(component);
});

/**
 * A new value typed in a Change is on its way to this field under this
 * protection, and Rust keeps it beside the field at a lock under the
 * protection it was typed for. The lock stays where it is until the Change
 * is closed, beside the name, and says why it will not move.
 */
it('keeps the lock where it is while a new value is being written in its Change', async () => {
	const onProtect = vi.fn().mockResolvedValue(undefined);
	const { component } = show({ field: masked('PIN'), onProtect });

	find('Change PIN').click();
	flushSync();
	expect(find('New value of PIN')).toBeInstanceOf(HTMLTextAreaElement);
	expect(lock('PIN').disabled).toBe(true);
	expect(lock('PIN').title).toBe('Save or cancel the new value first.');
	expect(lock('PIN').parentElement).toBe(title('PIN').parentElement);

	lock('PIN').click();
	await settled();
	expect(onProtect).not.toHaveBeenCalled();

	button('Cancel').click();
	flushSync();
	expect(lock('PIN').disabled).toBe(false);
	expect(lock('PIN').title).toBe('Hidden until shown. Press to keep it in the open.');

	return unmount(component);
});

/**
 * Leaving the field to press the lock writes what was typed in it, and the
 * write and the lock are two commands Tauri runs side by side. A lock that
 * did not wait moved the value Rust held before the write, and the entry
 * drawn last was the one from before the press.
 */
it('lets a value on its way land before the lock moves it', async () => {
	const at = next();
	const landing = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.setField.mockReturnValue(landing.promise);
	const onProtect = vi.fn().mockResolvedValue(undefined);
	const { component } = show({
		entry: at,
		field: visible('PIN', '4071'),
		onWrite: writes(at),
		onProtect
	});

	enter(value('PIN'), '9314');
	value('PIN').dispatchEvent(new FocusEvent('blur'));
	expect(ipc.setField).toHaveBeenCalledWith(at, 'PIN', '9314', false, expect.any(Number));

	lock('PIN').click();
	await settled();
	expect(onProtect, 'the lock moved a value that had not landed').not.toHaveBeenCalled();

	landing.resolve(entry());
	await vi.waitFor(() => expect(onProtect).toHaveBeenCalledWith(at, true));
	expect(ipc.setField.mock.invocationCallOrder[0]).toBeLessThan(
		onProtect.mock.invocationCallOrder[0]
	);

	return unmount(component);
});

/**
 * The press was about the entry on the screen when it was made. The write it
 * waits for can take as long as the disk does, and the reader may have
 * opened another entry by then, with a field of the same name; the lock moves
 * the field it was pressed beside, not that one.
 */
it('hides the field on the entry the lock was pressed on, whatever entry is up when it moves', async () => {
	const at = next();
	const landing = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.setField.mockReturnValue(landing.promise);
	const onProtect = vi.fn().mockResolvedValue(undefined);
	const { component, props } = show({
		entry: at,
		field: visible('PIN', '4071'),
		onWrite: writes(at),
		onProtect
	});

	enter(value('PIN'), '9314');
	value('PIN').dispatchEvent(new FocusEvent('blur'));
	lock('PIN').click();
	await settled();
	expect(onProtect).not.toHaveBeenCalled();

	const later = next();
	hand(props, { entry: later });
	landing.resolve(entry());
	await vi.waitFor(() => expect(onProtect).toHaveBeenCalledTimes(1));
	expect(onProtect, 'the lock moved a field on the entry opened since').toHaveBeenCalledWith(
		at,
		true
	);

	return unmount(component);
});

/** Typing that Rust has not heard yet is told at the press, and the lock
 * waits for it to arrive: a lock of the vault in between would otherwise
 * find a draft of the field under the protection it no longer has. */
it('tells Rust what was typed and not yet told before the lock moves', async () => {
	const at = next();
	const telling = Promise.withResolvers<void>();
	ipc.draft.mockReturnValue(telling.promise);
	const onProtect = vi.fn().mockResolvedValue(undefined);
	const { component } = show({ entry: at, field: visible('PIN', '4071'), onProtect });

	enter(value('PIN'), '93');
	lock('PIN').click();
	expect(ipc.draft).toHaveBeenCalledWith(at, 'PIN', '93', false, false, expect.any(Number));

	await settled();
	expect(onProtect, 'the lock moved before the draft arrived').not.toHaveBeenCalled();

	telling.resolve();
	await vi.waitFor(() => expect(onProtect).toHaveBeenCalledWith(at, true));

	key(value('PIN'), 'Escape');
	return unmount(component);
});

/**
 * Once the lock has landed the row is drawn from the field Rust sends back.
 * A value that was in the open and is hidden now is a mask: nothing of it is
 * left in a field or in the text of the row.
 */
it('takes a value off the screen once the field comes back hidden', async () => {
	const { component, props } = show({ field: visible('PIN', '4071') });

	lock('PIN').click();
	await vi.waitFor(() => expect(props.onProtect).toHaveBeenCalledWith(props.entry, true));
	hand(props, { field: masked('PIN') });

	expect(lock('PIN').getAttribute('aria-pressed')).toBe('true');
	expect(written()).not.toContain('4071');
	expect(host.textContent).not.toContain('4071');
	expect(find('Show PIN')).toBeInstanceOf(HTMLButtonElement);
	expect(find('Make one for PIN')).toBeInstanceOf(HTMLButtonElement);

	return unmount(component);
});

/**
 * Make one is for a value somebody should not have to think up and nobody
 * should read over a shoulder: a hidden field, whether there is something
 * behind its mask or nothing yet. A field in the open is not offered one, and
 * a row nothing may change offers nothing that changes it.
 */
it('offers to make a value for a hidden field only, and never where nothing may change', () => {
	const cases: [Field, boolean, boolean][] = [
		[masked('PIN'), false, true],
		[blank('PIN'), false, true],
		[visible('PIN', '4071'), false, false],
		[visible('PIN', ''), false, false],
		[masked('PIN'), true, false],
		[blank('PIN'), true, false]
	];
	for (const [shown, readOnly, offers] of cases) {
		const { component } = show({ field: shown, readOnly });
		expect(labelled('Make one for PIN') !== null, JSON.stringify({ shown, readOnly })).toBe(offers);
		unmount(component);
	}
});

/**
 * Two hidden fields each offer Make one, and the password's generator can be
 * open beside theirs. "Make one" read out twice, or a panel read out as the
 * password's when a field opened it, says nothing about where a made value
 * would go. Each says which field it is for, and a field's generator asks
 * Rust for the recipe fields remember rather than the password's.
 */
it('says which field Make one and the generator it opens are for', async () => {
	const pin = show({ field: masked('PIN') });
	const code = show({ field: blank('Recovery code') });

	expect(find('Make one for PIN').textContent?.trim()).toBe('Make one');
	expect(find('Make one for Recovery code').textContent?.trim()).toBe('Make one');

	await generate('Recovery code');
	expect(find('Generator for Recovery code').getAttribute('role')).toBe('group');
	expect(labelled('Generator for PIN')).toBeNull();
	expect(find('Make one for Recovery code').getAttribute('aria-expanded')).toBe('true');
	expect(find('Make one for PIN').getAttribute('aria-expanded')).toBe('false');
	expect(ipc.generator.mock.calls).toEqual([['field']]);
	expect(ipc.generatePassword.mock.calls.map((call) => call[1])).toEqual(['field']);

	unmount(pin.component);
	return unmount(code.component);
});

/**
 * The made value goes into the field through the vault and nowhere else: not
 * into the field an empty hidden value is typed in, where it would be a
 * protected value sitting in a DOM property, and not into an attribute. A
 * second press of Put it in the field in the same moment puts nothing more.
 */
it('puts a made value in once, through the vault, and leaves it nowhere else', async () => {
	for (const shown of [masked('PIN'), blank('PIN')]) {
		const onPut = vi.fn().mockResolvedValue(undefined);
		const { component } = show({ field: shown, onPut });
		const said = JSON.stringify(shown);

		expect(find('Make one for PIN').getAttribute('aria-expanded'), said).toBe('false');
		await generate('PIN');
		expect(find('Make one for PIN').getAttribute('aria-expanded'), said).toBe('true');
		expect(
			attributes().some((attribute) => attribute.value.includes(MADE)),
			said
		).toBe(false);
		expect(written(), said).not.toContain(MADE);

		const put = button('Put it in the field');
		put.click();
		put.click();
		flushSync();

		expect(onPut, said).toHaveBeenCalledTimes(1);
		expect(onPut, said).toHaveBeenCalledWith(MADE);
		expect(host.textContent, said).not.toContain(MADE);
		expect(written(), said).not.toContain(MADE);
		expect(
			[...host.querySelectorAll('button')].some(
				(each) => each.textContent?.trim() === 'Put it in the field'
			),
			said
		).toBe(false);
		// The focus comes back to the press that asked for the value, rather
		// than falling out of the pane, where the next Escape closes the entry.
		await vi.waitFor(() => expect(document.activeElement, said).toBe(find('Make one for PIN')));
		expect(find('Make one for PIN').getAttribute('aria-expanded'), said).toBe('false');

		await unmount(component);
	}
});

/** A made value the vault refuses is said, and not dropped in silence or
 * left to reject where nobody hears it. */
it('says so when the vault will not take a made value', async () => {
	const refusal = { code: 'readOnly', message: 'This database is open read only.' };
	const onPut = vi.fn().mockRejectedValue(refusal);
	const onFailure = vi.fn();
	const { component } = show({ field: masked('PIN'), onPut, onFailure });

	await generate('PIN');
	button('Put it in the field').click();
	flushSync();

	await vi.waitFor(() => expect(onFailure).toHaveBeenCalledWith(refusal));
	expect(onFailure).toHaveBeenCalledTimes(1);
	expect(JSON.stringify(onFailure.mock.calls)).not.toContain(MADE);

	return unmount(component);
});

/**
 * A Change and the generator are two ways to give the field a new value. A
 * made value put in while a typed one waited beside it was overwritten by the
 * typed one a moment later, so opening the one puts the other away, and the
 * made value goes off the screen with it.
 */
it('puts the generator away when a Change is opened beside it', async () => {
	const onPut = vi.fn().mockResolvedValue(undefined);
	const { component } = show({ field: masked('PIN'), onPut });

	await generate('PIN');
	find('Change PIN').click();
	flushSync();

	expect(labelled('Close the generator')).toBeNull();
	expect(host.textContent).not.toContain(MADE);
	expect(find('New value of PIN')).toBeInstanceOf(HTMLTextAreaElement);
	expect(written()).not.toContain(MADE);
	expect(onPut).not.toHaveBeenCalled();

	// Closed again, the line offers both, and the generator is not still open
	// behind a press that says it is.
	button('Cancel').click();
	flushSync();
	expect(find('Make one for PIN').getAttribute('aria-expanded')).toBe('false');
	expect(labelled('Close the generator')).toBeNull();

	return unmount(component);
});

/** An entry that cannot be changed any more - moved to the bin, or the vault
 * found to be one Coffer will not write - has nowhere to put a made value. */
it('takes the generator away when the entry stops being one that can change', async () => {
	const { component, props } = show({ field: masked('PIN') });

	await generate('PIN');
	props.readOnly = true;
	flushSync();

	expect(labelled('Close the generator')).toBeNull();
	expect(host.textContent).not.toContain(MADE);
	expect(props.onPut).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * Make one is offered for a hidden field only. The lock stays pressable while
 * the generator is open, and a field that comes back in the open has no Make
 * one left to say the generator is open or to close it, while Put it in the
 * field would write a made password where anyone can read it.
 */
it('puts the generator away when the field stops being hidden', async () => {
	const { component, props } = show({ field: masked('PIN') });

	await generate('PIN');
	lock('PIN').click();
	await vi.waitFor(() => expect(props.onProtect).toHaveBeenCalledWith(props.entry, false));
	hand(props, { field: visible('PIN', SECRET) });

	expect(labelled('Make one for PIN')).toBeNull();
	expect(
		labelled('Close the generator'),
		'the generator outlived the Make one that opened it'
	).toBeNull();
	expect(host.textContent).not.toContain(MADE);
	expect(props.onPut).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * The generator is open for the field under the name it has. A rename draws
 * the row again under another, and the made value on the screen and what the
 * reader set the generator to went with the old one, so the name is read
 * rather than pressed while the generator is up.
 */
it('offers no rename while the generator is open', async () => {
	const onRename = vi.fn().mockResolvedValue(true);
	const { component } = show({ field: masked('PIN'), onRename });

	await generate('PIN');
	expect(labelled('Rename PIN')).toBeNull();
	expect(title('PIN').tagName).toBe('SPAN');
	title('PIN').click();
	flushSync();
	expect(labelled('New name for PIN')).toBeNull();
	expect(host.textContent).toContain(MADE);

	find('Close the generator').click();
	flushSync();
	expect(find('Rename PIN')).toBeInstanceOf(HTMLButtonElement);
	expect(onRename).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * A value in the open is copied through Rust, which writes it concealed and
 * clears it again, rather than selected out of a field one stray key changes
 * and landed on the ordinary pasteboard. There is nothing to copy from an
 * empty one. Copying needs nothing to be changeable, so a row nothing may
 * change still offers it.
 */
it('copies a value in the open through Rust, and offers no copy of nothing', () => {
	const cases: [Field, boolean, boolean][] = [
		[visible('PIN', '4071'), false, true],
		[visible('PIN', '4071'), true, true],
		[visible('PIN', ''), false, false],
		[visible('PIN', ''), true, false]
	];
	for (const [shown, readOnly, offers] of cases) {
		const onCopy = vi.fn();
		const { component } = show({ field: shown, readOnly, onCopy });
		const said = JSON.stringify({ shown, readOnly });

		const copy = labelled('Copy PIN');
		expect(copy !== null, said).toBe(offers);
		if (copy) {
			copy.click();
			expect(onCopy, said).toHaveBeenCalledTimes(1);
			expect(onCopy, said).toHaveBeenCalledWith(null);
		}

		unmount(component);
	}
});

/** A hidden value is copied by name in Rust, and nothing is read into the
 * window to do it. */
it('copies a hidden value without reading it into the window', async () => {
	const onCopy = vi.fn();
	const { component } = show({ field: masked('PIN'), onCopy });

	find('Copy PIN').click();
	await settled();

	expect(onCopy).toHaveBeenCalledWith(null);
	expect(ipc.reveal).not.toHaveBeenCalled();
	expect(host.textContent).not.toContain(SECRET);

	return unmount(component);
});

/**
 * A name pressed is a name to change: the field it is written in has the
 * whole name, selected so that typing replaces it, and the focus. Ninety-six
 * pixels cut a long name short, so the press carries the whole of it.
 */
it('opens the name for writing whole, selected, and with the focus', () => {
	const long = `Recovery code for the account ${'x'.repeat(200)}`;
	const { component } = show({ field: visible(long, '4071') });

	expect(find(`Rename ${long}`).getAttribute('title')).toBe(long);

	const input = rename(long);
	expect(input.value).toBe(long);
	expect(document.activeElement).toBe(input);
	expect([input.selectionStart, input.selectionEnd]).toEqual([0, long.length]);

	key(input, 'Escape');
	return unmount(component);
});

/**
 * Return finishes the name, with the spaces at either end left off the way
 * they are when a field is made, on the entry it was drawn under, and says it
 * was finished with Return. The field it was written in goes, and a browser
 * blurs a field that goes with the focus in it: that blur is not a second
 * rename.
 */
it('renames on Return to the name without its spaces, once', async () => {
	const onRename = vi.fn().mockResolvedValue(true);
	const { component, props } = show({ onRename });

	const input = rename('PIN');
	input.value = '  Card PIN  ';
	expect(key(input, 'Enter').defaultPrevented).toBe(true);
	input.dispatchEvent(new FocusEvent('blur'));
	flushSync();
	input.dispatchEvent(new FocusEvent('blur'));

	await vi.waitFor(() => expect(onRename).toHaveBeenCalledWith(props.entry, 'Card PIN', true));
	await settled();
	expect(onRename).toHaveBeenCalledTimes(1);
	expect(input.isConnected).toBe(false);
	expect(find('Rename PIN')).toBeInstanceOf(HTMLButtonElement);

	return unmount(component);
});

/** Leaving the name finishes it too, the way leaving the name of a new field
 * does, and leaving it twice is one rename - one that was not finished with
 * Return. */
it('renames when the name is left, once', async () => {
	const onRename = vi.fn().mockResolvedValue(true);
	const { component, props } = show({ onRename });

	const input = rename('PIN');
	input.value = 'Card PIN';
	input.dispatchEvent(new FocusEvent('blur'));
	input.dispatchEvent(new FocusEvent('blur'));
	flushSync();

	await vi.waitFor(() => expect(onRename).toHaveBeenCalledWith(props.entry, 'Card PIN', false));
	await settled();
	expect(onRename).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/**
 * Nothing, spaces, or the name the field already has is no new name: a
 * rename to it would be a version of the entry with nothing changed, or a
 * field with no name the vault would have to refuse. A name that differs
 * only in case is a name of its own, and goes to Rust.
 */
it('renames to nothing, to spaces and to the same name as nothing at all', async () => {
	for (const finish of ['Return', 'leaving'] as const) {
		for (const typed of ['', '   ', '\t\n ', 'PIN', '  PIN  ', '\u00a0PIN\u00a0']) {
			const onRename = vi.fn().mockResolvedValue(true);
			const { component } = show({ onRename });
			const said = JSON.stringify({ finish, typed });

			const input = rename('PIN');
			input.value = typed;
			if (finish === 'Return') key(input, 'Enter');
			else input.dispatchEvent(new FocusEvent('blur'));
			flushSync();
			await settled();

			expect(onRename, said).not.toHaveBeenCalled();
			expect(input.isConnected, said).toBe(false);
			expect(find('Rename PIN'), said).toBeInstanceOf(HTMLButtonElement);

			await unmount(component);
		}
	}

	const onRename = vi.fn().mockResolvedValue(true);
	const { component, props } = show({ onRename });
	const input = rename('PIN');
	input.value = 'pin';
	key(input, 'Enter');
	await vi.waitFor(() => expect(onRename).toHaveBeenCalledWith(props.entry, 'pin', true));

	return unmount(component);
});

/**
 * A name another program wrote with a space at either end is the name the
 * file holds, and opening it to read it whole is not asking for the space to
 * go. Left as it was, by leaving it or with Return, it is no rename at all:
 * one to the name without its spaces would be a version of the entry nobody
 * asked for. Taking the space off by hand is a rename like any other.
 */
it('leaves a name with spaces at either end alone when it is opened and left as it was', async () => {
	for (const name of ['PIN ', ' PIN', '  PIN  ', 'PIN\u00a0']) {
		for (const finish of ['Return', 'leaving'] as const) {
			const onRename = vi.fn().mockResolvedValue(true);
			const { component } = show({ field: visible(name, '4071'), onRename });
			const said = JSON.stringify({ name, finish });

			const input = rename(name);
			expect(input.value, said).toBe(name);
			if (finish === 'Return') key(input, 'Enter');
			else input.dispatchEvent(new FocusEvent('blur'));
			flushSync();
			await settled();

			expect(onRename, said).not.toHaveBeenCalled();
			expect(find(`Rename ${name}`), said).toBeInstanceOf(HTMLButtonElement);

			await unmount(component);
		}
	}

	const onRename = vi.fn().mockResolvedValue(true);
	const { component, props } = show({ field: visible('PIN ', '4071'), onRename });
	const input = rename('PIN ');
	input.value = 'PIN';
	key(input, 'Enter');
	await vi.waitFor(() => expect(onRename).toHaveBeenCalledWith(props.entry, 'PIN', true));

	return unmount(component);
});

/**
 * Escape puts the name back and is the name's: the window reads the same key
 * as "close the entry", and a name put back is not an entry put away. The
 * blur that comes with the field going does not write what was typed before
 * it.
 */
it('puts the name back on Escape without closing the entry', async () => {
	const onRename = vi.fn().mockResolvedValue(true);
	const closing = vi.fn();
	window.addEventListener('keydown', closing);
	try {
		const { component } = show({ onRename });

		const input = rename('PIN');
		input.value = 'Card PIN';
		key(input, 'Escape');
		input.dispatchEvent(new FocusEvent('blur'));
		flushSync();
		await settled();

		expect(closing, 'the Escape reached the window').not.toHaveBeenCalled();
		expect(onRename).not.toHaveBeenCalled();
		expect(input.isConnected).toBe(false);
		expect(find('Rename PIN').textContent?.trim()).toBe('PIN');

		await unmount(component);
	} finally {
		window.removeEventListener('keydown', closing);
	}
});

/**
 * The field a name is written in goes with the focus in it when the name is
 * put back with Escape or finished with a Return that changes nothing, and
 * the focus fell out of the pane - where the next Escape closes the entry.
 * It goes back to the name instead, where the reader can press it again.
 */
it('gives the focus back to the name after Escape, or a Return that changes nothing', async () => {
	const cases: [string, string, string][] = [
		['PIN', 'Card PIN', 'Escape'],
		['PIN', 'PIN', 'Enter'],
		['PIN', '  PIN  ', 'Enter'],
		['PIN', '', 'Enter'],
		['PIN ', 'PIN ', 'Enter'],
		['PIN ', 'Card PIN', 'Escape']
	];
	for (const [name, typed, pressed] of cases) {
		const onRename = vi.fn().mockResolvedValue(true);
		const { component } = show({ field: visible(name, '4071'), onRename });
		const said = JSON.stringify({ name, typed, pressed });

		const input = rename(name);
		input.value = typed;
		key(input, pressed);
		flushSync();

		await vi.waitFor(() => expect(document.activeElement, said).toBe(find(`Rename ${name}`)));
		expect(onRename, said).not.toHaveBeenCalled();

		await unmount(component);
	}
});

/**
 * A rename finished with Return and refused - a name the entry already has -
 * leaves the reader where they were, on the name, rather than nowhere. One
 * finished by leaving the name leaves the focus where the reader took it,
 * even to nothing: they put it there before the refusal came back.
 */
it('gives the focus back to the name when a rename finished with Return is refused, and only then', async () => {
	const answer = Promise.withResolvers<boolean>();
	const onRename = vi.fn(() => answer.promise);
	const typed = show({ onRename });

	const input = rename('PIN');
	input.value = 'Card PIN';
	key(input, 'Enter');
	flushSync();
	await vi.waitFor(() =>
		expect(onRename).toHaveBeenCalledWith(typed.props.entry, 'Card PIN', true)
	);
	expect(document.activeElement).toBe(document.body);

	answer.resolve(false);
	await vi.waitFor(() => expect(document.activeElement).toBe(find('Rename PIN')));
	await unmount(typed.component);

	const elsewhere = outside();
	for (const to of [elsewhere, null]) {
		const refused = vi.fn().mockResolvedValue(false);
		const left = show({ onRename: refused });
		const said = to === null ? 'left to nothing' : 'left to another field';

		const leaving = rename('PIN');
		leaving.value = 'Card PIN';
		if (to) to.focus();
		else leaving.blur();
		flushSync();
		await vi.waitFor(() =>
			expect(refused, said).toHaveBeenCalledWith(left.props.entry, 'Card PIN', false)
		);
		await settled();
		flushSync();
		expect(document.activeElement, said).toBe(to ?? document.body);

		await unmount(left.component);
	}
});

/**
 * The refusal of a rename finished with Return can take as long as the disk
 * does, and a reader who has gone on to another field meanwhile is not pulled
 * back to the name.
 */
it('leaves the focus where the reader has put it since a refused rename was sent', async () => {
	const answer = Promise.withResolvers<boolean>();
	const onRename = vi.fn(() => answer.promise);
	const { component } = show({ onRename });
	const elsewhere = outside();

	const input = rename('PIN');
	input.value = 'Card PIN';
	key(input, 'Enter');
	flushSync();
	await vi.waitFor(() => expect(onRename).toHaveBeenCalledTimes(1));
	elsewhere.focus();

	answer.resolve(false);
	await vi.waitFor(() => expect(find('Rename PIN')).toBeInstanceOf(HTMLButtonElement));
	await settled();
	expect(document.activeElement).toBe(elsewhere);

	return unmount(component);
});

/**
 * A field renamed with Return comes back as a row of its own under the new
 * name, and the focus that was in the old row's name went with the old row.
 * The new row's name takes it, the way Escape gives it back - unless the
 * reader has put it somewhere since. A row already drawn takes nothing when
 * the pane says its name is the one being renamed to: that is a rename to a
 * name the entry already has, on its way to being refused, and the focus
 * belongs to the row the reader renamed.
 */
it('takes the focus for the name of a field just renamed with Return, and only when nobody has it', () => {
	const renamed = show({ field: visible('Card PIN', '4071'), renamed: true });
	expect(document.activeElement).toBe(find('Rename Card PIN'));
	unmount(renamed.component);

	const elsewhere = outside();
	elsewhere.focus();
	const busy = show({ field: visible('Card PIN', '4071'), renamed: true });
	expect(document.activeElement, 'the row took the focus from where the reader had put it').toBe(
		elsewhere
	);
	unmount(busy.component);

	elsewhere.blur();
	const standing = show({ field: visible('Card PIN', '4071') });
	expect(document.activeElement).toBe(document.body);
	standing.props.renamed = true;
	flushSync();
	expect(document.activeElement, 'a row drawn already took the focus').toBe(document.body);

	return unmount(standing.component);
});

/** The Return that confirms an input method's conversion and the Escape that
 * cancels one are the input method's. Read as the name's, they finished a
 * name half converted or threw away all of it. */
it('leaves the Return and the Escape of an input method to the input method', async () => {
	const onRename = vi.fn().mockResolvedValue(true);
	const { component, props } = show({ onRename });

	const input = rename('PIN');
	input.value = 'Karte';
	for (const composition of [{ isComposing: true }, { keyCode: 229 }]) {
		for (const pressed of ['Enter', 'Escape']) {
			key(input, pressed, composition);
			flushSync();
			expect(input.isConnected, JSON.stringify({ pressed, composition })).toBe(true);
			expect(input.value).toBe('Karte');
		}
	}
	await settled();
	expect(onRename).not.toHaveBeenCalled();

	key(input, 'Enter');
	await vi.waitFor(() => expect(onRename).toHaveBeenCalledWith(props.entry, 'Karte', true));

	return unmount(component);
});

/**
 * A value written under the old name has to land under the old name. A
 * rename that went first moved the field away from under the write, which
 * then made a field of the old name again with the new value in it.
 */
it('renames only once the value written under the old name has landed', async () => {
	const at = next();
	const landing = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.setField.mockReturnValue(landing.promise);
	const onRename = vi.fn().mockResolvedValue(true);
	const { component } = show({
		entry: at,
		field: visible('PIN', '4071'),
		onWrite: writes(at),
		onRename
	});

	enter(value('PIN'), '9314');
	value('PIN').dispatchEvent(new FocusEvent('blur'));
	const input = rename('PIN');
	input.value = 'Card PIN';
	key(input, 'Enter');
	await settled();
	expect(onRename, 'the rename went ahead of the value').not.toHaveBeenCalled();

	landing.resolve(entry());
	await vi.waitFor(() => expect(onRename).toHaveBeenCalledWith(at, 'Card PIN', true));
	expect(ipc.setField.mock.invocationCallOrder[0]).toBeLessThan(
		onRename.mock.invocationCallOrder[0]
	);

	return unmount(component);
});

/**
 * The name was finished on the entry on the screen. While the value written
 * under the old name lands, the reader may open another entry with a field of
 * the same name; the rename is still about the field it was typed over.
 */
it('renames the field on the entry the name was finished on, whatever entry is up when it goes', async () => {
	const at = next();
	const landing = Promise.withResolvers<ReturnType<typeof entry>>();
	ipc.setField.mockReturnValue(landing.promise);
	const onRename = vi.fn().mockResolvedValue(true);
	const { component, props } = show({
		entry: at,
		field: visible('PIN', '4071'),
		onWrite: writes(at),
		onRename
	});

	enter(value('PIN'), '9314');
	value('PIN').dispatchEvent(new FocusEvent('blur'));
	const input = rename('PIN');
	input.value = 'Card PIN';
	key(input, 'Enter');
	await settled();
	expect(onRename).not.toHaveBeenCalled();

	const later = next();
	hand(props, { entry: later });
	landing.resolve(entry());
	await vi.waitFor(() => expect(onRename).toHaveBeenCalledTimes(1));
	expect(onRename, 'the rename went to the entry opened since').toHaveBeenCalledWith(
		at,
		'Card PIN',
		true
	);

	return unmount(component);
});

/** Another field written a moment earlier lands while the name is being
 * written, and the row is handed the field again. What the reader is typing
 * stays where it is. */
it('keeps a name being written when the field comes back from Rust', () => {
	const { component, props } = show({ field: visible('PIN', '4071') });

	const input = rename('PIN');
	input.value = 'Card P';
	hand(props, { field: visible('PIN', '9314') });

	expect(input.isConnected).toBe(true);
	expect(input.value).toBe('Card P');

	key(input, 'Escape');
	return unmount(component);
});

/**
 * A name nothing may change is a name and not a press, and so is the name of
 * a field a new value is being written for: that value is on its way to this
 * name. Either way the whole name is still there for a pointer to read.
 */
it('offers no rename where nothing may change, or while a new value is on its way', () => {
	const long = `Security answer ${'y'.repeat(120)}`;

	const reading = show({ field: visible(long, '4071'), readOnly: true });
	expect(labelled(`Rename ${long}`)).toBeNull();
	expect(title(long).textContent?.trim()).toBe(long);
	unmount(reading.component);

	const { component } = show({ field: masked(long) });
	find(`Change ${long}`).click();
	flushSync();
	expect(labelled(`Rename ${long}`)).toBeNull();
	expect(title(long).tagName).toBe('SPAN');
	title(long).click();
	flushSync();
	expect(labelled(`New name for ${long}`)).toBeNull();

	return unmount(component);
});

/**
 * The trash asks Rust to take the field off. Whether that can be taken back
 * is Rust's to answer, so the first press never says it is for good.
 */
it('asks for the field to be taken off, and never for good at the first press', () => {
	const { component, props } = show();

	find('Remove the field PIN').click();
	flushSync();

	expect(props.onRemove).toHaveBeenCalledTimes(1);
	expect(props.onRemove).toHaveBeenCalledWith(false);
	expect(host.querySelector('[data-confirm]')).toBeNull();

	return unmount(component);
});

/**
 * Asked whether a removal nothing could bring back is wanted, the row takes
 * the field off only on the red answer. The focus lands on the way out, and
 * Escape is the way out without reaching the window, which reads it as
 * "close the entry".
 */
it('removes for good only on the red answer, and keeps the field on the way out', () => {
	const closing = vi.fn();
	window.addEventListener('keydown', closing);
	try {
		const { component, props } = show({ removing: true });

		expect(labelled('Remove the field PIN'), 'the trash stood beside its own question').toBeNull();
		expect(host.querySelector('[data-confirm]')?.textContent).toContain(
			'Remove “\u2068PIN\u2069”? This vault keeps no version to bring it back from, so this can’t be undone.'
		);
		expect(document.activeElement).toBe(button('Keep it'));

		key(button('Keep it'), 'Escape');
		expect(props.onKeep).toHaveBeenCalledTimes(1);
		expect(closing, 'the Escape reached the window').not.toHaveBeenCalled();

		button('Keep it').click();
		expect(props.onKeep).toHaveBeenCalledTimes(2);
		expect(props.onRemove).not.toHaveBeenCalled();

		button('Remove').click();
		expect(props.onRemove).toHaveBeenCalledTimes(1);
		expect(props.onRemove).toHaveBeenCalledWith(true);

		return unmount(component);
	} finally {
		window.removeEventListener('keydown', closing);
	}
});

/** A row nothing may change offers nothing that changes it: no trash, no
 * lock, no rename, no Change and no Make one. */
it('offers nothing that changes the field where nothing may change', () => {
	for (const shown of [visible('PIN', '4071'), masked('PIN'), blank('PIN')]) {
		const { component } = show({ field: shown, readOnly: true });
		const said = JSON.stringify(shown);

		for (const label of [
			'Remove the field PIN',
			'Keep PIN hidden',
			'Rename PIN',
			'Change PIN',
			'Make one for PIN'
		]) {
			expect(labelled(label), `${said}: ${label}`).toBeNull();
		}
		expect(host.querySelector('input, textarea'), said).toBeNull();

		unmount(component);
	}
});

/**
 * A field just named with Return is about to be given a value, so the field
 * the value is typed in has the focus as it is drawn - the empty hidden one a
 * new field is by default as much as one in the open. Only then: a row drawn
 * again later does not take the focus from wherever the reader has put it.
 */
it('takes the focus for the value of a field just named, and only then', () => {
	for (const shown of [blank('PIN'), visible('PIN', '')]) {
		const { component } = show({ field: shown, fresh: true });
		expect(document.activeElement, JSON.stringify(shown)).toBe(value('PIN'));
		unmount(component);
	}

	const elsewhere = outside();
	elsewhere.focus();
	const { component, props } = show({ field: blank('PIN'), fresh: false });
	expect(document.activeElement).toBe(elsewhere);

	props.fresh = true;
	flushSync();
	expect(document.activeElement, 'the row took the focus when it was drawn again').toBe(elsewhere);

	return unmount(component);
});

/**
 * A name and a value come from the vault, and so from whoever wrote the file.
 * Markup in either is text. A name may stand in the attributes that say what
 * a press is about to a screen reader and to a pointer; a value stands in
 * none.
 */
it('writes markup in a name and a value as text, and a value in no attribute', () => {
	const name = '<img src=x onerror="alert(1)">\u202eevil';
	const held = '<script>alert(1)</script><b>bold</b>';
	const { component } = show({ field: visible(name, held), removing: true });

	expect(host.querySelector('script, img, b')).toBeNull();
	expect(host.textContent).toContain(name);
	expect(written()).toContain(held);
	for (const attribute of attributes()) {
		expect(attribute.value, attribute.name).not.toContain(held);
		if (attribute.value.includes(name)) {
			expect(['title', 'aria-label'], attribute.value).toContain(attribute.name);
		}
	}
	// The override in the name ends inside the quotes, so it does not turn the
	// warning after it around.
	expect(host.querySelector('[data-confirm]')?.textContent).toContain(`“\u2068${name}\u2069”?`);

	return unmount(component);
});

/**
 * A hidden value is on the screen in the one node that shows it and nowhere
 * else: in no attribute, in no field - the Change field a new value is typed
 * in included - and not before anybody asked for it.
 */
it('keeps a hidden value out of every attribute and every field', async () => {
	const { component } = show({ field: masked('<b>PIN</b>') });
	expect(ipc.reveal).not.toHaveBeenCalled();
	expect(host.textContent).not.toContain(SECRET);

	find('Show <b>PIN</b>').click();
	await vi.waitFor(() => expect(host.textContent).toContain(SECRET));
	flushSync();
	expect(host.querySelector('b')).toBeNull();
	expect(attributes().some((attribute) => attribute.value.includes(SECRET))).toBe(false);
	expect(written()).not.toContain(SECRET);

	find('Change <b>PIN</b>').click();
	flushSync();
	expect(written()).not.toContain(SECRET);
	expect(attributes().some((attribute) => attribute.value.includes(SECRET))).toBe(false);

	await unmount(component);
	expect(document.body.textContent).not.toContain(SECRET);
});
