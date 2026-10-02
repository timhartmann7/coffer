import { describe, expect, it } from 'vitest';
import { moved } from './bin';
import { group, row } from './fixtures';
import type { Group } from './model';
import { destinations, movedTo, placeOf, placesFor, targets, TOP, TOP_LABEL } from './places';

/** A vault shaped to catch every way the list could be wrong: the bin inside a
 * folder, a folder inside the bin, and folders nested both ways. */
function vault() {
	const inBin = group({ name: 'Old', binned: { since: null, within: null, from: null } });
	const bin = group({ name: 'Recycle Bin', isRecycleBin: true, sections: [inBin] });
	const cards = group({ name: 'Cards' });
	const personal = group({ name: 'Personal', sections: [cards] });
	const archive = group({ name: 'Archive', sections: [bin] });
	const work = group({ name: 'Work' });
	const root = group({ name: 'Passwords', sections: [personal, archive, work] });
	return { root, personal, cards, archive, bin, inBin, work };
}

/** A chain of folders `depth` deep, and the deepest of them. */
function chain(depth: number): { root: Group; deepest: Group } {
	let deepest = group({ name: `level ${depth}` });
	let top = deepest;
	for (let level = depth - 1; level >= 1; level -= 1) {
		top = group({ name: `level ${level}`, sections: [top] });
	}
	if (depth === 1) deepest = top;
	return { root: group({ name: 'Root', sections: [top] }), deepest };
}

describe('where a thing can go', () => {
	it('lists every folder outside the bin in the tree’s order, the top first', () => {
		const { root, personal, cards, archive, work } = vault();

		expect(destinations(root)).toEqual([
			{ id: root.id, name: TOP_LABEL, parents: [] },
			{ id: personal.id, name: 'Personal', parents: [] },
			{ id: cards.id, name: 'Cards', parents: ['Personal'] },
			{ id: archive.id, name: 'Archive', parents: [] },
			{ id: work.id, name: 'Work', parents: [] }
		]);
	});

	/** A file whose top group is the bin holds nowhere a move may end. */
	it('offers nothing in a vault whose top is the bin', () => {
		expect(destinations(group({ isRecycleBin: true, sections: [group()] }))).toEqual([]);
	});

	it('walks a hundred folders deep without a call stack', () => {
		const { root, deepest } = chain(100);

		const listed = destinations(root);
		expect(listed).toHaveLength(101);
		const last = listed.at(-1);
		expect(last?.id).toBe(deepest.id);
		expect(last?.parents).toHaveLength(99);
		expect(last?.parents.at(0)).toBe('level 1');
	});

	it('a folder takes neither itself, nor anything under it, nor its own parent', () => {
		const { root, personal, cards, archive, work, bin, inBin } = vault();

		const forPersonal = targets(root, { folder: personal });
		expect([...forPersonal].sort()).toEqual([archive.id, work.id].sort());

		const forCards = targets(root, { folder: cards });
		expect(forCards.has(personal.id), 'the folder it is already in').toBe(false);
		expect(forCards.has(cards.id)).toBe(false);
		expect(forCards.has(root.id)).toBe(true);
		for (const binned of [bin.id, inBin.id]) expect(forCards.has(binned)).toBe(false);
	});

	it('a folder a hundred deep cannot go inside itself at any depth', () => {
		const { root } = chain(100);
		const top = root.sections[0];

		const places = targets(root, { folder: top });
		expect([...places], 'the only place outside it is the one it is in').toEqual([]);
	});

	it('entries can go anywhere but the one folder all of them are already in', () => {
		const { root, personal, work, bin } = vault();
		const here = row({ group: personal.id });
		const there = row({ group: work.id });

		const alone = targets(root, { entries: [here, row({ group: personal.id })] });
		expect(alone.has(personal.id)).toBe(false);
		expect(alone.has(work.id)).toBe(true);
		expect(alone.has(root.id)).toBe(true);

		// From two folders, each still has somewhere to go in the other.
		const mixed = targets(root, { entries: [here, there] });
		expect(mixed.has(personal.id)).toBe(true);
		expect(mixed.has(work.id)).toBe(true);
		expect(mixed.has(bin.id), 'the bin took a move').toBe(false);
	});
});

describe('a place in a sentence', () => {
	it('names a place in a sentence with its name isolated', () => {
		const sneaky = group({ name: 'evil‮slip' });
		const root = group({ name: 'Passwords', sections: [sneaky] });

		expect(placeOf(root, sneaky.id)).toBe('“⁨evil‮slip⁩”');
		expect(placeOf(root, root.id)).toBe(TOP);
		expect(placeOf(root, 'a folder that is not there')).toBeNull();
	});

	it('says a move the way the bin’s notice says one', () => {
		expect(movedTo('“⁨Chase⁩”', '“⁨Banking⁩”')).toBe('Moved “⁨Chase⁩” to “⁨Banking⁩”');
		expect(movedTo('3 entries', TOP)).toBe('Moved 3 entries to the top of the vault');
		expect(moved('“⁨Bank⁩”')).toBe('Moved “⁨Bank⁩” to the Recycle Bin');
	});
});

describe('where a menu under the pointer offers to move a thing', () => {
	/** The folder list's places, flat, each with how deep it sits and whether a
	 * move there is one the list would take: the menu draws exactly what the
	 * list offers. */
	it('offers the folder list’s places in its order, with how deep each sits', () => {
		const { root, personal, cards, archive, work } = vault();
		const rows = [row({ group: personal.id })];

		expect(placesFor(root, { entries: rows })).toEqual([
			{ id: root.id, name: TOP_LABEL, open: true, depth: 0 },
			{ id: personal.id, name: '\u2068Personal\u2069', open: false, depth: 0 },
			{ id: cards.id, name: '\u2068Cards\u2069', open: true, depth: 1 },
			{ id: archive.id, name: '\u2068Archive\u2069', open: true, depth: 0 },
			{ id: work.id, name: '\u2068Work\u2069', open: true, depth: 0 }
		]);
		for (const place of placesFor(root, { entries: rows }))
			expect(targets(root, { entries: rows }).has(place.id)).toBe(place.open);
	});

	/** A folder is offered where it is, greyed out, and never anything under
	 * it: moving it there is the one move no menu should show at all. Nothing
	 * in the bin is a place. */
	it('lists a folder itself, closed, and nothing under it', () => {
		const { root, personal, cards, archive, work, bin, inBin } = vault();

		const places = placesFor(root, { folder: personal });
		expect(places.map((place) => place.id)).toEqual([root.id, personal.id, archive.id, work.id]);
		expect(places.find((place) => place.id === personal.id)?.open).toBe(false);
		expect(places.find((place) => place.id === root.id)?.open, 'where it is now').toBe(false);
		for (const gone of [cards.id, bin.id, inBin.id])
			expect(places.some((place) => place.id === gone)).toBe(false);
	});

	/** A name that would take a menu over is cut there and set apart; the top
	 * of the vault is the window's own words. */
	it('cuts and sets apart a name the way a sentence does, and the top’s is its own', () => {
		const long = group({ name: `\u202E${'n'.repeat(500)}` });
		const root = group({ name: 'Passwords', sections: [long] });

		const [top, folder] = placesFor(root, { entries: [row({ group: root.id })] });
		expect(top.name).toBe(TOP_LABEL);
		expect(folder.name).toBe(`\u2068\u202E${'n'.repeat(59)}…\u2069`);
	});

	it('walks a hundred folders deep, each one deeper', () => {
		const { root, deepest } = chain(100);
		const places = placesFor(root, { entries: [row({ group: root.id })] });
		expect(places.map((place) => place.depth)).toEqual([
			0,
			...Array.from({ length: 100 }, (_, at) => at)
		]);
		expect(places.at(-1)?.id).toBe(deepest.id);
	});
});
