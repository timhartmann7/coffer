/**
 * What the window calls the places a thing can be in, and where a move may
 * take it. Every way of moving asks here - the folder list, a drag, the undo's
 * sentence - so the rule and the names live once.
 */

import { quoted } from './format';
import type { EntryRow, Group } from './model';
import { find, inBin, pathTo, projects } from './tree';

/**
 * What a sentence calls the top of the vault. The file gives the top group a
 * name, and the window never shows it: the folders pane calls everything in
 * the vault "All entries", so a sentence that named the top group would be
 * naming a folder the reader has never seen.
 */
export const TOP = 'the top of the vault';

/** What a line or a list calls it. */
export const TOP_LABEL = 'Top of the vault';

/** A place a thing can go, as the folder list draws it. */
interface Destination {
	id: string;
	/** The folder's name, or the top of the vault's label. */
	name: string;
	/** The names of the folders above it, outermost first. */
	parents: string[];
}

/**
 * The top of the vault first, then every folder outside the recycle bin in the
 * tree's order, each with the names of the folders above it. Nothing in the
 * bin is a place to go: deleting is the way in.
 *
 * Walked with a stack, as `visible` is, so a hundred levels cannot overflow
 * anything.
 */
export function destinations(root: Group): Destination[] {
	if (inBin(root)) return [];
	const found: Destination[] = [{ id: root.id, name: TOP_LABEL, parents: [] }];
	const pending: { group: Group; parents: string[] }[] = projects(root)
		.map((group) => ({ group, parents: [] }))
		.reverse();

	for (let here = pending.pop(); here !== undefined; here = pending.pop()) {
		found.push({ id: here.group.id, name: here.group.name, parents: here.parents });
		const parents = [...here.parents, here.group.name];
		for (let at = here.group.sections.length - 1; at >= 0; at -= 1) {
			const section = here.group.sections[at];
			if (!section.isRecycleBin) pending.push({ group: section, parents });
		}
	}

	return found;
}

/**
 * A place as a sentence names it: the top of the vault, a folder's name in
 * quotes, or `null` when the tree does not hold it.
 *
 * Read off the tree, so a folder renamed since is called what it is called
 * now.
 */
export function placeOf(root: Group, id: string): string | null {
	if (id === root.id) return TOP;
	const folder = find(root, id);
	return folder ? quoted(folder.name) : null;
}

/** What is being moved: entries, as their rows were drawn, or one folder. */
export type Moving = { entries: EntryRow[] } | { folder: Group };

/**
 * The places `moving` may land: every destination but the one place everything
 * being moved already is, and for a folder neither itself nor anything under
 * it. Worked out once, when a drag lifts, rather than at every step of the
 * pointer.
 */
export function targets(root: Group, moving: Moving): Set<string> {
	const places = new Set(destinations(root).map((place) => place.id));

	if ('entries' in moving) {
		const folders = new Set(moving.entries.map((row) => row.group));
		if (folders.size === 1) for (const folder of folders) places.delete(folder);
		return places;
	}

	const parent = pathTo(root, moving.folder.id)?.at(-2);
	if (parent) places.delete(parent.id);
	const under = [moving.folder];
	for (let here = under.pop(); here !== undefined; here = under.pop()) {
		places.delete(here.id);
		for (const section of here.sections) under.push(section);
	}
	return places;
}

/** What a move says, the bin's included: "Moved “Chase” to “Banking”". `what`
 * and `place` are as a sentence names them, quotes and all. */
export function movedTo(what: string, place: string): string {
	return `Moved ${what} to ${place}`;
}
