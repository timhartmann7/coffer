import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Stubbed } from '$lib/stubbed';
import type { OnDisk, SnapshotOf } from '$lib/model';
import InBackup from './InBackup.svelte';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('$lib/ipc'), async (real) =>
	Object.assign(ipc, (await import('$lib/stubbed')).stubbed(await real()))
);

const backup: SnapshotOf = {
	vault: 'personal.kdbx',
	taken: '2026-08-27T18:40:00Z',
	keptAs: 'personal.kdbx.1.bak',
	vaultFile: { there: true, written: null },
	because: 'asked'
};

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => {
	host.remove();
});

function strip(
	over: Partial<SnapshotOf> = {},
	onAdopt = vi.fn().mockResolvedValue(undefined),
	onBack = vi.fn().mockResolvedValue(undefined)
) {
	const component = mount(InBackup, {
		target: host,
		props: { snapshot: { ...backup, ...over }, onAdopt, onBack }
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
 * Which backup this is, to the minute in the reader's clock, and why it is
 * open: the vault's file would not open, is not there, or the reader asked to
 * look - in which case what there is to say is that nothing can be changed.
 */
it('says which backup this is, and why it is open', () => {
	const cases: [Partial<SnapshotOf>, string][] = [
		[{}, 'as it was on 27 Aug at 18:40. Nothing in it can be changed.'],
		[{ because: 'unopened' }, 'as it was on 27 Aug at 18:40. Your vault file could not be opened.'],
		[
			{ because: 'unopened', vaultFile: { there: false, written: null } },
			'as it was on 27 Aug at 18:40. Your vault file is not there any more.'
		],
		[
			{ taken: null },
			'You’re looking at a backup Coffer took before one of its saves. Nothing in it can be changed.'
		]
	];

	for (const [over, said] of cases) {
		const component = strip(over);
		expect(reads()).toContain(said);
		unmount(component);
	}
});

/**
 * What using it does to the vault's file: which file it goes over, when that
 * file last changed, and both things that can become of it, because which one
 * is not known until the press. And which password opens the vault from then
 * on - the one the backup was taken under - before the press, not after it.
 * Nothing is promised about a file that has gone.
 */
it('says what using it does to the vault file, and promises nothing for one that has gone', () => {
	let component = strip({ vaultFile: { there: true, written: '2026-08-28T09:07:00Z' } });
	expect(reads()).toContain(
		'Using it as your vault puts it in place of personal.kdbx, last changed on 28 Aug at 9:07. If that file opens with this backup’s password, it is kept as personal.kdbx.1.bak until later saves push it out of the backups; if not, it is kept beside it under a name of its own. Nothing is deleted.'
	);
	expect(reads()).toContain(
		'From then on, your vault opens with the password this backup opens with.'
	);
	expect(button('Use this copy as my vault').className).toContain('bg-accent');
	expect(button('Back to my vault').className).not.toContain('bg-accent');
	expect(button('Save a copy as…').className).not.toContain('bg-accent');
	unmount(component);

	component = strip({ vaultFile: { there: false, written: null } });
	expect(reads()).toContain('Using it as your vault puts it back as personal.kdbx.');
	expect(reads()).not.toContain('.1.bak');
	expect(reads()).not.toContain('Nothing is deleted');
	expect(reads()).toContain(
		'From then on, your vault opens with the password this backup opens with.'
	);
	unmount(component);
});

/** Every answer ends what the strip is about, or writes from the vault the
 * others would close, so a second press on any of them while one is on its way
 * does nothing. */
it('makes it the vault once however often it is pressed', async () => {
	const pending = Promise.withResolvers<void>();
	const onAdopt = vi.fn().mockReturnValue(pending.promise);
	const onBack = vi.fn().mockResolvedValue(undefined);
	const component = strip({}, onAdopt, onBack);

	button('Use this copy as my vault').click();
	flushSync();
	expect(reads()).toContain('Making it your vault…');
	button('Making it your vault…').click();
	button('Back to my vault').click();
	button('Save a copy as…').click();
	expect(onAdopt).toHaveBeenCalledTimes(1);
	expect(onBack).not.toHaveBeenCalled();
	expect(ipc.saveCopy).not.toHaveBeenCalled();
	expect(button('Back to my vault').disabled).toBe(true);

	pending.resolve();
	await vi.waitFor(() => expect(reads()).toContain('Use this copy as my vault'));
	expect(button('Back to my vault').disabled).toBe(false);

	unmount(component);
});

/**
 * Rust holds the press to the vault's file as the strip last described it.
 * When somebody wrote it since, nothing is replaced, and the strip says so in
 * the words the copy's strip uses; any other refusal is Rust's own sentence.
 */
it('says why nothing was replaced when the vault file changed', async () => {
	const changed = vi.fn().mockRejectedValue({
		code: 'externalChange',
		message: 'the vault file is not as it stood when it was shown'
	});
	let component = strip({}, changed);
	button('Use this copy as my vault').click();
	await vi.waitFor(() =>
		expect(reads()).toContain(
			'Your vault file changed after this was shown, so nothing was replaced. What is said above is how it stands now.'
		)
	);
	expect(button('Use this copy as my vault').disabled).toBe(false);
	unmount(component);

	const held = vi.fn().mockRejectedValue({
		code: 'heldByAnother',
		message: 'someone has this vault open on a-mac'
	});
	component = strip({}, held);
	button('Use this copy as my vault').click();
	await vi.waitFor(() => expect(reads()).toContain('someone has this vault open on a-mac'));
	unmount(component);
});

/** The backup written to a file of its own, wherever the reader says: where it
 * went is said under the strip's sentences. A panel closed says nothing, and
 * nothing else on the strip changes either way. */
it('keeps a copy and says where, and says nothing for a closed panel', async () => {
	const onAdopt = vi.fn();
	ipc.saveCopy.mockResolvedValueOnce(null);
	const component = strip({}, onAdopt);

	button('Save a copy as…').click();
	await vi.waitFor(() => expect(ipc.saveCopy).toHaveBeenCalledTimes(1));
	await vi.waitFor(() => expect(button('Save a copy as…').disabled).toBe(false));
	expect(reads()).not.toContain('Kept as');

	ipc.saveCopy.mockResolvedValueOnce({ path: '/Users/someone/Desktop/old.kdbx', name: 'old' });
	button('Save a copy as…').click();
	await vi.waitFor(() => expect(reads()).toContain('Kept as “⁨old⁩”'));
	expect(onAdopt).not.toHaveBeenCalled();

	unmount(component);
});

/** "Back to my vault" goes back once however often it is pressed, and nothing
 * else on the strip runs meanwhile. */
it('goes back to the vault once, and only that', async () => {
	const pending = Promise.withResolvers<void>();
	const onAdopt = vi.fn();
	const onBack = vi.fn().mockReturnValue(pending.promise);
	const component = strip({}, onAdopt, onBack);

	button('Back to my vault').click();
	button('Back to my vault').click();
	button('Use this copy as my vault').click();
	flushSync();
	expect(onBack).toHaveBeenCalledTimes(1);
	expect(onAdopt).not.toHaveBeenCalled();
	expect(button('Save a copy as…').disabled).toBe(true);

	pending.resolve();
	await vi.waitFor(() => expect(button('Back to my vault').disabled).toBe(false));
	unmount(component);
});

/** A copy the save panel was answered for and Rust would not write - a name
 * already taken beside the vault - is said where the press was, in Rust's
 * words, and nothing is said to have been kept. */
it('says why a copy was not written', async () => {
	ipc.saveCopy.mockRejectedValueOnce({
		code: 'taken',
		message: 'there is already a file with that name'
	});
	const component = strip();

	button('Save a copy as…').click();
	await vi.waitFor(() => expect(reads()).toContain('there is already a file with that name'));
	expect(reads()).not.toContain('Kept as');
	expect(button('Save a copy as…').disabled).toBe(false);

	unmount(component);
});

/** A vault's file name is the reader's and can hold markup or an override. It
 * is drawn as the characters it is, and an override stays inside its name. */
it('draws a vault name holding markup or an override as the characters it is', () => {
	for (const vaultFile of [
		{ there: true, written: null },
		{ there: false, written: null }
	] satisfies OnDisk[]) {
		const component = strip({
			vault: '<img src=x onerror=alert(1)>‮x.kdbx',
			keptAs: '<img src=x onerror=alert(1)>‮x.kdbx.1.bak',
			vaultFile
		});

		expect(host.querySelector('img')).toBeNull();
		const names = [...host.querySelectorAll('bdi')].map((each) => each.textContent);
		expect(names[0]).toBe('<img src=x onerror=alert(1)>‮x.kdbx');
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
