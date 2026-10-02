import { describe, expect, it } from 'vitest';
import { entry, field, row } from './fixtures';
import {
	ago,
	at,
	called,
	copied,
	counted,
	day,
	fully,
	isolated,
	minute,
	quoted,
	size,
	when
} from './format';

/** The suite runs with TZ pinned to UTC, so the local clock the screen writes
 * in is the same one the database keeps. */
const now = new Date('2026-08-29T14:30:00Z');

describe('a date as the list writes it', () => {
	it('gives the time for today and the day for anything older', () => {
		expect(when('2026-08-29T13:08:00Z', now)).toBe('13:08');
		expect(when('2026-08-29T09:05:00Z', now)).toBe('9:05');
		expect(when('2026-07-01T10:00:00Z', now)).toBe('1 Jul');
		expect(when('2024-03-04T10:00:00Z', now)).toBe('4 Mar 2024');
	});

	it('writes nothing for an entry with no date and for a date it cannot read', () => {
		expect(when(null, now)).toBe('');
		expect(when('', now)).toBe('');
		expect(when('not a date', now)).toBe('');
		expect(when('0000-00-00T00:00:00Z', now)).toBe('');
	});

	/** KDBX can hold dates centuries either side of now, and the fixture does. */
	it('writes the years at the edges of what the format can hold', () => {
		expect(when('1600-01-01T00:00:00Z', now)).toBe('1 Jan 1600');
		expect(when('3000-12-31T23:59:59Z', now)).toBe('31 Dec 3000');
	});

	it('says "today" on the entry screen and the full date everywhere else', () => {
		expect(fully('2026-08-29T13:08:00Z', now)).toBe('today at 13:08');
		expect(fully('2024-03-12T18:42:00Z', now)).toBe('12 Mar 2024');
		// The year even for this one, which the list and a sentence leave out.
		expect(fully('2026-03-12T18:42:00Z', now)).toBe('12 Mar 2026');
		expect(fully(null, now)).toBe('unknown');
	});
});

describe('a moment two files are compared by', () => {
	/** A copy and the vault it was taken from are usually hours apart on the
	 * same day, so the minute is always there and the day is said as a day. */
	it('always gives the minute, and the day the way a sentence says it', () => {
		expect(at('2026-08-29T14:05:00Z', now)).toBe('today at 14:05');
		expect(at('2026-08-28T09:07:00Z', now)).toBe('on 28 Aug at 9:07');
		expect(at('2025-12-31T23:59:00Z', now)).toBe('on 31 Dec 2025 at 23:59');
		expect(at('3000-01-01T00:00:00Z', now)).toBe('on 1 Jan 3000 at 0:00');
	});

	it('writes nothing for a time the filesystem did not keep', () => {
		expect(at(null, now)).toBe('');
		expect(at('not a date', now)).toBe('');
	});

	/** Ten backups are often ten saves within the hour, so a row in their list
	 * always has the minute, and is told from its neighbours by nothing else. */
	it('gives a row in a list of files the minute, after the day', () => {
		expect(minute('2026-08-29T14:05:00Z', now)).toBe('today, 14:05');
		expect(minute('2026-08-29T14:04:00Z', now)).toBe('today, 14:04');
		expect(minute('2026-08-27T18:40:00Z', now)).toBe('27 Aug, 18:40');
		expect(minute('2025-08-27T18:40:00Z', now)).toBe('27 Aug 2025, 18:40');
		expect(minute('3000-01-01T00:00:00Z', now)).toBe('1 Jan 3000, 0:00');
		expect(minute(null, now)).toBe('');
		expect(minute('', now)).toBe('');
		expect(minute('not a date', now)).toBe('');
	});
});

describe('an attachment size', () => {
	it('keeps to three or four digits', () => {
		expect(size(0)).toBe('0 B');
		expect(size(411)).toBe('411 B');
		expect(size(1023)).toBe('1023 B');
		expect(size(1024)).toBe('1.0 KB');
		expect(size(1536)).toBe('1.5 KB');
		expect(size(100 * 1024)).toBe('100 KB');
		expect(size(3 * 1024 * 1024)).toBe('3.0 MB');
		expect(size(100 * 1024 * 1024)).toBe('100 MB');
		expect(size(2 * 1024 * 1024 * 1024)).toBe('2.0 GB');
	});
});

describe('a day as a sentence says it', () => {
	it('says today, the day this year, and the year as well for any other', () => {
		expect(day('2026-08-29T01:00:00Z', now)).toBe('today');
		expect(day('2026-08-27T10:00:00Z', now)).toBe('27 Aug');
		expect(day('2025-09-30T10:00:00Z', now)).toBe('30 Sep 2025');
		expect(day(null, now)).toBe('');
		expect(day('not a date', now)).toBe('');
	});

	/** Counted between midnights: an evening is yesterday the next morning,
	 * however few hours ago it was. */
	it('counts days back the way a person does', () => {
		const morning = new Date('2026-08-29T00:10:00Z');
		expect(ago('2026-08-28T23:50:00Z', morning)).toBe('yesterday');
		expect(ago('2026-08-29T00:00:00Z', morning)).toBe('today');
		expect(ago('2026-08-26T09:00:00Z', now)).toBe('3 days ago');
		expect(ago('2026-08-23T09:00:00Z', now)).toBe('6 days ago');
		expect(ago('2026-08-22T09:00:00Z', now)).toBe('on 22 Aug');
		expect(ago('2024-03-04T10:00:00Z', now)).toBe('on 4 Mar 2024');
		expect(ago(null, now)).toBe('');
	});

	/** A date after today is somebody else's clock, and the edges of what the
	 * format holds are dates like any other. Neither is counted into nonsense. */
	it('writes a date in the future or at the edge of the format as the day', () => {
		expect(ago('2026-08-30T09:00:00Z', now)).toBe('on 30 Aug');
		expect(ago('3000-12-31T23:59:59Z', now)).toBe('on 31 Dec 3000');
		expect(ago('1600-01-01T00:00:00Z', now)).toBe('on 1 Jan 1600');
		expect(ago('0050-06-01T00:00:00Z', now)).toBe('on 1 Jun 50');
	});
});

describe('what a sentence calls an entry', () => {
	const titled = (value: string | null) =>
		entry({ fields: [field({ name: 'Title', kind: 'title', value, empty: value === '' })] });

	it('quotes the title, and says "this entry" when there is none to show', () => {
		expect(called(titled('Bank'))).toBe('“\u2068Bank\u2069”');
		expect(called(titled(''))).toBe('this entry');
		// Protected: the value is not here, and a notice is no reason to fetch it.
		expect(called(titled(null))).toBe('this entry');
		expect(called(entry({ fields: [] }))).toBe('this entry');
	});

	/** A row says the same as the entry it opens: the notice of a move made
	 * from the list must not name an entry differently from one made from the
	 * pane. */
	it('calls one row what it calls the entry', () => {
		expect(counted([row({ title: 'Bank' })])).toBe(called(titled('Bank')));
		expect(counted([row({ title: '' })])).toBe('this entry');
		expect(counted([row({ title: null })])).toBe('this entry');
		expect(counted([row({ title: 'evil\u202Eslip' })])).toBe('“\u2068evil\u202Eslip\u2069”');
	});

	/** Any other number is how many: a list of twelve titles is not a notice,
	 * and none of them is set apart from the rest. */
	it('counts any other number of rows, titles or none', () => {
		expect(counted([row({ title: 'Bank' }), row({ title: null })])).toBe('2 entries');
		expect(counted(Array.from({ length: 50_000 }, () => row()))).toBe('50000 entries');
		expect(counted([])).toBe('0 entries');
	});
});

/** What UAX #9 counts as the end of a paragraph, written out again rather
 * than taken from the code under test. */
const SEPARATORS = ['\n', '\r', '\u001C', '\u001D', '\u001E', '\u0085', '\u2029'];
const OVERRIDES = /[\u202A-\u202E]/u;

/**
 * What a mark opened inside a name can reach, by the rules of UAX #9: an
 * override or embedding lasts until the isolate around it closes, a closing
 * mark with nothing open is ignored, and anything left open lasts to the
 * end of the paragraph, where a paragraph separator closes every isolate
 * there is. Answers with the text outside every isolate, and fails when
 * the sentence ends with one still open or an override stands outside them
 * all.
 */
function outside(sentence: string): string {
	let depth = 0;
	let left = '';
	for (const char of sentence) {
		if (['\u2066', '\u2067', '\u2068'].includes(char)) depth += 1;
		else if (char === '\u2069') depth = Math.max(0, depth - 1);
		else if (SEPARATORS.includes(char)) depth = 0;
		else if (depth === 0) {
			expect(char, `an override reaches ${JSON.stringify(sentence)}`).not.toMatch(OVERRIDES);
			left += char;
		}
	}
	expect(depth, 'an isolate runs on past the sentence').toBe(0);
	return left;
}

describe('a name in running text', () => {
	it('keeps a right-to-left override in a name off the rest of the sentence', () => {
		const sentence = `This entry already has ${quoted('evil‮fdp.exe')} (1.2 MB). Keep both to add the new one (840 KB) as ${quoted('evil‮fdp 2.exe')}. A replaced file can’t be brought back.`;

		expect(quoted('evil‮fdp.exe')).toBe('“⁨evil‮fdp.exe⁩”');
		expect(outside(sentence)).toBe(
			'This entry already has “” (1.2 MB). Keep both to add the new one (840 KB) as “”. A replaced file can’t be brought back.'
		);
	});

	it('does not let a name close the isolate early, or leave one of its own open', () => {
		for (const name of ['a⁩‮b', '⁩⁩‭c', '⁧‮open', '⁦⁨‮nested', '⁧ok⁩⁩‮']) {
			expect(outside(`Delete ${quoted(name)} forever? This can’t be undone.`), name).toBe(
				'Delete “” forever? This can’t be undone.'
			);
		}
	});

	/** A paragraph separator ends every isolate along with the paragraph, so an
	 * override after one in a name from another client ran on through the rest
	 * of the question. */
	it('does not let a paragraph separator in a name end the isolate', () => {
		for (const separator of SEPARATORS) {
			const name = `a${separator}\u202Efdp.exe`;
			expect(
				outside(`This entry already has ${quoted(name)} (1.2 MB). Keep both?`),
				JSON.stringify(separator)
			).toBe('This entry already has \u201C\u201D (1.2 MB). Keep both?');
			expect(quoted(name)).toBe('\u201C\u2068a \u202Efdp.exe\u2069\u201D');
		}
	});

	it('leaves a name that needs nothing as it was, between the marks', () => {
		expect(quoted('Bank')).toBe('“⁨Bank⁩”');
		expect(quoted('בנק ⁧x⁩')).toBe('“⁨בנק ⁧x⁩⁩”');
		expect(quoted('')).toBe('“⁨⁩”');
	});

	/** A line of a menu is as wide as its longest line, and a folder's name can
	 * be a megabyte. It is cut, by characters rather than by halves of one, and
	 * what it opened before the cut is closed after it all the same. */
	it('cuts a name where it stands among words of its own, and still closes it', () => {
		expect(isolated('x'.repeat(200), 60)).toBe(`\u2068${'x'.repeat(60)}…\u2069`);
		expect(isolated('x'.repeat(60), 60), 'exactly as long').toBe(`\u2068${'x'.repeat(60)}\u2069`);
		expect(isolated('🔐'.repeat(70), 60)).toBe(`\u2068${'🔐'.repeat(60)}…\u2069`);
		expect(isolated('x'.repeat(1 << 20), 60)).toHaveLength(63);
		for (const name of [`\u2067${'y'.repeat(100)}`, `${'z'.repeat(59)}\u202E${'w'.repeat(9)}`]) {
			expect(outside(`Move to ${isolated(name, 60)} now`), name.slice(0, 3)).toBe('Move to  now');
		}
		expect(isolated('a\nb', 60)).toBe('\u2068a b\u2069');
		expect(quoted('Bank')).toBe(`“${isolated('Bank')}”`);
	});
});

describe('what a notice says was copied', () => {
	/** A name of the reader's own as the notice puts it, written out again
	 * rather than taken from the code under test. */
	const own = (name: string) => `\u201C\u2068${name}\u2069\u201D`;

	/** The five fields every entry has, by their names in the file, and what
	 * the pane calls each one. */
	const STANDARD: [string, string][] = [
		['Title', 'title'],
		['UserName', 'login'],
		['Password', 'password'],
		['URL', 'address'],
		['Notes', 'notes']
	];

	it('calls each standard field by the word the pane uses for it', () => {
		expect(STANDARD.map(([name]) => copied(name, false))).toEqual([
			'Title copied.',
			'Login copied.',
			'Password copied.',
			'Address copied.',
			'Notes copied.'
		]);
	});

	it('says only part of a standard field went when a selection was copied', () => {
		expect(STANDARD.map(([name]) => copied(name, true))).toEqual([
			'Part of the title copied.',
			'Part of the login copied.',
			'Part of the password copied.',
			'Part of the address copied.',
			'Part of the notes copied.'
		]);
	});

	/** The login a second before the password used to be announced in the
	 * same words, and the reader pasted an email address into a password box.
	 * No two fields may be told apart by nothing. */
	it('never says the same thing about two different fields', () => {
		const names = [...STANDARD.map(([name]) => name), 'PIN', 'pin', 'Login', 'login', 'API token'];
		for (const part of [false, true]) {
			const said = names.map((name) => copied(name, part));
			expect(new Set(said).size, said.join(' | ')).toBe(names.length);
		}
	});

	it('quotes a field of the reader’s own by its name', () => {
		expect(copied('PIN', false)).toBe(`${own('PIN')} copied.`);
		expect(copied('API token', false)).toBe(`${own('API token')} copied.`);
		expect(copied('PIN', true)).toBe(`Part of ${own('PIN')} copied.`);
	});

	/**
	 * A field of the reader's own may be called anything, including a standard
	 * field's name in another case, the word the pane uses for one, or that
	 * name with a space or a look-alike letter in it. None of them is the
	 * standard field, and a notice that said "Password copied." for a field
	 * called "password" told the reader the entry's password was on the
	 * pasteboard when something else was.
	 */
	it('does not take a field of the reader’s own for a standard one it is spelled like', () => {
		const lookalikes = [
			'password',
			'PASSWORD',
			'url',
			'Url',
			'username',
			'userName',
			'title',
			'notes',
			'Login',
			'login',
			'Address',
			'Password ',
			' Password',
			'Pass\u200Bword',
			'Passw\u03BFrd',
			'Pa\u0301ssword'
		];
		for (const name of lookalikes) {
			expect(copied(name, false), JSON.stringify(name)).toBe(`${own(name)} copied.`);
			expect(copied(name, true), JSON.stringify(name)).toBe(`Part of ${own(name)} copied.`);
		}
	});

	/** The standard names are looked up by the field's name, and a lookup that
	 * walked up to what every object inherits found a function for
	 * "constructor" and a whole object for "__proto__". */
	it('quotes a field named after a property every object has, and does not break on it', () => {
		const inherited = [
			'__proto__',
			'constructor',
			'toString',
			'hasOwnProperty',
			'valueOf',
			'isPrototypeOf',
			'propertyIsEnumerable',
			'toLocaleString',
			'__defineGetter__',
			'__lookupSetter__'
		];
		for (const name of inherited) {
			expect(copied(name, false), name).toBe(`${own(name)} copied.`);
			expect(copied(name, true), name).toBe(`Part of ${own(name)} copied.`);
		}
	});

	it('keeps an override in a field’s name off the rest of the notice', () => {
		for (const name of [
			'a\u202Eb',
			'\u2069\u202Eexe',
			'\u2067\u202Eopen',
			'x\n\u202Ey',
			'z\u2029\u202Dz'
		]) {
			for (const part of [false, true]) {
				const notice = `${copied(name, part)} The clipboard clears in 1 minute.`;
				expect(outside(notice), JSON.stringify(name)).toBe(
					`${part ? 'Part of ' : ''}\u201C\u201D copied. The clipboard clears in 1 minute.`
				);
			}
		}
	});

	/** A field another client saved with no name, or with a name the size of
	 * a document, is still a field a copy can be made from. */
	it('names a field with an empty or an enormous name without losing the sentence', () => {
		expect(copied('', false)).toBe(`${own('')} copied.`);
		expect(copied('', true)).toBe(`Part of ${own('')} copied.`);
		const huge = 'x'.repeat(1_000_000);
		expect(copied(huge, false)).toBe(`${own(huge)} copied.`);
	});
});
