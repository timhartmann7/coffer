import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import Confirm from './Confirm.svelte';

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => host.remove());

function ask(props: Record<string, unknown> = {}) {
	const onKeep = vi.fn();
	const onAct = vi.fn();
	const component = mount(Confirm, {
		target: host,
		props: {
			question: 'Remove passport.pdf (1.2 MB)?',
			act: 'Remove',
			onKeep,
			onAct,
			...props
		}
	});
	flushSync();
	return { component, onKeep, onAct };
}

function buttons(): HTMLButtonElement[] {
	return [...host.querySelectorAll('button')];
}

function button(label: string): HTMLButtonElement {
	const found = buttons().find((candidate) => candidate.textContent?.trim() === label);
	if (!found) throw new Error(`no button labelled ${label}`);
	return found;
}

/** The two answers do exactly what they say and nothing else: the way out
 * never destroys, and the destructive answer never merely closes. */
it('does what each answer says and nothing else', () => {
	const { component, onKeep, onAct } = ask();

	expect(host.textContent).toContain('Remove passport.pdf (1.2 MB)?');

	button('Keep it').click();
	expect(onKeep).toHaveBeenCalledTimes(1);
	expect(onAct).not.toHaveBeenCalled();

	button('Remove').click();
	expect(onAct).toHaveBeenCalledTimes(1);
	expect(onKeep).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/** The mockup's rule for a choice: named by its result, the destructive one
 * last and the only one in the danger colour. A neutral step between them is
 * neither the way out nor the way to lose something. */
it('puts the destructive answer last, and makes it the only red one', () => {
	const run = vi.fn();
	const { component, onKeep, onAct } = ask({
		neutral: { label: 'Save a copy first…', run }
	});

	const named = buttons().map((each) => each.textContent?.trim());
	expect(named).toEqual(['Keep it', 'Save a copy first…', 'Remove']);

	const red = buttons().filter((each) => each.className.includes('text-danger'));
	expect(red).toEqual([button('Remove')]);

	button('Save a copy first…').click();
	expect(run).toHaveBeenCalledTimes(1);
	expect(onKeep).not.toHaveBeenCalled();
	expect(onAct).not.toHaveBeenCalled();

	return unmount(component);
});

/**
 * The press that asked the question is still under the reader's finger, and
 * the key that pressed it is still under their hand. The focus lands on the way
 * out, so a second Return or Space keeps the thing rather than destroying it.
 */
it('puts the focus on the way out', () => {
	const { component } = ask({ neutral: { label: 'Save a copy first…', run: vi.fn() } });

	expect(document.activeElement).toBe(button('Keep it'));

	return unmount(component);
});

/**
 * A neutral answer can be the one a reader means most often - keeping both of
 * two files that share a name - and then the focus is there, so Return gives
 * it. Never the destructive one, and Escape is still the way out.
 */
it('puts the focus on the neutral answer when asked to, and never on the destructive one', () => {
	const run = vi.fn();
	const { component, onKeep, onAct } = ask({
		keep: 'Don’t add it',
		neutral: { label: 'Keep both', run },
		focus: 'neutral',
		act: 'Replace'
	});

	expect(document.activeElement).toBe(button('Keep both'));

	button('Keep both').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
	expect(onKeep).toHaveBeenCalledTimes(1);
	expect(run).not.toHaveBeenCalled();
	expect(onAct).not.toHaveBeenCalled();

	return unmount(component);
});

/** Asked again after its destructive answer was refused, the question keeps
 * the answers that are still open and nothing in the danger colour. */
it('asks again without the destructive answer when there is none to give', () => {
	const { component } = ask({
		act: undefined,
		neutral: { label: 'Keep both', run: vi.fn() },
		focus: 'neutral'
	});

	expect(buttons().map((each) => each.textContent?.trim())).toEqual(['Keep it', 'Keep both']);
	expect(buttons().filter((each) => each.className.includes('text-danger'))).toEqual([]);
	expect(document.activeElement).toBe(button('Keep both'));

	return unmount(component);
});

/**
 * Escape is the way out here too, and it stops here. The window reads the same
 * key as "put the entry away", and a question answered by the pane vanishing
 * from under it is one the reader never saw answered.
 */
it('keeps the thing on Escape without letting the key reach the window', () => {
	const elsewhere = vi.fn();
	window.addEventListener('keydown', elsewhere);
	try {
		const { component, onKeep, onAct } = ask();

		button('Remove').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
		expect(onKeep).toHaveBeenCalledTimes(1);
		expect(onAct).not.toHaveBeenCalled();
		expect(elsewhere, 'the Escape reached the window as well').not.toHaveBeenCalled();

		return unmount(component);
	} finally {
		window.removeEventListener('keydown', elsewhere);
	}
});

/** A question names what goes, and what goes is a value out of somebody's
 * database. It is written as text, whatever it holds. */
it('writes a question that is markup as text', () => {
	const { component } = ask({ question: 'Remove <img src=x onerror=alert(1)>?' });

	expect(host.querySelector('img')).toBeNull();
	expect(host.textContent).toContain('<img src=x onerror=alert(1)>');

	return unmount(component);
});

/** The group says what it is about, for anything that reads the screen aloud. */
it('is labelled by its own question', () => {
	const { component } = ask();

	const group = host.querySelector('[role="group"]');
	const label = group?.getAttribute('aria-labelledby');
	expect(label).toBeTruthy();
	expect(host.querySelector(`[id="${label}"]`)?.textContent).toBe('Remove passport.pdf (1.2 MB)?');

	return unmount(component);
});
