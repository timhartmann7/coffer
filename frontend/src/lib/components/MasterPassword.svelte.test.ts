import { flushSync, mount, tick, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { plain } from '$lib/context.svelte';
import { held } from '$lib/holding';
import type { Rekey } from '$lib/saving.svelte';
import type { Stubbed } from '$lib/stubbed';
import MasterPassword from './MasterPassword.svelte';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('$lib/ipc'), async (real) =>
	Object.assign(ipc, (await import('$lib/stubbed')).stubbed(await real()))
);

const OLD = 'the one it had';
const NEW = 'correct horse battery staple';

let host: HTMLElement;
let drawn: ReturnType<typeof mount>[] = [];

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

// Unmounted, so that a change still on its way when a test ends lets go of
// the settings rather than holding them for every test after it; and
// everything in the body cleared, not just the host, because a field a test
// put the focus in would otherwise hold it for the next test, which reads
// where it is.
afterEach(() => {
	for (const row of drawn) void unmount(row);
	drawn = [];
	document.body.replaceChildren();
});

/** Draws the row, with Rust's answer to a change. */
function show(rekey = vi.fn<Rekey>().mockResolvedValue(0)) {
	drawn.push(mount(MasterPassword, { target: host, props: { rekey } }));
	flushSync();
	return rekey;
}

function button(label: string): HTMLButtonElement {
	const found = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === label
	);
	if (!found) throw new Error(`no button labelled ${label}`);
	return found;
}

function field(label: string): HTMLInputElement {
	const named = [...host.querySelectorAll('label')].find(
		(candidate) => candidate.textContent?.trim() === label
	);
	const found = named && document.getElementById(named.htmlFor);
	if (!(found instanceof HTMLInputElement)) throw new Error(`no field labelled ${label}`);
	return found;
}

const current = () => field('Current password');
const fresh = () => field('New password');
const again = () => field('Once more');

function type(input: HTMLInputElement, value: string) {
	input.value = value;
	input.dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
}

function begin() {
	button('Change…').click();
	flushSync();
}

function fill(had = OLD, wanted = NEW, twice = wanted) {
	begin();
	type(current(), had);
	type(fresh(), wanted);
	type(again(), twice);
}

function submit() {
	host.querySelector('form')?.dispatchEvent(new Event('submit', { cancelable: true }));
	flushSync();
}

function key(at: Element, name: string, extra: KeyboardEventInit = {}): KeyboardEvent {
	const event = new KeyboardEvent('keydown', {
		key: name,
		bubbles: true,
		cancelable: true,
		...extra
	});
	at.dispatchEvent(event);
	flushSync();
	return event;
}

function rightClick(at: Element): MouseEvent {
	const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
	at.dispatchEvent(event);
	return event;
}

/** The focus leaving `from` for `to`, which is what masks a password again. */
function leave(from: HTMLElement, to: Element | null) {
	from.dispatchEvent(new FocusEvent('focusout', { relatedTarget: to, bubbles: true }));
	flushSync();
}

/** The live regions in the row now. A sentence is read out when it arrives in
 * one of these, and may not be when it comes with a region of its own. */
const regions = () => [...host.querySelectorAll('[role="status"]')];

/** The live region the line saying `text` is in, if any. */
function regionOf(text: string): Element | null {
	const line = [...host.querySelectorAll('p')].find((each) => each.textContent?.includes(text));
	return line?.closest('[role="status"]') ?? null;
}

const encode = (text: string) => [...new TextEncoder().encode(text)];

it('offers nothing to type until Change is pressed', () => {
	show();

	expect(host.textContent).toContain('Master password');
	expect(host.querySelectorAll('input')).toHaveLength(0);
	expect(host.textContent).not.toContain('cannot be recovered');
});

/** With a key file beside the password, or a key file alone, "the one password
 * that opens this vault" would be untrue - and the reader whose key file
 * alone opens it has to know the current password is nothing at all. */
it('says what the current password is, whatever opens the vault', () => {
	show();

	expect(host.textContent).toContain(
		'What you type to unlock this vault - nothing, if its key file alone opens it'
	);
	expect(host.textContent).not.toContain('The one password that opens this vault');
});

it('opens with the cursor in the current password, under the warning the creation screen gives', () => {
	show();
	begin();

	expect(document.activeElement).toBe(current());
	expect(host.querySelectorAll('input')).toHaveLength(3);
	expect(host.textContent).toContain('The password cannot be recovered.');
	expect(host.textContent).toContain('nobody can get the data back');
	for (const input of host.querySelectorAll('input')) {
		expect(input.type, 'a password is shown before anybody asked').toBe('password');
	}
	expect(current().autocomplete).toBe('off');
	expect(fresh().autocomplete).toBe('new-password');
});

it('sends the passwords as bytes, the current one first', async () => {
	const sent: number[][] = [];
	const rekey = show(
		vi.fn<Rekey>(async (had, wanted) => {
			sent.push([...had], [...wanted]);
			return 0;
		})
	);
	fill('é au lait', 'x\u0000y');
	submit();

	await vi.waitFor(() => expect(rekey).toHaveBeenCalledTimes(1));
	expect(sent).toEqual([encode('é au lait'), encode('x\u0000y')]);
});

/** A vault its key file alone opens holds no password, and nothing is what is
 * typed as the current one. A check here that the field is not empty would
 * leave those readers no way to give their vault a password. */
it('sends an empty current password', async () => {
	const sent: number[][] = [];
	const rekey = show(
		vi.fn<Rekey>(async (had, wanted) => {
			sent.push([...had], [...wanted]);
			return 0;
		})
	);
	fill('', NEW);
	submit();

	await vi.waitFor(() => expect(rekey).toHaveBeenCalledTimes(1));
	expect(sent).toEqual([[], encode(NEW)]);
});

it('sends one change however often the form is sent', async () => {
	const rekey = show();
	fill();

	submit();
	submit();
	button('Changing it…').click();
	flushSync();

	expect(rekey).toHaveBeenCalledTimes(1);
	await vi.waitFor(() => expect(host.textContent).toContain('Master password changed.'));
	expect(rekey).toHaveBeenCalledTimes(1);
});

/** The buffers are framed and wiped on the way to Rust; these are wiped here
 * as well, for a call that never got as far as framing them. */
it('wipes what it sent once Rust has answered', async () => {
	let handed: Uint8Array[] = [];
	show(
		vi.fn<Rekey>(async (had, wanted) => {
			handed = [had, wanted];
			throw { code: 'io', message: 'the disk is full' };
		})
	);
	fill();
	submit();

	await vi.waitFor(() => expect(host.textContent).toContain('the disk is full'));
	for (const buffer of handed) {
		expect(buffer.every((byte) => byte === 0)).toBe(true);
	}
});

it('leaves no typed password in the markup, shown or hidden', () => {
	show();
	fill();

	expect(host.innerHTML).not.toContain(OLD);
	expect(host.innerHTML).not.toContain(NEW);

	for (const eye of host.querySelectorAll<HTMLButtonElement>(
		'button[aria-label="Show the password"]'
	)) {
		eye.click();
	}
	flushSync();
	expect(current().type).toBe('text');
	expect(host.innerHTML).not.toContain(OLD);
	expect(host.innerHTML).not.toContain(NEW);
});

it('says the two are not the same, empties only the second, and sends nothing', async () => {
	const rekey = show();
	fill(OLD, NEW, `${NEW}!`);
	const before = regions();
	submit();

	expect(host.textContent).toContain('Those two are not the same.');
	expect(before, 'the sentence came with a region of its own').toContain(
		regionOf('Those two are not the same.')
	);
	expect(rekey).not.toHaveBeenCalled();
	expect(again().value).toBe('');
	expect(fresh().value).toBe(NEW);
	expect(current().value).toBe(OLD);
	await vi.waitFor(() => expect(document.activeElement).toBe(again()));
	const described = document.getElementById(again().getAttribute('aria-describedby') ?? '');
	expect(described?.textContent, 'the field is not read out with the sentence').toBe(
		'Those two are not the same.'
	);

	// Typing again takes the sentence back.
	type(again(), 'n');
	expect(host.textContent).not.toContain('Those two are not the same.');
	expect(again().hasAttribute('aria-describedby')).toBe(false);
});

/** An accent typed as one character and as a letter with a combining mark look
 * the same and are two different passwords, and the vault would open with
 * only one of them. */
it('compares the two new passwords as bytes, not as text that looks the same', () => {
	const rekey = show();
	fill(OLD, 'caf\u00e9', 'cafe\u0301');
	submit();

	expect(host.textContent).toContain('Those two are not the same.');
	expect(rekey).not.toHaveBeenCalled();
});

it("refuses an empty new password with the creation screen's sentence and sends nothing", () => {
	const rekey = show();
	fill(OLD, '', '');
	submit();

	expect(host.textContent).toContain('A vault needs a master password.');
	expect(rekey).not.toHaveBeenCalled();
	expect(document.activeElement).toBe(fresh());
});

it('marks a wrong current password, keeps everything typed, and puts the cursor back in it', async () => {
	show(
		vi.fn<Rekey>().mockRejectedValue({
			code: 'wrongCredentials',
			message: "that is not the vault's current password"
		})
	);
	fill('typed wrong');
	const before = regions();
	submit();

	const sentence = "The password was not changed: that is not the vault's current password.";
	await vi.waitFor(() => expect(host.textContent).toContain(sentence));
	expect(before, 'the refusal came with a region of its own').toContain(regionOf(sentence));
	expect(current().getAttribute('aria-invalid')).toBe('true');
	expect(current().parentElement?.className).toContain('border-danger/60');
	expect(fresh().getAttribute('aria-invalid')).toBe('false');
	expect(document.activeElement).toBe(current());
	expect([current().value, fresh().value, again().value]).toEqual(['typed wrong', NEW, NEW]);

	// A key in the field takes the mark back.
	type(current(), OLD);
	expect(current().getAttribute('aria-invalid')).toBe('false');
});

/** A refusal is about the attempt it answered. Left on screen beside the
 * sentence the next attempt brings, it would send the reader to a field that
 * is right now - here the current password they have just corrected. */
it('says only what the latest attempt came to', async () => {
	show(
		vi.fn<Rekey>().mockRejectedValue({
			code: 'wrongCredentials',
			message: "that is not the vault's current password"
		})
	);
	fill('typed wrong');
	submit();
	await vi.waitFor(() =>
		expect(host.textContent).toContain("that is not the vault's current password")
	);

	type(current(), OLD);
	type(again(), `${NEW}!`);
	submit();
	expect(host.textContent).toContain('Those two are not the same.');
	expect(host.textContent, 'the last refusal is still said beside the mismatch').not.toContain(
		'The password was not changed'
	);

	// The empty password's sentence goes the same way.
	type(fresh(), '');
	type(again(), '');
	submit();
	expect(host.textContent).toContain('A vault needs a master password.');
	type(fresh(), NEW);
	type(again(), `${NEW}?`);
	submit();
	expect(host.textContent).toContain('Those two are not the same.');
	expect(host.textContent).not.toContain('A vault needs a master password.');
});

/** The copy a lock left opens with the password the vault has now. A change
 * beside it would leave the two opening with different ones, and making the
 * copy the vault afterwards would bring the old password back; so the row
 * says what to do with the copy first, and offers nothing to type. */
it('says what to do with a lock’s copy first, and offers no change beside it', () => {
	drawn.push(
		mount(MasterPassword, { target: host, props: { rekey: vi.fn<Rekey>(), copyBeside: true } })
	);
	flushSync();

	expect(host.textContent).toContain('Master password');
	expect(host.textContent).toContain(
		'Not while the copy a lock left sits beside this vault, which opens with the same password.'
	);
	expect(host.textContent?.replace(/\s+/g, ' ')).toContain(
		'Lock the vault, then make the copy your vault or remove it'
	);
	expect(host.querySelectorAll('button')).toHaveLength(0);
	expect(host.querySelectorAll('input')).toHaveLength(0);
});

it('puts the cursor in the new password when Rust refuses it', async () => {
	show(
		vi.fn<Rekey>().mockRejectedValue({
			code: 'refused',
			message: 'the new password is the one the vault already has'
		})
	);
	fill(OLD, OLD);
	submit();

	await vi.waitFor(() =>
		expect(host.textContent).toContain(
			'The password was not changed: the new password is the one the vault already has.'
		)
	);
	expect(document.activeElement).toBe(fresh());
	expect(current().getAttribute('aria-invalid')).toBe('false');
});

it('says the password was not changed when the file would not take it', async () => {
	for (const [refused, said] of [
		[{ code: 'io', message: 'the disk is full' }, 'the disk is full'],
		[
			{ code: 'externalChange', message: 'the database changed on disk after Coffer opened it' },
			'the database changed on disk after Coffer opened it'
		]
	] as const) {
		show(vi.fn<Rekey>().mockRejectedValue(refused));
		fill();
		submit();

		await vi.waitFor(() =>
			expect(host.textContent).toContain(`The password was not changed: ${said}.`)
		);
		expect(host.textContent).not.toContain('Master password changed');
		expect(host.querySelector('form'), 'the form went with what was typed').not.toBeNull();
		const row = drawn.pop();
		if (row) await unmount(row);
	}
});

it('puts the form away on Escape and leaves the settings open', async () => {
	show();
	fill();

	const escape = key(fresh(), 'Escape');

	expect(escape.defaultPrevented, 'the settings were left to close behind the form').toBe(true);
	expect(host.querySelector('form')).toBeNull();
	await tick();
	expect(document.activeElement).toBe(button('Change…'));
});

it('leaves an Escape that ends a composition to the input method', () => {
	show();
	fill();

	for (const composition of [{ isComposing: true }, { keyCode: 229 }]) {
		const escape = key(fresh(), 'Escape', composition);
		expect(escape.defaultPrevented).toBe(false);
		expect(host.querySelector('form'), 'a composition took the form away').not.toBeNull();
	}
});

it('cannot be put away, nor let the settings go, while the password is being changed', async () => {
	let answer: (left: number) => void = () => {};
	show(vi.fn<Rekey>(() => new Promise((settle) => (answer = settle))));
	fill();
	submit();

	expect(button('Changing it…').disabled).toBe(true);
	expect(button('Never mind').disabled).toBe(true);
	expect(current().readOnly).toBe(true);
	expect(held(), 'the settings could be closed under the change').toBe(true);
	expect(key(current(), 'Escape').defaultPrevented).toBe(true);
	expect(host.querySelector('form'), 'Escape put the form away mid-change').not.toBeNull();

	answer(0);
	await vi.waitFor(() => expect(host.textContent).toContain('Master password changed.'));
	expect(held()).toBe(false);
});

/** A right-click in any of the three fields keeps WebKit's menu under the
 * window's own listener, shown or masked, because its Cut, Copy, Paste and
 * spelling are about the typing. The eye is not a field and draws none, and
 * nor does a field gone read only while the change is on its way: nothing in
 * it is the reader's to edit then. Coffer's own menu is never asked for. */
it('leaves a right-click in a field to WebKit, and draws none on the eye or mid-change', async () => {
	let answer: (left: number) => void = () => {};
	show(vi.fn<Rekey>(() => new Promise((settle) => (answer = settle))));
	host.addEventListener('contextmenu', plain);
	fill();
	const eye = fresh().parentElement?.querySelector('button');
	if (!eye) throw new Error('the field has no eye');
	eye.click();
	flushSync();
	expect(fresh().type).toBe('text');

	const fields = { current, fresh, again };
	for (const [name, each] of Object.entries(fields)) {
		expect(rightClick(each()).defaultPrevented, `${name} lost its menu`).toBe(false);
	}
	expect(rightClick(eye).defaultPrevented, 'the eye drew WebKit’s menu').toBe(true);

	submit();
	expect(current().readOnly).toBe(true);
	for (const [name, each] of Object.entries(fields)) {
		expect(rightClick(each()).defaultPrevented, `${name} offered a menu mid-change`).toBe(true);
	}
	expect(ipc.contextMenu).not.toHaveBeenCalled();

	answer(0);
	await vi.waitFor(() => expect(host.textContent).toContain('Master password changed.'));
});

it('moves on with Return rather than sending from the first two fields', () => {
	const rekey = show();
	fill();
	current().focus();

	expect(key(current(), 'Enter').defaultPrevented).toBe(true);
	expect(document.activeElement).toBe(fresh());
	expect(key(fresh(), 'Enter').defaultPrevented).toBe(true);
	expect(document.activeElement).toBe(again());
	expect(rekey).not.toHaveBeenCalled();

	// The Return that confirms a conversion is the input method's.
	current().focus();
	key(current(), 'Enter', { keyCode: 229 });
	expect(document.activeElement).toBe(current());
});

/** "Once more" has no Return of its own to move on with, so the field's guard
 * is all that stops a word half composed in it from being sent. */
it('sends nothing on the Return an input method takes in the last field', () => {
	const rekey = show();
	fill();
	again().focus();

	for (const composition of [{ isComposing: true }, { keyCode: 229 }]) {
		expect(key(again(), 'Enter', composition).defaultPrevented).toBe(true);
	}
	expect(rekey).not.toHaveBeenCalled();
	expect(host.querySelector('form')).not.toBeNull();
});

it('shows a password only while its field has the focus', () => {
	show();
	fill();
	const eye = fresh().parentElement?.querySelector('button');
	if (!eye) throw new Error('the field has no eye');

	eye.click();
	flushSync();
	expect(fresh().type).toBe('text');
	expect(eye.getAttribute('aria-label')).toBe('Hide the password');
	expect(eye.hasAttribute('aria-pressed'), 'the eye says two things at once').toBe(false);

	// Onto its own eye, it stays shown.
	leave(fresh(), eye);
	expect(fresh().type).toBe('text');

	// Anywhere else, it is a mask again.
	leave(fresh(), current());
	expect(fresh().type).toBe('password');
	expect(current().type, 'one eye showed another field').toBe('password');
	expect(eye.getAttribute('aria-label')).toBe('Show the password');
});

it('draws the check only when the two new passwords agree', () => {
	show();
	fill(OLD, NEW, 'not yet');
	const check = () => host.querySelector('use[href="#i-check"]');

	leave(again(), current());
	expect(check()).toBeNull();

	type(again(), NEW);
	leave(again(), current());
	expect(check()).not.toBeNull();
	expect(host.textContent).toContain('The two are the same');

	type(fresh(), `${NEW}!`);
	expect(check(), 'a key in the first left the check standing').toBeNull();
});

it('says the password changed and asks nothing when no backup was there to ask about', async () => {
	show(vi.fn<Rekey>().mockResolvedValue(0));
	fill();
	const before = regions();
	submit();

	await vi.waitFor(() => expect(host.textContent).toContain('Master password changed.'));
	expect(before, 'what the change came to is said where nothing reads it out').toContain(
		regionOf('Master password changed.')
	);
	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(host.textContent).not.toContain('automatic backup');
	await tick();
	expect(document.activeElement).toBe(button('Change…'));
});

/** The question is the old backups' own (OldBackups has its tests); the row
 * hands over to it, and takes the focus back once it is answered. */
it('asks about the old backups after a change that left some, and comes back when they are kept', async () => {
	show(vi.fn<Rekey>().mockResolvedValue(4));
	fill();
	submit();

	await vi.waitFor(() =>
		expect(host.textContent).toContain(
			'Master password changed. Your 4 automatic backups of earlier saves still open with the old password'
		)
	);
	expect(host.querySelector('form')).toBeNull();
	expect(document.activeElement).toBe(button('Keep them'));

	button('Keep them').click();
	flushSync();
	expect(host.querySelector('[data-confirm]')).toBeNull();
	expect(ipc.removeOldSnapshots).not.toHaveBeenCalled();
	await tick();
	expect(document.activeElement).toBe(button('Change…'));
});

it('says in the row how many old backups went, and gives the focus back', async () => {
	ipc.removeOldSnapshots.mockResolvedValue({ gone: 4, left: 0, refused: null });
	show(vi.fn<Rekey>().mockResolvedValue(4));
	fill();
	submit();
	await vi.waitFor(() => expect(host.querySelector('[data-confirm]')).not.toBeNull());
	const before = regions();

	button('Remove old backups').click();

	await vi.waitFor(() => expect(host.textContent).toContain('Removed 4 old backups.'));
	expect(before, 'how many went is said where nothing reads it out').toContain(
		regionOf('Removed 4 old backups.')
	);
	expect(host.querySelector('[data-confirm]')).toBeNull();
	await tick();
	expect(document.activeElement).toBe(button('Change…'));
});

/** Keep is the reader declining the removal, and a sentence about a removal
 * that failed has nothing left to be about. */
it('takes a failed removal’s sentence away with the question when the backups are kept', async () => {
	ipc.removeOldSnapshots.mockRejectedValue({ code: 'noVault', message: 'no database is open' });
	show(vi.fn<Rekey>().mockResolvedValue(2));
	fill();
	submit();
	await vi.waitFor(() => expect(host.querySelector('[data-confirm]')).not.toBeNull());

	button('Remove old backups').click();
	await vi.waitFor(() =>
		expect(host.textContent).toContain('The old backups could not be removed')
	);
	button('Keep them').click();
	flushSync();

	expect(host.textContent).not.toContain('could not be removed');
	expect(host.querySelector('[data-confirm]')).toBeNull();
});

it('opens empty again after a change', async () => {
	show(vi.fn<Rekey>().mockResolvedValue(0));
	fill();
	submit();
	await vi.waitFor(() => expect(host.textContent).toContain('Master password changed.'));

	begin();
	expect([current().value, fresh().value, again().value]).toEqual(['', '', '']);
	expect(host.textContent, 'what the last change said is still there').not.toContain(
		'Master password changed.'
	);
});

it('puts the form away with Never mind and sends nothing', async () => {
	const rekey = show();
	fill();

	button('Never mind').click();
	flushSync();

	expect(host.querySelector('form')).toBeNull();
	expect(rekey).not.toHaveBeenCalled();
	await tick();
	expect(document.activeElement).toBe(button('Change…'));
	begin();
	expect(current().value).toBe('');
});
