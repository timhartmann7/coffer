import { flushSync, mount, unmount, type ComponentProps } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import Generator from './Generator.svelte';
import { drawing, generated } from '$lib/fixtures';
import type { Alphabet, Generated, Generator as Drawn, Purpose, Recipe } from '$lib/model';

const ipc = vi.hoisted(() => ({ generator: vi.fn(), generatePassword: vi.fn() }));
vi.mock('$lib/ipc', () => ipc);

const MADE = 't4Yv-8Qmz-Ld6R-Wn2H-Pk9C';

/** What Rust answers when there is nothing left to draw from. */
const REFUSAL = { code: 'refused', message: 'a password needs at least one kind of character' };

const LOOK_ALIKES = 'Avoid look-alikes (0 O 1 l I |)';

const OSRNG = ['Randomness comes from OsRng', 'There are no “memorable” passwords'];

const NO_DIGIT = ['No digit in this one.', 'Make another if the site asks for one.'];

/** Digits and nothing else, as Rust draws it: four digits, a slider that goes
 * down to them, and no look-alikes, because a PIN keeps its 0 and its 1. */
const PIN = drawing({
	recipe: recipe({ length: 4, alphabets: ['digits'] }),
	shortest: 4,
	pin: true,
	lookAlikes: ''
});

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
	ipc.generator.mockResolvedValue(drawing());
	ipc.generatePassword.mockResolvedValue(generated(MADE));
});

afterEach(() => {
	// Everything, not just the host: a test that fails with a second panel
	// open would leave its password on the page for the next test that reads
	// the whole document to find.
	document.body.replaceChildren();
	localStorage.clear();
});

function recipe(over: Partial<Recipe> = {}): Recipe {
	return { ...drawing().recipe, ...over };
}

/** The password's own generator unless a test says otherwise, which is the
 * one the entry pane opens under the password. */
function show(props: Partial<ComponentProps<typeof Generator>> = {}, target: HTMLElement = host) {
	return mount(Generator, {
		target,
		props: {
			purpose: 'password',
			label: 'Password generator',
			onInsert: vi.fn(),
			onClose: vi.fn(),
			onFailure: vi.fn(),
			...props
		}
	});
}

/** A second panel beside the first, the way a field's generator opens while
 * the password's is still out. */
function beside(): HTMLElement {
	const second = document.createElement('div');
	document.body.appendChild(second);
	return second;
}

function button(label: string, within: ParentNode = host): HTMLButtonElement {
	const found = [...within.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === label
	);
	if (!found) throw new Error(`no button labelled ${label}`);
	return found;
}

function labelled(label: string, within: ParentNode = host): HTMLButtonElement {
	const found = within.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);
	if (!found) throw new Error(`no button called ${label}`);
	return found;
}

function pressed(label: string): string | null {
	return button(label).getAttribute('aria-pressed');
}

/** The look-alike switch, whatever characters it names, or nothing. */
function lookAlikeSwitch(): HTMLButtonElement | undefined {
	return [...host.querySelectorAll('button')].find((candidate) =>
		candidate.textContent?.toLowerCase().includes('look-alike')
	);
}

function value(within: ParentNode = host): string {
	return within.querySelector('[data-value]')?.textContent ?? '';
}

function group(within: ParentNode = host): HTMLElement {
	const found = within.querySelector<HTMLElement>('[role="group"]');
	if (!found) throw new Error('the panel is not a group');
	return found;
}

/** The control a label names, found the way assistive technology finds it:
 * through the label's `for`, anywhere in the document. */
function named(label: HTMLLabelElement | null | undefined): HTMLInputElement {
	const found = label ? document.getElementById(label.htmlFor) : null;
	if (!(found instanceof HTMLInputElement)) throw new Error('the label names no input');
	return found;
}

function labels(within: ParentNode = host): HTMLLabelElement[] {
	return [...within.querySelectorAll('label')];
}

function avoidance(within: ParentNode = host): HTMLInputElement {
	return named(
		labels(within).find((label) => label.textContent?.trim() === 'Avoid these characters')
	);
}

function slider(within: ParentNode = host): HTMLInputElement {
	const found = within.querySelector<HTMLInputElement>('input[type="range"]');
	if (!found) throw new Error('there is no slider');
	return found;
}

/** What the slider's label calls it, and the number written beside it. */
function heading(): { label: string; length: string } {
	const label = labels().find((each) => each.htmlFor === slider().id);
	return {
		label: label?.textContent?.trim() ?? '',
		length: label?.nextElementSibling?.textContent?.trim() ?? ''
	};
}

/** The numbers written under the slider's two ends. */
function ends(): string[] {
	const under = slider().nextElementSibling;
	return [...(under?.children ?? [])].map((end) => end.textContent?.trim() ?? '');
}

/** Moves the slider the way a hand does: every step on the way, then the
 * release. */
function slide(to: number, release = true) {
	const range = slider();
	range.value = String(to);
	range.dispatchEvent(new Event('input', { bubbles: true }));
	if (release) range.dispatchEvent(new Event('change', { bubbles: true }));
}

function type(text: string) {
	const field = avoidance();
	field.value = text;
	field.dispatchEvent(new Event('input', { bubbles: true }));
}

/** The live region: what a screen reader says on its own when it changes. */
function status(): HTMLElement {
	const found = host.querySelector<HTMLElement>('[role="status"]');
	if (!found) throw new Error('there is no live region');
	return found;
}

/** The line beside "Put it in the field", as the eye reads it: the warning
 * when there is one, the footnote when there is not. */
function note(): HTMLElement {
	const found = button('Put it in the field').previousElementSibling;
	if (!(found instanceof HTMLElement)) throw new Error('there is nothing beside the button');
	return found;
}

/** Text one entry per line it is drawn on, wherever in it a `<br>` sits.
 * Blank lines are not lines. */
function lines(within: Node): string[] {
	const walk = document.createTreeWalker(within, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT);
	let text = '';
	while (walk.nextNode()) {
		const node = walk.currentNode;
		if (node.nodeName === 'BR') text += '\n';
		else if (node.nodeType === Node.TEXT_NODE) text += node.textContent ?? '';
	}
	return text
		.split('\n')
		.map((line) => line.replace(/\s+/g, ' ').trim())
		.filter((line) => line !== '');
}

/** What the live region holds, line by line. */
function said(): string[] {
	return lines(status());
}

/** What the line beside the button shows, line by line. */
function written(): string[] {
	return lines(note());
}

/** The text nodes in the live region that hold words, the ones a screen
 * reader reads out when they are put there. */
function spoken(): Node[] {
	const walk = document.createTreeWalker(status(), NodeFilter.SHOW_TEXT);
	const found: Node[] = [];
	while (walk.nextNode()) if (walk.currentNode.textContent?.trim()) found.push(walk.currentNode);
	return found;
}

function attributes(): string {
	return [...host.querySelectorAll('*')]
		.flatMap((element) => [...element.attributes].map((attribute) => attribute.value))
		.join(' ');
}

/** Lets every answer already given reach the panel. */
async function settle() {
	await new Promise((done) => setTimeout(done));
	flushSync();
}

/**
 * The password is on the screen, and it is only on the screen: not in an
 * attribute an accessibility tree or an inspector reads, not in storage, not
 * in the address. The line that says what it lacks says so in words about the
 * kinds, never by quoting it.
 */
it('draws a password from the vault and puts it nowhere else', async () => {
	ipc.generatePassword.mockResolvedValue(generated(MADE, { missing: ['digits'] }));
	const component = show();
	await vi.waitFor(() => expect(value()).toBe(MADE));
	flushSync();

	expect(said()[0]).toBe('No digit in this one.');
	expect(note().textContent).not.toContain(MADE);
	expect(attributes()).not.toContain(MADE);
	expect(JSON.stringify(localStorage)).not.toContain(MADE);
	expect(JSON.stringify(sessionStorage)).not.toContain(MADE);
	expect(window.location.href).not.toContain(MADE);

	return unmount(component);
});

/**
 * A reader who set the generator up for their bank's rules does not set it up
 * again after a lock. The first password is made from what Rust remembers,
 * every switch of it, and not from a default written down in the window.
 */
it('opens with the recipe it was last asked for and asks for exactly that', async () => {
	const banking = recipe({
		length: 12,
		alphabets: ['lower', 'digits', 'symbols'],
		similar: true,
		avoid: '"\'\\`'
	});
	ipc.generator.mockResolvedValue(drawing({ recipe: banking }));
	ipc.generatePassword.mockResolvedValue(
		generated(MADE, { generator: drawing({ recipe: banking }) })
	);

	const component = show();
	await vi.waitFor(() => expect(value()).toBe(MADE));
	flushSync();

	expect(ipc.generator).toHaveBeenCalledWith('password');
	expect(ipc.generatePassword).toHaveBeenCalledTimes(1);
	expect(ipc.generatePassword).toHaveBeenCalledWith(banking, 'password');
	expect(pressed('a–z')).toBe('true');
	expect(pressed('A–Z')).toBe('false');
	expect(pressed('0–9')).toBe('true');
	expect(pressed('Symbols')).toBe('true');
	expect(pressed(LOOK_ALIKES)).toBe('false');
	expect(avoidance().value).toBe('"\'\\`');
	expect(slider().value).toBe('12');
	expect(heading()).toEqual({ label: 'Length', length: '12' });

	return unmount(component);
});

/**
 * Until Rust has said what the generator was last asked for, the panel knows
 * no length and no switch. A password asked for then is asked for out of
 * nothing: Rust refuses it, and the reader is told a password needs a kind of
 * character they never turned off. The same holds when what it was last asked
 * for could not be read at all.
 */
it('asks for no password before it knows what to make it of', async () => {
	const opening = Promise.withResolvers<Drawn>();
	ipc.generator.mockReturnValue(opening.promise);
	const onFailure = vi.fn();
	let component = show({ onFailure });
	flushSync();

	expect(slider().disabled).toBe(true);
	expect(avoidance().disabled).toBe(true);
	for (const label of ['a–z', 'A–Z', '0–9', 'Symbols']) expect(button(label).disabled).toBe(true);
	labelled('Make another one').click();
	await settle();
	expect(ipc.generatePassword).not.toHaveBeenCalled();

	opening.resolve(drawing());
	await vi.waitFor(() => expect(value()).toBe(MADE));
	expect(ipc.generatePassword).toHaveBeenCalledTimes(1);
	expect(ipc.generatePassword).toHaveBeenCalledWith(drawing().recipe, 'password');
	expect(onFailure).not.toHaveBeenCalled();
	await unmount(component);

	const unread = { code: 'io', message: 'the generator settings could not be read' };
	ipc.generator.mockRejectedValue(unread);
	ipc.generatePassword.mockClear();
	component = show({ onFailure });
	await vi.waitFor(() => expect(onFailure).toHaveBeenCalledWith(unread));

	labelled('Make another one').click();
	await settle();
	expect(ipc.generatePassword).not.toHaveBeenCalled();
	expect(onFailure).toHaveBeenCalledTimes(1);

	return unmount(component);
});

/** Whatever Rust could not read, the window says, and nothing is made from a
 * recipe nobody chose. */
it('says why it could not open and makes no password', async () => {
	const onFailure = vi.fn();
	const unread = { code: 'io', message: 'the generator settings could not be read' };
	ipc.generator.mockRejectedValue(unread);

	const component = show({ onFailure });
	await vi.waitFor(() => expect(onFailure).toHaveBeenCalledWith(unread));
	await settle();

	expect(onFailure).toHaveBeenCalledTimes(1);
	expect(ipc.generatePassword).not.toHaveBeenCalled();
	expect(value()).toBe('');
	expect(button('Put it in the field').disabled).toBe(true);
	expect(slider().disabled).toBe(true);

	return unmount(component);
});

/**
 * Make another one has nothing to make a password of until the recipe is in,
 * and says so by being off rather than by a press that quietly does nothing.
 * It comes on with the recipe, not with the first password, which may still
 * be on its way; and it stays off when the recipe could not be read.
 */
it('keeps Make another one off until the recipe has arrived', async () => {
	const opening = Promise.withResolvers<Drawn>();
	const first = Promise.withResolvers<Generated>();
	ipc.generator.mockReturnValue(opening.promise);
	ipc.generatePassword.mockReturnValueOnce(first.promise);
	const onFailure = vi.fn();
	let component = show({ onFailure });
	flushSync();
	expect(labelled('Make another one').disabled).toBe(true);
	await settle();
	expect(labelled('Make another one').disabled).toBe(true);

	opening.resolve(drawing());
	await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(1));
	flushSync();
	expect(value()).toBe('');
	expect(labelled('Make another one').disabled).toBe(false);

	labelled('Make another one').click();
	await vi.waitFor(() => expect(value()).toBe(MADE));
	expect(ipc.generatePassword).toHaveBeenCalledTimes(2);
	first.resolve(generated('the one asked for first'));
	await settle();
	expect(value()).toBe(MADE);
	await unmount(component);

	ipc.generator.mockRejectedValue({
		code: 'io',
		message: 'the generator settings could not be read'
	});
	component = show({ onFailure });
	await vi.waitFor(() => expect(onFailure).toHaveBeenCalledTimes(1));
	await settle();
	expect(labelled('Make another one').disabled).toBe(true);

	return unmount(component);
});

/**
 * The password's generator and a field's can be open at once. A label that
 * pointed at an id the other panel also had named the other panel's slider, so
 * a click on "Length" in one moved nothing in it, and a screen reader read out
 * the other one's length.
 */
it('names its own slider and its own avoid field when another generator is open', async () => {
	const second = beside();
	const first = show();
	const other = show({ purpose: 'field', label: 'Generator for PIN' }, second);
	await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(2));
	flushSync();

	for (const panel of [host, second]) {
		for (const label of labels(panel)) expect(panel.contains(named(label))).toBe(true);
	}
	expect(slider(host).id).not.toBe(slider(second).id);
	expect(avoidance(host).id).not.toBe(avoidance(second).id);

	await unmount(other);
	return unmount(first);
});

/**
 * The password's generator and a field's can be open at once, and both offer
 * "Put it in the field". A screen reader moving into one hears which field it
 * is for. A field's name is the reader's own text: markup in it is words, and
 * a character that turns text around is kept in the name rather than acted on.
 */
it('is a group named by its label, and two open panels are named apart', async () => {
	const second = beside();
	const own = 'Generator for <img src=x onerror=alert(1)> \u202Enip';
	const first = show();
	const other = show({ purpose: 'field', label: own }, second);
	await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(2));
	flushSync();

	expect(group(host).getAttribute('aria-label')).toBe('Password generator');
	expect(group(second).getAttribute('aria-label')).toBe(own);
	expect(group(host).getAttribute('aria-label')).not.toBe(group(second).getAttribute('aria-label'));
	for (const panel of [host, second]) {
		expect(panel.querySelectorAll('[role="group"]')).toHaveLength(1);
		const whole = group(panel);
		expect(whole.contains(labelled('Close the generator', panel))).toBe(true);
		expect(whole.contains(slider(panel))).toBe(true);
		expect(whole.contains(button('Put it in the field', panel))).toBe(true);
	}
	expect(document.querySelector('img')).toBeNull();

	await unmount(other);
	return unmount(first);
});

/**
 * The password's generator and a field's remember a recipe each, so a PIN
 * made for a card is not what the password's generator opens with next. Every
 * way a password is asked for - opening, a switch, the look-alike switch, the
 * slider, the avoid field, Make another one - says which one is asking, as it
 * was given and with nothing after it.
 */
it('asks with its own purpose every way a password is asked for', async () => {
	ipc.generatePassword.mockImplementation(async (asked: Recipe) =>
		generated(MADE, { generator: drawing({ recipe: asked }) })
	);
	for (const purpose of ['password', 'field'] as const) {
		ipc.generator.mockClear();
		ipc.generatePassword.mockClear();
		const component = show({ purpose, label: `Generator for ${purpose}` });
		await vi.waitFor(() => expect(value()).toBe(MADE));
		flushSync();

		const asks = [
			() => button('Symbols').click(),
			() => button(LOOK_ALIKES).click(),
			() => slide(30),
			() => type('O0'),
			() => labelled('Make another one').click()
		];
		for (const ask of asks) {
			const already = ipc.generatePassword.mock.calls.length;
			ask();
			await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(already + 1));
		}
		await settle();

		expect(ipc.generator.mock.calls).toEqual([[purpose]]);
		expect(ipc.generatePassword.mock.calls.map((call) => call.slice(1))).toEqual(
			asks.concat(asks[0]).map(() => [purpose])
		);
		expect(ipc.generatePassword).toHaveBeenLastCalledWith(
			recipe({
				length: 30,
				alphabets: ['lower', 'upper', 'digits', 'symbols'],
				similar: true,
				avoid: 'O0'
			}),
			purpose
		);
		await unmount(component);
	}
});

/** Two open at once ask each with its own purpose and show each its own
 * answer. One asking with the other's would leave a card's PIN where the
 * password's generator opens next time. */
it('keeps two open generators to their own purposes', async () => {
	ipc.generatePassword.mockImplementation(async (_asked: Recipe, purpose: Purpose) =>
		generated(`made for the ${purpose}`)
	);
	const second = beside();
	const first = show();
	const other = show({ purpose: 'field', label: 'Generator for PIN' }, second);
	await vi.waitFor(() => {
		expect(value(host)).toBe('made for the password');
		expect(value(second)).toBe('made for the field');
	});
	expect(ipc.generator).toHaveBeenCalledTimes(2);
	expect(ipc.generator).toHaveBeenCalledWith('password');
	expect(ipc.generator).toHaveBeenCalledWith('field');

	labelled('Make another one', second).click();
	await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(3));
	expect(ipc.generatePassword).toHaveBeenLastCalledWith(drawing().recipe, 'field');

	button('Symbols', host).click();
	await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(4));
	expect(ipc.generatePassword).toHaveBeenLastCalledWith(
		recipe({ alphabets: ['lower', 'upper', 'digits', 'symbols'] }),
		'password'
	);
	await settle();
	expect(value(host)).toBe('made for the password');
	expect(value(second)).toBe('made for the field');

	await unmount(other);
	return unmount(first);
});

/**
 * The switch reads as what it does. It is on from the start, which keeps
 * look-alikes out, and turning it off is what lets them in. It used to read
 * "Look-alike characters" and let them in while it was on.
 */
it('keeps look-alikes out from the start and lets them in only with the switch off', async () => {
	const component = show();
	await vi.waitFor(() => expect(value()).toBe(MADE));
	flushSync();

	expect(pressed(LOOK_ALIKES)).toBe('true');
	expect(ipc.generatePassword).toHaveBeenLastCalledWith(recipe({ similar: false }), 'password');
	expect(host.textContent).not.toContain('Look-alike characters');

	button(LOOK_ALIKES).click();
	flushSync();
	expect(pressed(LOOK_ALIKES)).toBe('false');
	await vi.waitFor(() =>
		expect(ipc.generatePassword).toHaveBeenLastCalledWith(recipe({ similar: true }), 'password')
	);

	button(LOOK_ALIKES).click();
	flushSync();
	expect(pressed(LOOK_ALIKES)).toBe('true');
	await vi.waitFor(() =>
		expect(ipc.generatePassword).toHaveBeenLastCalledWith(recipe({ similar: false }), 'password')
	);
	expect(ipc.generatePassword).toHaveBeenCalledTimes(3);

	return unmount(component);
});

/**
 * A PIN keeps its 0 and its 1, so Rust names no look-alikes for one, and a
 * switch reading "Avoid look-alikes ()" would promise to leave out nothing.
 * It is not drawn for a PIN. It comes back with letters as the reader left
 * it, and while it is away what it was set to is still what is asked for.
 */
it('draws no look-alike switch for a PIN and brings it back with letters', async () => {
	const letters = drawing({ recipe: recipe({ length: 8, alphabets: ['digits', 'lower'] }) });
	ipc.generator.mockResolvedValue(PIN);
	ipc.generatePassword.mockResolvedValueOnce(generated('4821', { generator: PIN }));
	const component = show();
	await vi.waitFor(() => expect(value()).toBe('4821'));
	flushSync();

	expect(lookAlikeSwitch()).toBeUndefined();
	expect(host.querySelectorAll('button[aria-pressed]')).toHaveLength(4);

	ipc.generatePassword.mockResolvedValueOnce(generated('k3m9x2p7', { generator: letters }));
	button('a–z').click();
	await vi.waitFor(() => expect(value()).toBe('k3m9x2p7'));
	flushSync();
	expect(ipc.generatePassword).toHaveBeenLastCalledWith(
		recipe({ length: 4, alphabets: ['digits', 'lower'] }),
		'password'
	);
	expect(pressed(LOOK_ALIKES)).toBe('true');

	ipc.generatePassword.mockResolvedValueOnce(generated('k3m9x2p0', { generator: letters }));
	button(LOOK_ALIKES).click();
	await vi.waitFor(() => expect(value()).toBe('k3m9x2p0'));
	flushSync();
	expect(pressed(LOOK_ALIKES)).toBe('false');

	ipc.generatePassword.mockResolvedValueOnce(generated('5821', { generator: PIN }));
	button('a–z').click();
	await vi.waitFor(() => expect(value()).toBe('5821'));
	flushSync();
	expect(lookAlikeSwitch()).toBeUndefined();
	expect(ipc.generatePassword).toHaveBeenLastCalledWith(
		recipe({ length: 8, alphabets: ['digits'], similar: true }),
		'password'
	);

	ipc.generatePassword.mockResolvedValueOnce(generated('p0k3m9x2', { generator: letters }));
	button('a–z').click();
	await vi.waitFor(() => expect(value()).toBe('p0k3m9x2'));
	flushSync();
	expect(pressed(LOOK_ALIKES)).toBe('false');
	expect(ipc.generatePassword).toHaveBeenLastCalledWith(
		recipe({ length: 4, alphabets: ['digits', 'lower'], similar: true }),
		'password'
	);

	return unmount(component);
});

/**
 * "!@#$%" on the switch promised five symbols and the engine drew thirty-two,
 * the quotes and the backslash a bank turns away among them. The switch says
 * "Symbols", and the set is written out as Rust sends it, character for
 * character, so a set Rust changes is the set the window shows.
 */
it('calls the switch Symbols and writes out every symbol Rust draws from', async () => {
	let component = show();
	await vi.waitFor(() => expect(value()).toBe(MADE));
	flushSync();

	const switches = [...host.querySelectorAll('button')].map((each) => each.textContent ?? '');
	expect(switches.some((label) => label.includes('!@#$%'))).toBe(false);
	expect(pressed('Symbols')).toBe('false');
	expect(host.querySelector('p')?.textContent?.trim()).toBe(
		'Symbols: !"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~'
	);

	button('Symbols').click();
	await vi.waitFor(() =>
		expect(ipc.generatePassword).toHaveBeenLastCalledWith(
			recipe({ alphabets: ['lower', 'upper', 'digits', 'symbols'] }),
			'password'
		)
	);
	await unmount(component);

	const narrow = drawing({ symbols: '#%+=@<b>', lookAlikes: '0O' });
	ipc.generator.mockResolvedValue(narrow);
	ipc.generatePassword.mockResolvedValue(generated(MADE, { generator: narrow }));
	component = show();
	await vi.waitFor(() => expect(value()).toBe(MADE));
	flushSync();

	const line = host.querySelector('p');
	expect(line?.textContent?.trim()).toBe('Symbols: #%+=@<b>');
	expect(line?.children).toHaveLength(0);
	expect(pressed('Avoid look-alikes (0 O)')).toBe('true');

	return unmount(component);
});

/** A card, a phone and a door ask for four or six digits. With digits and
 * nothing else, the slider is a PIN's and goes down to four. */
it('offers a PIN of four digits when digits are all it draws from', async () => {
	ipc.generator.mockResolvedValue(PIN);
	ipc.generatePassword.mockResolvedValue(generated('4821', { generator: PIN }));

	const component = show();
	await vi.waitFor(() => expect(value()).toBe('4821'));
	flushSync();

	expect(ipc.generatePassword).toHaveBeenCalledWith(PIN.recipe, 'password');
	expect(heading()).toEqual({ label: 'PIN length', length: '4' });
	expect(slider().min).toBe('4');
	expect(slider().max).toBe('64');
	expect(slider().value).toBe('4');
	expect(ends()).toEqual(['4', '64']);

	return unmount(component);
});

/**
 * The slider learns its shortest end from Rust's answer, so turning the
 * letters off is what lets it down to four. A password is asked for when the
 * slider is let go, not for every step it passes on the way.
 */
it('lets the slider down to four once letters are off, and asks only when it is let go', async () => {
	const component = show();
	await vi.waitFor(() => expect(value()).toBe(MADE));
	flushSync();
	expect(heading()).toEqual({ label: 'Length', length: '24' });
	expect(slider().min).toBe('8');
	expect(ends()).toEqual(['8', '64']);

	ipc.generatePassword.mockResolvedValueOnce(
		generated('Qm7Rk2Hd9TxW4Lp3Nb8Vc6Zs', {
			generator: drawing({ recipe: recipe({ alphabets: ['upper', 'digits'] }) })
		})
	);
	button('a–z').click();
	await vi.waitFor(() => expect(value()).toBe('Qm7Rk2Hd9TxW4Lp3Nb8Vc6Zs'));

	const digits = drawing({ ...PIN, recipe: recipe({ alphabets: ['digits'] }) });
	ipc.generatePassword.mockResolvedValueOnce(
		generated('739204816352947103826459', { generator: digits })
	);
	button('A–Z').click();
	await vi.waitFor(() => expect(value()).toBe('739204816352947103826459'));
	flushSync();
	expect(heading()).toEqual({ label: 'PIN length', length: '24' });
	expect(slider().min).toBe('4');
	expect(ends()).toEqual(['4', '64']);

	const asked = ipc.generatePassword.mock.calls.length;
	for (const step of [20, 12, 6]) slide(step, false);
	flushSync();
	expect(heading().length).toBe('6');
	expect(ipc.generatePassword).toHaveBeenCalledTimes(asked);

	ipc.generatePassword.mockResolvedValueOnce(
		generated('5821', {
			generator: drawing({ ...digits, recipe: recipe({ length: 4, alphabets: ['digits'] }) })
		})
	);
	slide(4);
	await vi.waitFor(() => expect(value()).toBe('5821'));
	expect(ipc.generatePassword).toHaveBeenCalledTimes(asked + 1);
	expect(ipc.generatePassword).toHaveBeenLastCalledWith(
		recipe({ length: 4, alphabets: ['digits'] }),
		'password'
	);

	return unmount(component);
});

/**
 * Letters cannot be as short as a PIN. Turning them on under a four-digit PIN
 * asks for four, and Rust makes eight: the slider and the number beside it
 * move to the eight the password has, and the next password is asked for at
 * eight rather than at a length the slider no longer shows.
 */
it('moves the slider to the length Rust settled on, not the one it asked for', async () => {
	ipc.generator.mockResolvedValue(PIN);
	ipc.generatePassword.mockResolvedValueOnce(generated('4821', { generator: PIN }));
	const component = show();
	await vi.waitFor(() => expect(value()).toBe('4821'));
	flushSync();

	const letters = drawing({ recipe: recipe({ length: 8, alphabets: ['lower', 'digits'] }) });
	ipc.generatePassword.mockResolvedValueOnce(generated('k3m9x2p7', { generator: letters }));
	button('a–z').click();
	await vi.waitFor(() => expect(value()).toBe('k3m9x2p7'));
	flushSync();

	expect(ipc.generatePassword).toHaveBeenLastCalledWith(
		recipe({ length: 4, alphabets: ['digits', 'lower'] }),
		'password'
	);
	expect(heading()).toEqual({ label: 'Length', length: '8' });
	expect(slider().value).toBe('8');
	expect(slider().min).toBe('8');
	expect(ends()).toEqual(['8', '64']);

	ipc.generatePassword.mockResolvedValueOnce(generated('p7k3m9x2', { generator: letters }));
	labelled('Make another one').click();
	await vi.waitFor(() => expect(value()).toBe('p7k3m9x2'));
	expect(ipc.generatePassword).toHaveBeenLastCalledWith(
		recipe({ length: 8, alphabets: ['digits', 'lower'] }),
		'password'
	);

	return unmount(component);
});

/**
 * The generator draws evenly and does not draw again on its own, so a
 * password of twelve can come without a digit. The reader knows what the site
 * wants, and the line says what this one lacks in words a sentence uses,
 * however many kinds that is, and goes back to its own words once nothing is
 * missing.
 */
it('says which kinds the password lacks, and nothing once it lacks none', async () => {
	ipc.generatePassword.mockResolvedValueOnce(generated(MADE, { missing: ['digits'] }));
	const component = show();
	await vi.waitFor(() => expect(said()).toEqual(NO_DIGIT));
	expect(written()).toEqual(NO_DIGIT);
	expect(note().className).toContain('text-warn');

	const cases: [Alphabet[], string[]][] = [
		[
			['digits', 'symbols'],
			['No digit or symbol in this one.', 'Make another if the site asks for them.']
		],
		[
			['upper', 'digits', 'symbols'],
			['No capital letter, digit or symbol in this one.', 'Make another if the site asks for them.']
		],
		[['lower'], ['No lowercase letter in this one.', 'Make another if the site asks for one.']]
	];
	for (const [missing, warning] of cases) {
		ipc.generatePassword.mockResolvedValueOnce(generated(MADE, { missing }));
		labelled('Make another one').click();
		await vi.waitFor(() => expect(said()).toEqual(warning));
		expect(written()).toEqual(warning);
	}

	ipc.generatePassword.mockResolvedValueOnce(generated(MADE));
	labelled('Make another one').click();
	await vi.waitFor(() => expect(said()).toEqual([]));
	expect(written()).toEqual(OSRNG);
	expect(note().className).not.toContain('text-warn');

	return unmount(component);
});

/**
 * Only news is read out: a kind the password on the screen lacks. The
 * footnote about where the randomness comes from is the same for every
 * password, and inside the live region it was read out for each one made.
 * The region is on the page, empty, before anything is made, because one that
 * appears together with its first sentence is not announced at all.
 */
it('reads out what the password lacks and never the footnote', async () => {
	const making = Promise.withResolvers<Generated>();
	ipc.generatePassword.mockReturnValueOnce(making.promise);
	const component = show();
	flushSync();
	const region = status();
	expect(region.textContent?.trim()).toBe('');

	making.resolve(generated(MADE));
	await vi.waitFor(() => expect(value()).toBe(MADE));
	flushSync();
	expect(written()).toEqual(OSRNG);
	expect(said()).toEqual([]);
	expect(status().textContent?.trim()).toBe('');
	expect(status()).toBe(region);

	ipc.generatePassword.mockResolvedValueOnce(generated(MADE, { missing: ['digits'] }));
	labelled('Make another one').click();
	await vi.waitFor(() => expect(said()).toEqual(NO_DIGIT));
	expect(written()).toEqual(NO_DIGIT);
	expect(status()).toBe(region);
	expect(note().contains(region)).toBe(true);

	return unmount(component);
});

/**
 * A second password that lacks a digit too is as much news as the first. A
 * screen reader speaks when what is in a live region changes, and the same
 * words left standing are no change, so the sentence is put there afresh, in
 * new nodes, for every password that has it. The region it goes into stays
 * the one the screen reader is already listening to.
 */
it('says again that a second password lacks a digit, in a new sentence', async () => {
	ipc.generatePassword.mockResolvedValue(generated(MADE, { missing: ['digits'] }));
	const component = show();
	await vi.waitFor(() => expect(said()).toEqual(NO_DIGIT));
	const region = status();

	let earlier = spoken();
	expect(earlier.length).toBeGreaterThan(0);
	for (const made of [2, 3]) {
		labelled('Make another one').click();
		await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(made));
		await settle();

		const now = spoken();
		expect(said()).toEqual(NO_DIGIT);
		expect(status()).toBe(region);
		expect(now.length).toBeGreaterThan(0);
		for (const node of earlier) {
			expect(node.isConnected).toBe(false);
			expect(now).not.toContain(node);
		}
		earlier = now;
	}

	return unmount(component);
});

/**
 * Whatever the reader types into the avoid field is sent as it stands, on
 * every keystroke: the quote, the apostrophe and the backslash a bank turns
 * away, a character that turns text around, one that is two code units, and
 * spaces at either end, none of them trimmed, escaped or dropped on the way.
 * Rust decides what of it a password could have held.
 */
it('sends the characters to avoid exactly as they were typed', async () => {
	const component = show();
	await vi.waitFor(() => expect(value()).toBe(MADE));
	flushSync();

	for (const typed of ['"', '"\'\\`', ' \u202Eabc ', '\u{1F600}O0', '<script>&amp;', '']) {
		const before = ipc.generatePassword.mock.calls.length;
		type(typed);
		await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(before + 1));
		expect(ipc.generatePassword).toHaveBeenLastCalledWith(recipe({ avoid: typed }), 'password');
		await settle();
		expect(avoidance().value).toBe(typed);
	}

	return unmount(component);
});

/**
 * A switch pressed while a password is on its way asks again, and the two
 * can come back in either order. The older one arriving last would put a
 * password on the screen the switches no longer describe, with its own hint
 * and its own length.
 */
it('shows the password asked for last when an older one comes back after it', async () => {
	const slow = Promise.withResolvers<Generated>();
	const fast = Promise.withResolvers<Generated>();
	ipc.generatePassword.mockReturnValueOnce(slow.promise).mockReturnValueOnce(fast.promise);
	const component = show();
	await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(1));
	flushSync();

	button('Symbols').click();
	await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(2));

	fast.resolve(generated('the one with symbols'));
	await vi.waitFor(() => expect(value()).toBe('the one with symbols'));

	slow.resolve(
		generated('the one before', {
			missing: ['digits'],
			generator: drawing({ recipe: recipe({ length: 12 }) })
		})
	);
	await settle();

	expect(value()).toBe('the one with symbols');
	expect(host.textContent).not.toContain('the one before');
	expect(said()).toEqual([]);
	expect(written()).toEqual(OSRNG);
	expect(heading().length).toBe('24');

	return unmount(component);
});

/** A refusal is about the recipe it was asked for. Once a newer one has made
 * a password, the refusal of the older one is not news, and saying it would
 * leave a notice beside a password that works. */
it('says nothing of a refusal for a recipe already replaced', async () => {
	const onFailure = vi.fn();
	const slow = Promise.withResolvers<Generated>();
	ipc.generatePassword.mockReturnValueOnce(slow.promise);
	const component = show({ onFailure });
	await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(1));
	flushSync();

	button('Symbols').click();
	await vi.waitFor(() => expect(value()).toBe(MADE));

	slow.reject(REFUSAL);
	await settle();

	expect(onFailure).not.toHaveBeenCalled();
	expect(value()).toBe(MADE);
	expect(button('Put it in the field').disabled).toBe(false);

	return unmount(component);
});

/** A refusal takes the last password off the screen, and the line that said
 * what it lacked goes with it: a hint about a password that is not there is a
 * hint about nothing. */
it('reports a refusal rather than making something out of nothing', async () => {
	const onFailure = vi.fn();
	ipc.generatePassword.mockResolvedValueOnce(generated(MADE, { missing: ['digits'] }));
	const component = show({ onFailure });
	await vi.waitFor(() => expect(said()).toEqual(NO_DIGIT));
	const region = status();

	ipc.generatePassword.mockRejectedValueOnce(REFUSAL);
	labelled('Make another one').click();
	await vi.waitFor(() => expect(onFailure).toHaveBeenCalledWith(REFUSAL));
	flushSync();

	expect(value()).toBe('');
	expect(status()).toBe(region);
	expect(said()).toEqual([]);
	expect(written()).toEqual(OSRNG);
	expect(note().className).not.toContain('text-warn');
	expect(button('Put it in the field').disabled).toBe(true);

	return unmount(component);
});

/** The reader can turn every set off. The engine refuses to make a password
 * out of nothing, and the message it sends back is what the window shows,
 * once, without the password that was on the screen before it. */
it('reports the refusal Rust sends when every set is turned off', async () => {
	const onFailure = vi.fn();
	ipc.generatePassword.mockImplementation(async (asked: Recipe) => {
		if (asked.alphabets.length === 0) throw REFUSAL;
		return generated(MADE);
	});
	const component = show({ onFailure });
	await vi.waitFor(() => expect(value()).toBe(MADE));
	flushSync();

	for (const label of ['a–z', 'A–Z', '0–9']) button(label).click();
	await vi.waitFor(() => expect(onFailure).toHaveBeenCalledWith(REFUSAL));
	await settle();

	expect(ipc.generatePassword).toHaveBeenLastCalledWith(recipe({ alphabets: [] }), 'password');
	for (const label of ['a–z', 'A–Z', '0–9', 'Symbols']) expect(pressed(label)).toBe('false');
	expect(onFailure).toHaveBeenCalledTimes(1);
	expect(JSON.stringify(onFailure.mock.calls)).not.toContain(MADE);
	expect(value()).toBe('');
	expect(button('Put it in the field').disabled).toBe(true);

	button('0–9').click();
	await vi.waitFor(() => expect(value()).toBe(MADE));
	expect(ipc.generatePassword).toHaveBeenLastCalledWith(
		recipe({ alphabets: ['digits'] }),
		'password'
	);
	expect(button('Put it in the field').disabled).toBe(false);

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
	expect(onInsert).toHaveBeenCalledTimes(1);
	expect(onClose).toHaveBeenCalledTimes(1);
	expect(value()).toBe('');

	return unmount(component);
});

/** What goes into the field is the password the reader was looking at. One
 * asked for a moment before and still on its way is somebody else's, and
 * finds no screen to go to when it comes. */
it('puts in the field the password on the screen, not one still on its way', async () => {
	const onInsert = vi.fn();
	const component = show({ onInsert });
	await vi.waitFor(() => expect(value()).toBe(MADE));
	flushSync();

	const next = Promise.withResolvers<Generated>();
	ipc.generatePassword.mockReturnValueOnce(next.promise);
	labelled('Make another one').click();
	await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(2));

	button('Put it in the field').click();
	flushSync();
	expect(onInsert).toHaveBeenCalledWith(MADE);

	next.resolve(generated('one nobody saw', { missing: ['digits'] }));
	await settle();

	expect(value()).toBe('');
	expect(said()).toEqual([]);
	expect(written()).toEqual(OSRNG);
	expect(button('Put it in the field').disabled).toBe(true);
	expect(onInsert).toHaveBeenCalledTimes(1);

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

/**
 * A password still on its way when the pane goes has nowhere to go. Written
 * into the node the panel left behind, it would be a secret in an element
 * nothing will ever wipe; and a refusal arriving then is a notice about a
 * panel that is not there.
 */
it('writes nothing anywhere when an answer arrives after the panel went', async () => {
	const onFailure = vi.fn();
	const late = Promise.withResolvers<Generated>();
	ipc.generatePassword.mockReturnValueOnce(late.promise);
	let component = show({ onFailure });
	await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(1));
	const left = host.querySelector('[data-value]');
	await unmount(component);

	late.resolve(generated(MADE, { missing: ['digits'] }));
	await settle();

	expect(left?.textContent).toBe('');
	expect(document.body.textContent).not.toContain(MADE);

	const refused = Promise.withResolvers<Generated>();
	ipc.generatePassword.mockReturnValueOnce(refused.promise);
	component = show({ onFailure });
	await vi.waitFor(() => expect(ipc.generatePassword).toHaveBeenCalledTimes(2));
	await unmount(component);

	refused.reject(REFUSAL);
	await settle();
	expect(onFailure).not.toHaveBeenCalled();
});

/** The recipe arriving after the pane went is no reason to make a password:
 * nobody will see it, and Rust would remember a recipe nobody pressed. */
it('asks for no password when what to make it of arrives after the panel went', async () => {
	const opening = Promise.withResolvers<Drawn>();
	ipc.generator.mockReturnValue(opening.promise);
	const component = show();
	flushSync();
	await unmount(component);

	opening.resolve(drawing());
	await settle();

	expect(ipc.generatePassword).not.toHaveBeenCalled();
});

/**
 * A made password is not in the vault yet, so nothing in Rust can copy it by
 * name, and the system's copy of it would be the plain pasteboard write the
 * rest of the window is kept away from. The copy is refused, with the way that
 * works, and the menu and the drag that would hand it to another application
 * do not start.
 */
it('keeps a made password out of the system copy, its menu and a drag', async () => {
	const onFailure = vi.fn();
	const component = show({ onFailure });
	await vi.waitFor(() => expect(value()).toBe(MADE));
	const node = host.querySelector('[data-value]') as HTMLElement;

	for (const kind of ['copy', 'cut']) {
		const event = new ClipboardEvent(kind, { bubbles: true, cancelable: true });
		node.dispatchEvent(event);
		expect(event.defaultPrevented, `${kind} was left to the system`).toBe(true);
	}
	expect(onFailure).toHaveBeenCalledWith(
		expect.objectContaining({ message: 'Put it in the field first, and copy it from there.' })
	);
	expect(JSON.stringify(onFailure.mock.calls)).not.toContain(MADE);

	for (const kind of ['contextmenu', 'dragstart']) {
		const event = new MouseEvent(kind, { bubbles: true, cancelable: true });
		node.dispatchEvent(event);
		expect(event.defaultPrevented, `${kind} was left to the system`).toBe(true);
	}

	return unmount(component);
});
