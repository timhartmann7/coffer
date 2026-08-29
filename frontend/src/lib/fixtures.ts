/**
 * Entries and groups the way the commands hand them over, for the tests.
 *
 * Nothing in the application imports this. The shapes it builds are the ones
 * pinned on the Rust side by `dto.rs`, so a change there that this file does
 * not follow shows up as a test that no longer describes reality.
 */

import type { Entry, EntryRow, Field, Group } from './model';

let next = 0;

function id(): string {
	next += 1;
	return `00000000-0000-0000-0000-${String(next).padStart(12, '0')}`;
}

export function row(over: Partial<EntryRow> = {}): EntryRow {
	return {
		id: id(),
		group: 'group',
		title: 'an entry',
		username: '',
		url: '',
		tags: [],
		modified: null,
		hasPassword: false,
		attachments: 0,
		...over
	};
}

export function group(over: Partial<Group> = {}): Group {
	return {
		id: id(),
		name: 'a group',
		isRecycleBin: false,
		sections: [],
		entries: [],
		...over
	};
}

export function field(over: Partial<Field> = {}): Field {
	return {
		name: 'Title',
		kind: 'title',
		value: '',
		empty: true,
		openable: false,
		...over
	};
}

export function entry(over: Partial<Entry> = {}): Entry {
	return {
		id: id(),
		group: 'group',
		fields: [],
		attachments: [],
		tags: [],
		created: null,
		modified: null,
		...over
	};
}
