import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Stubbed } from '$lib/stubbed';
import OldBackups from './OldBackups.svelte';

const ipc = vi.hoisted(() => ({}) as Stubbed);
vi.mock(import('$lib/ipc'), async (real) =>
	Object.assign(ipc, (await import('$lib/stubbed')).stubbed(await real()))
);

let host: HTMLElement;
let drawn: ReturnType<typeof mount> | null = null;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => {
	if (drawn) void unmount(drawn);
	drawn = null;
	document.body.replaceChildren();
});

/** Asks about `count` old backups, and keeps how the question was answered. */
function ask(count: number) {
	const onDone = vi.fn<(said: string | null) => void>();
	drawn = mount(OldBackups, { target: host, props: { count, onDone } });
	flushSync();
	return onDone;
}

function button(label: string): HTMLButtonElement {
	const found = [...host.querySelectorAll('button')].find(
		(candidate) => candidate.textContent?.trim() === label
	);
	if (!found) throw new Error(`no button labelled ${label}`);
	return found;
}

it('asks before removing the old backups, and removes nothing when they are kept', () => {
	const onDone = ask(4);

	expect(host.textContent).toContain(
		'Master password changed. Your 4 automatic backups of earlier saves still open with the old password until later saves replace them. Removing them can’t be undone.'
	);
	expect(document.activeElement, 'the focus is not on the way out').toBe(button('Keep them'));

	button('Keep them').click();
	expect(onDone).toHaveBeenCalledExactlyOnceWith(null);
	expect(ipc.removeOldSnapshots).not.toHaveBeenCalled();
});

/** Escape is Keep, and goes no further: the settings stay, with the question
 * answered. */
it('keeps the old backups on Escape and leaves the settings open', () => {
	const onDone = ask(2);

	const seen = vi.fn();
	window.addEventListener('keydown', seen);
	button('Keep them').dispatchEvent(
		new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
	);
	window.removeEventListener('keydown', seen);

	expect(seen, 'the settings heard the Escape that kept the backups').not.toHaveBeenCalled();
	expect(onDone).toHaveBeenCalledExactlyOnceWith(null);
	expect(ipc.removeOldSnapshots).not.toHaveBeenCalled();
});

it('says one backup in the singular', () => {
	ask(1);

	expect(host.textContent).toContain(
		'Master password changed. Your 1 automatic backup of an earlier save still opens with the old password until a later save replaces it. Removing it can’t be undone.'
	);
	expect(button('Keep it')).toBeDefined();
	expect(button('Remove the old backup')).toBeDefined();
});

it('removes the old backups once however often Remove is pressed, and says how many went', async () => {
	let answer: (removed: unknown) => void = () => {};
	ipc.removeOldSnapshots.mockReturnValue(new Promise((settle) => (answer = settle)));
	const onDone = ask(4);

	button('Remove old backups').click();
	button('Remove old backups').click();
	flushSync();
	expect(ipc.removeOldSnapshots).toHaveBeenCalledTimes(1);

	answer({ gone: 4, left: 0, refused: null });
	await vi.waitFor(() => expect(onDone).toHaveBeenCalledExactlyOnceWith('Removed 4 old backups.'));
});

/** Later saves may have pushed every one of them out of the chain before the
 * reader answered; or exactly one went. */
it('says how many went in words that fit the number', async () => {
	for (const [gone, said] of [
		[0, 'No old backups were left to remove.'],
		[1, 'Removed the old backup.']
	] as const) {
		ipc.removeOldSnapshots.mockResolvedValue({ gone, left: 0, refused: null });
		const onDone = ask(1);

		button('Remove the old backup').click();
		await vi.waitFor(() => expect(onDone).toHaveBeenCalledExactlyOnceWith(said));
		if (drawn) await unmount(drawn);
		drawn = null;
	}
});

/** Three went and one would not. The question goes on, about the one that is
 * left - the count it gave a moment ago is no longer true - and says why it
 * stayed; the next press takes what Rust still remembers. */
it('asks about the ones that are left when not every backup could be removed, and says why', async () => {
	ipc.removeOldSnapshots.mockResolvedValueOnce({
		gone: 3,
		left: 1,
		refused: { code: 'io', message: 'Operation not permitted (os error 1)' }
	});
	const onDone = ask(4);
	const regions = [...host.querySelectorAll('[role="status"]')];

	button('Remove old backups').click();
	await vi.waitFor(() =>
		expect(host.textContent).toContain(
			'Not every old backup could be removed: Operation not permitted (os error 1).'
		)
	);
	const line = [...host.querySelectorAll('p')].find((each) =>
		each.textContent?.includes('Not every old backup')
	);
	expect(regions, 'why is said where nothing reads it out').toContain(
		line?.closest('[role="status"]')
	);
	expect(host.textContent).toContain(
		'Your 1 automatic backup of an earlier save still opens with the old password'
	);
	expect(host.textContent).not.toContain('Your 4 automatic backups');
	expect(onDone).not.toHaveBeenCalled();

	ipc.removeOldSnapshots.mockResolvedValueOnce({ gone: 1, left: 0, refused: null });
	button('Remove the old backup').click();
	await vi.waitFor(() => expect(onDone).toHaveBeenCalledExactlyOnceWith('Removed the old backup.'));
});

/** Nothing went: the directory took nothing away, or the call never reached
 * the disk. "Not every" would say some had gone. */
it('says none could be removed when none went, and keeps the question about all of them', async () => {
	ipc.removeOldSnapshots.mockResolvedValueOnce({
		gone: 0,
		left: 4,
		refused: { code: 'io', message: 'Permission denied (os error 13)' }
	});
	const onDone = ask(4);

	button('Remove old backups').click();
	await vi.waitFor(() =>
		expect(host.textContent).toContain(
			'The old backups could not be removed: Permission denied (os error 13).'
		)
	);
	expect(host.textContent).not.toContain('Not every old backup');
	expect(host.textContent).toContain('Your 4 automatic backups');
	expect(onDone).not.toHaveBeenCalled();
});

it('keeps the question when the removal could not be asked for, and says why', async () => {
	ipc.removeOldSnapshots.mockRejectedValue({ code: 'noVault', message: 'no database is open' });
	const onDone = ask(4);

	button('Remove old backups').click();
	await vi.waitFor(() =>
		expect(host.textContent).toContain('The old backups could not be removed: no database is open.')
	);
	expect(host.textContent).not.toContain('Not every old backup');
	expect(host.querySelector('[data-confirm]'), 'the question went with the backups').not.toBeNull();
	expect(host.textContent).toContain('Your 4 automatic backups');
	expect(onDone).not.toHaveBeenCalled();
});
