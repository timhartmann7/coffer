import { describe, expect, it } from 'vitest';
import { deleted, standing } from './bin';
import { group } from './fixtures';

/** The suite runs with TZ pinned to UTC. */
const now = new Date('2026-08-29T14:30:00Z');

const personal = group({ name: 'Personal' });
const root = group({ name: 'Passwords', sections: [personal] });

describe('what the bin says about what is in it', () => {
	it('says since when and which folder it was in', () => {
		const binned = { since: '2026-08-27T10:00:00Z', within: null, from: personal.id };
		expect(standing(binned, root, now)).toBe('In the Recycle Bin since 27 Aug · was in “Personal”');
		expect(deleted(binned, root, now)).toBe('Deleted 2 days ago · from “Personal”');
	});

	/** The top group has a name in the file that the window never shows. */
	it('calls the top of the vault the top of the vault, not by its name', () => {
		const binned = { since: '2026-08-29T10:00:00Z', within: null, from: root.id };
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
		const unknown = { since: null, within: null, from: null };
		expect(standing(unknown, root, now)).toBe(
			'In the Recycle Bin · goes back to the top of the vault'
		);
		expect(deleted(unknown, root, now)).toBe('Deleted');

		// An id the tree does not hold is not a folder to name.
		const stale = { since: '2026-08-28T10:00:00Z', within: null, from: 'not a folder' };
		expect(standing(stale, root, now)).toBe(
			'In the Recycle Bin since 28 Aug · goes back to the top of the vault'
		);
		expect(deleted(stale, root, now)).toBe('Deleted yesterday');
	});

	/**
	 * Something that went in with a deleted folder says so, and goes back where
	 * the folder came from. It is never said to come from the folder its own
	 * last move left, which the deletion did not touch.
	 */
	it('says which folder something went in with, and where it goes back', () => {
		const banking = group({ name: 'Banking' });
		const bin = group({ name: 'Recycle Bin', isRecycleBin: true, sections: [banking] });
		const tree = group({ ...root, sections: [personal, bin] });

		const filed = { since: '2026-08-27T10:00:00Z', within: banking.id, from: personal.id };
		expect(deleted(filed, tree, now)).toBe('Deleted 2 days ago · with “Banking”');
		expect(standing(filed, tree, now)).toBe(
			'In the Recycle Bin since 27 Aug · deleted with “Banking” · goes back to “Personal”'
		);

		const homeless = { since: null, within: banking.id, from: null };
		expect(deleted(homeless, tree, now)).toBe('Deleted with “Banking”');
		expect(standing(homeless, tree, now)).toBe(
			'In the Recycle Bin · deleted with “Banking” · goes back to the top of the vault'
		);
		expect(standing({ ...homeless, from: tree.id }, tree, now)).toBe(
			'In the Recycle Bin · deleted with “Banking” · goes back to the top of the vault'
		);
	});

	/** The folder is named by the tree as it is now, so one renamed since the
	 * deletion is called what the reader would look for. */
	it('names the folder as it is called now', () => {
		const renamed = group({ ...personal, name: 'Home' });
		const tree = group({ ...root, sections: [renamed] });
		expect(deleted({ since: null, within: null, from: personal.id }, tree, now)).toBe(
			'Deleted from “Home”'
		);
	});
});
