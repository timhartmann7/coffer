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

/**
 * The entries a folder shows when it is the one being drawn.
 *
 * The recycle bin is a group like any other in the file and a client may put it
 * anywhere, including inside a project. Deleted entries belong to the bin's own
 * screen and to no other, so a project holding one does not show its contents -
 * unless the bin itself is the folder that was chosen.
 */
export function shownEntries(group: Group): EntryRow[] {
	return group.isRecycleBin ? entriesOf(group) : liveEntries(group);
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

/** A line of the tree pane. */
export interface Line {
	group: Group;
	/** How far in it sits, counting from nothing for a project. */
	depth: number;
	/** How many entries it holds, its own sections included. */
	entries: number;
}

/**
 * The lines the tree pane draws, top down, leaving out what nobody has opened.
 *
 * The walk is a stack rather than a recursion, and the pane draws one flat list
 * rather than a component inside a component inside a component. A file can
 * nest a group as deep as it likes, and a hundred of them must not take the
 * window down with a call stack.
 */
export function visible(root: Group, expanded: Set<string>): Line[] {
	const lines: Line[] = [];
	const pending: { group: Group; depth: number }[] = projects(root)
		.map((group) => ({ group, depth: 0 }))
		.reverse();

	for (let here = pending.pop(); here !== undefined; here = pending.pop()) {
		lines.push({ ...here, entries: shownEntries(here.group).length });

		if (!expanded.has(here.group.id)) continue;
		for (let at = here.group.sections.length - 1; at >= 0; at -= 1) {
			const section = here.group.sections[at];
			if (!section.isRecycleBin) pending.push({ group: section, depth: here.depth + 1 });
		}
	}

	return lines;
}
