import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import InCopy from './InCopy.svelte';
import type { CopyOf, OnDisk } from '$lib/model';

vi.mock('$lib/ipc', () => ({
	asFailure: (thrown: unknown) => thrown as { code: string; message: string }
}));

const copy: CopyOf = {
	vault: 'personal.kdbx',
	saved: '2026-09-01T14:05:00Z',
	keptAs: 'personal.kdbx.1.bak',
	vaultFile: { there: true, written: null }
};

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => {
	host.remove();
});

function banner(
	vaultFile: OnDisk,
	onPromote = vi.fn().mockResolvedValue(undefined),
	onBack = vi.fn().mockResolvedValue(undefined),
	about: Omit<CopyOf, 'vaultFile'> = copy
) {
	const component = mount(InCopy, {
		target: host,
		props: { copy: { ...about, vaultFile }, onPromote, onBack }
	});
	flushSync();
	return component;
}

function reads(): string {
	return (host.textContent ?? '').replace(/\s+/g, ' ').trim();
}

function button(label: string): HTMLButtonElement {
	const found = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === label
	);
	if (!found) throw new Error(`there is no button called ${label}`);
	return found;
}

/**
 * Inside the copy, the banner says what it is and what editing it does, and
 * says plainly what "Make this my vault" does to the vault's file: which file
 * it goes over, when that file last changed - how the reader tells whether
 * somebody wrote it after the copy was made - and the name it is kept under.
 */
it('says it is the copy, and what becomes of the vault file it would replace', () => {
	const component = banner({ there: true, written: '2026-09-01T14:07:00Z' });

	expect(reads()).toContain('This is the copy saved on 1 Sep at 14:05.');
	expect(reads()).toContain('What you change here changes the copy, not your vault.');
	expect(reads()).toContain(
		'puts it in place of personal.kdbx, last changed on 1 Sep at 14:07, which is kept as personal.kdbx.1.bak until later saves push it out of the snapshots.'
	);
	expect(button('Make this my vault').className).toContain('bg-accent');
	expect(button('Back to my vault').className).not.toContain('bg-accent');

	unmount(component);
});

/** A filesystem that keeps no times still gets whole sentences, and a vault
 * whose file has gone is not promised a snapshot it will never have. */
it('says only what it knows about the times, and nothing is kept of a vault that has gone', () => {
	let component = banner({ there: true, written: null }, undefined, undefined, {
		...copy,
		saved: null
	});
	expect(reads()).toContain('This is the copy a lock saved. What you change');
	expect(reads()).toContain(
		'puts it in place of personal.kdbx, which is kept as personal.kdbx.1.bak until later saves push it out of the snapshots.'
	);
	unmount(component);

	component = banner({ there: false, written: null });
	expect(reads()).toContain(
		'personal.kdbx is not there any more, so making this your vault puts the copy in its place.'
	);
	expect(reads()).not.toContain('.1.bak');
	unmount(component);
});

/** Either answer ends what the banner is about, so a second press - on the
 * same button or the other one - while the first is on its way does nothing. */
it('answers one press at a time', async () => {
	const pending = Promise.withResolvers<void>();
	const onPromote = vi.fn().mockReturnValue(pending.promise);
	const onBack = vi.fn().mockResolvedValue(undefined);
	const component = banner({ there: true, written: null }, onPromote, onBack);

	button('Make this my vault').click();
	flushSync();
	expect(reads()).toContain('Making it your vault…');
	button('Making it your vault…').click();
	button('Back to my vault').click();
	expect(onPromote).toHaveBeenCalledTimes(1);
	expect(onBack).not.toHaveBeenCalled();

	pending.resolve();
	await vi.waitFor(() => expect(reads()).toContain('Make this my vault'));

	button('Back to my vault').click();
	await vi.waitFor(() => expect(onBack).toHaveBeenCalledTimes(1));

	unmount(component);
});

/** A refusal - somebody holding the vault, a folder that takes nothing - is
 * said where the press was, and the banner stays so the reader can try again
 * or go back. */
it('says why the copy did not become the vault, where the press was', async () => {
	const onPromote = vi.fn().mockRejectedValue({
		code: 'heldByAnother',
		message: 'someone has this vault open on a-mac'
	});
	const component = banner({ there: true, written: null }, onPromote);

	button('Make this my vault').click();
	await vi.waitFor(() => expect(reads()).toContain('someone has this vault open on a-mac'));
	expect(button('Make this my vault').disabled).toBe(false);

	unmount(component);
});

/**
 * Rust holds the press to the vault's file as the banner last described it.
 * When somebody wrote it since, nothing is replaced, and the banner says so in
 * words about itself rather than Rust's: the time it shows has been read again
 * by then, and the reader looks at it before pressing again.
 */
it('says nothing was replaced when the vault file changed after the banner said how it stood', async () => {
	const onPromote = vi.fn().mockRejectedValue({
		code: 'externalChange',
		message: 'the vault file is not as it stood when it was shown'
	});
	const component = banner({ there: true, written: '2026-09-01T14:07:00Z' }, onPromote);

	button('Make this my vault').click();
	await vi.waitFor(() =>
		expect(reads()).toContain(
			'Your vault file changed after this was shown, so nothing was replaced. What is said above is how it stands now.'
		)
	);
	expect(button('Make this my vault').disabled).toBe(false);

	unmount(component);
});

/** A vault's file name is the reader's, and can hold anything a file name can.
 * It is drawn as the characters it is. */
it('draws a vault name that holds markup as the characters it is', () => {
	const component = banner({ there: true, written: null }, undefined, undefined, {
		vault: '<img src=x onerror=alert(1)>.kdbx',
		saved: null,
		keptAs: '<img src=x onerror=alert(1)>.kdbx.1.bak'
	});

	expect(host.querySelector('img')).toBeNull();
	expect(reads()).toContain('<img src=x onerror=alert(1)>.kdbx.1.bak');

	unmount(component);
});

/**
 * A file name can hold a right-to-left override, and one in the vault's name
 * would turn the rest of the sentence around: which file is replaced, and the
 * name it is kept under. Each name is isolated in a bdi of its own, which is
 * where the override ends.
 */
it('keeps a right-to-left override in a file name off the rest of the sentence', () => {
	for (const there of [true, false]) {
		const component = banner({ there, written: null }, undefined, undefined, {
			vault: 'evil‮xcbdk.kdbx',
			saved: null,
			keptAs: 'evil‮xcbdk.kdbx.1.bak'
		});

		const names = [...host.querySelectorAll('bdi')].map((each) => each.textContent);
		expect(names).toEqual(
			there ? ['evil‮xcbdk.kdbx', 'evil‮xcbdk.kdbx.1.bak'] : ['evil‮xcbdk.kdbx']
		);
		for (const text of host.querySelectorAll('p')) {
			const loose = [...text.childNodes]
				.filter((node) => node.nodeType === Node.TEXT_NODE)
				.map((node) => node.textContent)
				.join('');
			expect(loose).not.toMatch(/[‪-‮]/u);
		}

		unmount(component);
	}
});
