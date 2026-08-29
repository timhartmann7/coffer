import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import Generator from './Generator.svelte';

const ipc = vi.hoisted(() => ({ generatePassword: vi.fn() }));
vi.mock('$lib/ipc', () => ipc);

const MADE = 't4Yv-8Qmz-Ld6R-Wn2H-Pk9C';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.generatePassword.mockResolvedValue(MADE);
});

afterEach(() => {
	host.remove();
	localStorage.clear();
});

function show(props: Record<string, unknown> = {}) {
	return mount(Generator, {
		target: host,
		props: { onInsert: vi.fn(), onClose: vi.fn(), onFailure: vi.fn(), ...props }
	});
}

function button(label: string): HTMLButtonElement {
	const found = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === label
	);
	if (!found) throw new Error(`no button labelled ${label}`);
	return found;
}

function value(): string {
	return host.querySelector('[data-value]')?.textContent ?? '';
}

it('draws a password from the vault and puts it nowhere else', async () => {
	const component = show();
	await vi.waitFor(() => expect(value()).toBe(MADE));
	flushSync();

	// The password is on the screen, and it is only on the screen.
	const attributes = [...host.querySelectorAll('*')].flatMap((element) =>
		[...element.attributes].map((attribute) => attribute.value)
	);
	expect(attributes.join(' ')).not.toContain(MADE);
	expect(JSON.stringify(localStorage)).not.toContain(MADE);
	expect(JSON.stringify(sessionStorage)).not.toContain(MADE);
	expect(window.location.href).not.toContain(MADE);

	return unmount(component);
});

it('asks for the length and the sets the reader chose', async () => {
	const component = show();
	await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalled());

	expect(ipc.generatePassword).toHaveBeenLastCalledWith(24, ['lower', 'upper', 'digits'], false);

	button('!@#$%').click();
	await vi.waitFor(() =>
		expect(ipc.generatePassword).toHaveBeenLastCalledWith(
			24,
			['lower', 'upper', 'digits', 'symbols'],
			false
		)
	);

	button('Look-alike characters').click();
	await vi.waitFor(() =>
		expect(ipc.generatePassword).toHaveBeenLastCalledWith(
			24,
			['lower', 'upper', 'digits', 'symbols'],
			true
		)
	);

	return unmount(component);
});

/** The reader can turn every set off. The engine refuses to make a password out
 * of nothing, and the message it sends back is what the window shows. */
it('reports a refusal rather than making something out of nothing', async () => {
	const onFailure = vi.fn();
	ipc.generatePassword.mockRejectedValue({
		code: 'refused',
		message: 'a password needs at least one kind of character'
	});

	const component = show({ onFailure });
	await vi.waitFor(() => expect(onFailure).toHaveBeenCalled());
	flushSync();

	expect(value()).toBe('');
	expect(button('Put it in the field').disabled).toBe(true);

	return unmount(component);
});

it('hands the password over once and takes it off the screen', async () => {
	const onInsert = vi.fn();
	const onClose = vi.fn();
	const component = show({ onInsert, onClose });
	await vi.waitFor(() => expect(value()).toBe(MADE));
	flushSync();

	button('Put it in the field').click();
	flushSync();

	expect(onInsert).toHaveBeenCalledWith(MADE);
	expect(onClose).toHaveBeenCalledTimes(1);
	expect(value()).toBe('');

	return unmount(component);
});

/** Closing the panel is one of the two ways a password stops being on the
 * screen, and the other one is the pane going. Neither may leave it behind. */
it('leaves nothing behind when the panel goes', async () => {
	const component = show();
	await vi.waitFor(() => expect(value()).toBe(MADE));

	await unmount(component);
	expect(document.body.textContent).not.toContain(MADE);
});
