/** Walking the group tree the way the two panes read it. */

import type { EntryRow, Group } from './model';

/** Every entry in this group and in every group under it, in tree order. */
export function entriesOf(group: Group): EntryRow[] {
	const found = [...group.entries];
	for (const section of group.sections) {
		found.push(...entriesOf(section));
	}
	return found;
}

/**
 * Everything the vault holds except what was deleted.
 *
 * The recycle bin is a group like any other in the file, and the tree shows it
 * apart from the rest, so "all entries" has to mean all the entries that are
 * not in it.
 */
export function liveEntries(root: Group): EntryRow[] {
	if (root.isRecycleBin) return [];
	const found = [...root.entries];
	for (const section of root.sections) {
		found.push(...liveEntries(section));
	}
	return found;
}

export function recycleBin(root: Group): Group | null {
	if (root.isRecycleBin) return root;
	for (const section of root.sections) {
		const found = recycleBin(section);
		if (found) return found;
	}
	return null;
}

/** The groups from the root down to `id`, or `null` when there is no such
 * group. The entry screen writes the tail of it above the title. */
export function pathTo(root: Group, id: string): Group[] | null {
	if (root.id === id) return [root];
	for (const section of root.sections) {
		const below = pathTo(section, id);
		if (below) return [root, ...below];
	}
	return null;
}

/** The projects of a vault: the groups the root holds, without the recycle
 * bin, which the tree draws on its own at the bottom. */
export function projects(root: Group): Group[] {
	return root.sections.filter((section) => !section.isRecycleBin);
}
