import { describe, expect, it } from 'vitest';
import { deleted, standing } from './bin';
import { group } from './fixtures';

/** The suite runs with TZ pinned to UTC. */
const now = new Date('2026-08-29T14:30:00Z');

const personal = group({ name: 'Personal' });
const root = group({ name: 'Passwords', sections: [personal] });

describe('what the bin says about what is in it', () => {
	it('says since when and which folder it was in', () => {
		const binned = { since: '2026-08-27T10:00:00Z', from: personal.id };
		expect(standing(binned, root, now)).toBe('In the Recycle Bin since 27 Aug · was in “Personal”');
		expect(deleted(binned, root, now)).toBe('Deleted 2 days ago · from “Personal”');
	});

	/** The top group has a name in the file that the window never shows. */
	it('calls the top of the vault the top of the vault, not by its name', () => {
		const binned = { since: '2026-08-29T10:00:00Z', from: root.id };
		expect(standing(binned, root, now)).toBe(
			'In the Recycle Bin since today · was at the top of the vault'
		);
		expect(deleted(binned, root, now)).toBe('Deleted today · from the top of the vault');
		expect(standing(binned, root, now)).not.toContain('Passwords');
	});

	/**
	 * Nothing known is not made up. A thing another client binned without a
	 * record, or whose folder has gone or is in the bin too, says where it goes
	 * rather than where it was.
	 */
	it('does not invent where something came from', () => {
		const unknown = { since: null, from: null };
		expect(standing(unknown, root, now)).toBe(
			'In the Recycle Bin · goes back to the top of the vault'
		);
		expect(deleted(unknown, root, now)).toBe('Deleted');

		// An id the tree does not hold is not a folder to name.
		const stale = { since: '2026-08-28T10:00:00Z', from: 'not a folder' };
		expect(standing(stale, root, now)).toBe(
			'In the Recycle Bin since 28 Aug · goes back to the top of the vault'
		);
		expect(deleted(stale, root, now)).toBe('Deleted yesterday');
	});

	/** The folder is named by the tree as it is now, so one renamed since the
	 * deletion is called what the reader would look for. */
	it('names the folder as it is called now', () => {
		const renamed = group({ ...personal, name: 'Home' });
		const tree = group({ ...root, sections: [renamed] });
		expect(deleted({ since: null, from: personal.id }, tree, now)).toBe('Deleted from “Home”');
	});
});
