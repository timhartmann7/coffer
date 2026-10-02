import { describe, expect, it, vi } from 'vitest';
import { adopted, adoptedBeforeLocking, refusal } from './replacing';

vi.mock(import('$lib/ipc'), async (real) => (await import('$lib/stubbed')).stubbed(await real()));

const database = { path: '/Users/someone/personal.kdbx', name: 'personal' };

describe('what a backup made the vault says about the file it replaced', () => {
	/** Each of the three things that can have become of the file is said, and
	 * its name with it, so the reader can find it in the Finder. */
	it('names where the file it replaced went, or says nothing of one that was not there', () => {
		expect(adopted({ database, keptAs: 'personal.kdbx.1.bak', setAside: null })).toBe(
			'This backup is your vault now. The file it replaced is kept as “⁨personal.kdbx.1.bak⁩”.'
		);
		expect(
			adopted({ database, keptAs: null, setAside: 'personal.kdbx.replaced-2026-10-01.kdbx' })
		).toBe(
			'This backup is your vault now. The file it replaced did not open with this password and is kept as “⁨personal.kdbx.replaced-2026-10-01.kdbx⁩”.'
		);
		expect(adopted({ database, keptAs: null, setAside: null })).toBe(
			'This backup is your vault now.'
		);
	});

	/** Said on the unlock screen when a lock took the window first, with the
	 * password the vault opens with now: the reader is about to type one. */
	it('says on the unlock screen what the notice would have, and which password opens it', () => {
		const lead =
			'A backup was made your vault before locking, and it opens with the password that backup opened with.';
		expect(adoptedBeforeLocking({ database, keptAs: 'personal.kdbx.1.bak', setAside: null })).toBe(
			`${lead} The file it replaced is kept as “⁨personal.kdbx.1.bak⁩”.`
		);
		expect(
			adoptedBeforeLocking({
				database,
				keptAs: null,
				setAside: 'personal.kdbx.replaced-2026-10-01.kdbx'
			})
		).toBe(
			`${lead} The file it replaced did not open with that password and is kept as “⁨personal.kdbx.replaced-2026-10-01.kdbx⁩”.`
		);
		expect(adoptedBeforeLocking({ database, keptAs: null, setAside: null })).toBe(lead);
	});

	/** A file name can hold a right-to-left override, which would turn the end
	 * of the sentence around. It is isolated where it stands. */
	it('keeps an override in a file name off the rest of the sentence', () => {
		const said = adopted({ database, keptAs: 'evil‮xcbdk.kdbx.1.bak', setAside: null });
		expect(said.endsWith('⁨evil‮xcbdk.kdbx.1.bak⁩”.')).toBe(true);
	});
});

describe('why a press that would have replaced the vault file did not', () => {
	/** Rust holds the press to how the strip said the vault's file stood. The
	 * window has read it again by the time this is said, so it points there. */
	it('words a vault file that changed after it was shown about the strip', () => {
		expect(
			refusal({
				code: 'externalChange',
				message: 'the vault file is not as it stood when it was shown'
			})
		).toBe(
			'Your vault file changed after this was shown, so nothing was replaced. What is said above is how it stands now.'
		);
	});

	it('says any other refusal in Rust’s words, and something for what is not one', () => {
		expect(
			refusal({ code: 'heldByAnother', message: 'someone has this vault open on a-mac' })
		).toBe('someone has this vault open on a-mac');
		expect(refusal(new Error('not a refusal'))).toBe('Coffer could not finish that.');
	});
});
