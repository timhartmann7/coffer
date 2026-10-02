/**
 * Entries and groups the way the commands hand them over, for the tests.
 *
 * Nothing in the application imports this. The shapes it builds are the ones
 * pinned on the Rust side by `dto.rs`, so a change there that this file does
 * not follow shows up as a test that no longer describes reality.
 */

import type {
	Attachment,
	Entry,
	EntryRow,
	Field,
	Generated,
	Generator,
	Group,
	Version
} from './model';

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
		binned: null,
		deletion: 'bin',
		...over
	};
}

export function group(over: Partial<Group> = {}): Group {
	return {
		id: id(),
		name: 'a group',
		isRecycleBin: false,
		binned: null,
		deletion: 'bin',
		sections: [],
		entries: [],
		...over
	};
}

export function field(over: Partial<Field> = {}): Field {
	return {
		name: 'Title',
		kind: 'title',
		protected: false,
		value: '',
		empty: true,
		openable: false,
		lines: false,
		...over
	};
}

export function attachment(over: Partial<Attachment> = {}): Attachment {
	return {
		name: 'id_ed25519',
		size: 411,
		fileName: 'id_ed25519',
		...over
	};
}

export function version(over: Partial<Version> = {}): Version {
	return {
		index: 0,
		modified: '2026-03-12T18:42:00Z',
		...over
	};
}

export function entry(over: Partial<Entry> = {}): Entry {
	return {
		id: id(),
		group: 'group',
		versions: 0,
		fields: [],
		attachments: [],
		tags: [],
		created: null,
		modified: null,
		binned: null,
		deletion: 'bin',
		...over
	};
}

/** The generator as Rust draws it for the recipe it opens with when nothing
 * has been remembered: twenty-four letters and digits, no look-alikes. */
export function drawing(over: Partial<Generator> = {}): Generator {
	return {
		recipe: { length: 24, alphabets: ['lower', 'upper', 'digits'], similar: false, avoid: '' },
		shortest: 8,
		longest: 64,
		pin: false,
		symbols: '!"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~',
		lookAlikes: '0O1lI|',
		...over
	};
}

/** A password Rust made, lacking nothing it was asked for. */
export function generated(value: string, over: Partial<Generated> = {}): Generated {
	return { value, missing: [], generator: drawing(), ...over };
}
