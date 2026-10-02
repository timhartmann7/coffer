import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Snapshot } from '$lib/model';
import Backups from './Backups.svelte';

vi.mock(import('$lib/ipc'), async (real) => (await import('$lib/stubbed')).stubbed(await real()));

/** When the list is drawn, in the UTC clock the suite runs in. */
const NOW = new Date('2026-08-29T14:30:00Z');

/** Ten saves a minute apart, newest first, as Rust lists them. */
function chain(): Snapshot[] {
	return Array.from({ length: 10 }, (_, at) => ({
		name: `personal.kdbx.${at + 1}.bak`,
		index: at + 1,
		taken: new Date(NOW.getTime() - (at + 1) * 60_000).toISOString()
	}));
}

let host: HTMLElement;

beforeEach(() => {
	host = document.createElement('div');
	document.body.appendChild(host);
});

afterEach(() => host.remove());

function list(
	backups: Snapshot[],
	over: Partial<{
		open: string | null;
		onOpen: (index: number) => Promise<void>;
		onAgain: () => Promise<void>;
	}> = {}
) {
	const component = mount(Backups, {
		target: host,
		props: {
			backups,
			act: 'Open',
			onOpen: vi.fn().mockResolvedValue(undefined),
			onAgain: vi.fn().mockResolvedValue(undefined),
			...over
		}
	});
	flushSync();
	return component;
}

function rows(): HTMLLIElement[] {
	return [...host.querySelectorAll('li')];
}

function reads(): string {
	return (host.textContent ?? '').replace(/\s+/g, ' ').trim();
}

/** Every backup is a row, in the order Rust gave, with the minute it holds
 * the vault as and its file name; the capital on "Today" is the row's to put
 * there, so the words are the ones a sentence uses. */
it('lists every backup newest first, to the minute', () => {
	vi.useFakeTimers({ toFake: ['Date'] });
	vi.setSystemTime(NOW);
	try {
		const backups = chain();
		const component = list(backups);

		expect(rows()).toHaveLength(10);
		expect(rows().map((row) => row.querySelector('bdi')?.textContent)).toEqual(
			backups.map((backup) => backup.name)
		);
		const times = rows().map((row) => row.querySelector('span')?.textContent);
		expect(times[0]).toBe('today, 14:29');
		expect(times[9]).toBe('today, 14:20');
		expect(rows()[0].querySelector('span')?.className).toContain('first-letter:uppercase');

		unmount(component);
	} finally {
		vi.useRealTimers();
	}
});

/** A filesystem that keeps no times still lists its backups, and says so
 * rather than leaving a hole where the time goes. */
it('says the time was not kept, and still offers the backup', () => {
	const component = list([{ name: 'personal.kdbx.1.bak', index: 1, taken: null }]);

	expect(reads()).toContain('Time not kept');
	expect(rows()[0].querySelector('button')?.textContent?.trim()).toBe('Open');

	unmount(component);
});

/** A row is opened by the slot it was shown at, once however often it is
 * pressed, and its button is named after the file for whoever cannot see the
 * row. */
it('opens the one pressed by its slot, once', async () => {
	const pending = Promise.withResolvers<void>();
	const onOpen = vi.fn().mockReturnValue(pending.promise);
	const component = list(chain(), { onOpen });

	const third = rows()[2].querySelector('button');
	expect(third?.getAttribute('aria-label')).toBe('Open personal.kdbx.3.bak');
	third?.click();
	third?.click();
	rows()[4].querySelector('button')?.click();
	flushSync();

	expect(onOpen).toHaveBeenCalledTimes(1);
	expect(onOpen).toHaveBeenCalledWith(3);
	expect(third?.disabled).toBe(true);

	pending.resolve();
	await vi.waitFor(() => expect(third?.disabled).toBe(false));
	unmount(component);
});

/** The backup that was shown went before the press reached it - pushed out of
 * the chain by a save. The list says so and is read again; nothing is opened
 * in its place. */
it('says a backup that went is gone and reads the list again', async () => {
	const onOpen = vi.fn().mockRejectedValue({ code: 'gone', message: 'the database file is gone' });
	const onAgain = vi.fn().mockResolvedValue(undefined);
	const component = list(chain(), { onOpen, onAgain });

	rows()[9].querySelector('button')?.click();
	await vi.waitFor(() =>
		expect(reads()).toContain('That backup is not there any more. The list is as it stands now.')
	);
	expect(onAgain).toHaveBeenCalledTimes(1);
	expect(host.querySelector('[role="status"]')?.textContent).toContain('not there any more');

	unmount(component);
});

/** The list read again after one went may hold nothing at all. That is said
 * under the sentence about the one that went, and there is nothing to press. */
it('says when no backups are left', () => {
	const component = list([]);

	expect(reads()).toContain('No backups are left.');
	expect(host.querySelector('button')).toBeNull();

	unmount(component);
});

it('says any other refusal in Rust’s words, and reads nothing again', async () => {
	const onOpen = vi.fn().mockRejectedValue({ code: 'io', message: 'the disk went away' });
	const onAgain = vi.fn();
	const component = list(chain(), { onOpen, onAgain });

	rows()[0].querySelector('button')?.click();
	await vi.waitFor(() => expect(reads()).toContain('the disk went away'));
	expect(onAgain).not.toHaveBeenCalled();

	unmount(component);
});

/** The backup open now is drawn as open, not offered to be opened again. */
it('marks the one already open instead of offering it', () => {
	const component = list(chain(), { open: 'personal.kdbx.2.bak' });

	expect(rows()[1].querySelector('button')).toBeNull();
	expect(rows()[1].textContent).toContain('Open now');
	expect(rows()[0].querySelector('button')).not.toBeNull();

	unmount(component);
});

/** A backup's name is the vault's with a slot after it, and the vault's can
 * hold anything a file name can. It is drawn as the characters it is, and an
 * override in it stays inside its own row's name. */
it('draws a name holding markup or an override as the characters it is', () => {
	const component = list([
		{ name: '<img src=x onerror=alert(1)>.kdbx.1.bak', index: 1, taken: null },
		{ name: 'evil‮xcbdk.kdbx.2.bak', index: 2, taken: null }
	]);

	expect(host.querySelector('img')).toBeNull();
	const names = [...host.querySelectorAll('bdi')].map((each) => each.textContent);
	expect(names).toEqual(['<img src=x onerror=alert(1)>.kdbx.1.bak', 'evil‮xcbdk.kdbx.2.bak']);

	unmount(component);
});
