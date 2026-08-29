import { describe, expect, it } from 'vitest';
import { group, row } from './fixtures';
import { entriesOf, liveEntries, pathTo, projects, recycleBin, visible } from './tree';

describe('walking the tree', () => {
	it('gathers the entries of a group and of everything under it', () => {
		const tree = group({
			entries: [row({ title: 'top' })],
			sections: [
				group({ entries: [row({ title: 'inside' })] }),
				group({ sections: [group({ entries: [row({ title: 'deeper' })] })] })
			]
		});

		expect(entriesOf(tree).map((entry) => entry.title)).toEqual(['top', 'inside', 'deeper']);
	});

	/** Deleted entries are in a group like any other, and "all entries" has to
	 * mean the ones that are not in it - including the ones in a group nested
	 * inside it. */
	it('leaves everything in the recycle bin out of the live entries', () => {
		const tree = group({
			entries: [row({ title: 'kept' })],
			sections: [
				group({
					isRecycleBin: true,
					entries: [row({ title: 'deleted' })],
					sections: [group({ entries: [row({ title: 'deleted deeper' })] })]
				})
			]
		});

		expect(liveEntries(tree).map((entry) => entry.title)).toEqual(['kept']);
		expect(entriesOf(tree)).toHaveLength(3);
	});

	it('finds the recycle bin wherever the file puts it', () => {
		const bin = group({ name: 'Recycle Bin', isRecycleBin: true });
		const tree = group({ sections: [group({ sections: [bin] })] });

		expect(recycleBin(tree)?.id).toBe(bin.id);
		expect(recycleBin(group())).toBeNull();
	});

	it('keeps the recycle bin out of the projects, which the tree draws apart', () => {
		const bin = group({ isRecycleBin: true });
		const project = group({ name: 'Work' });
		const tree = group({ sections: [project, bin] });

		expect(projects(tree).map((section) => section.name)).toEqual(['Work']);
	});

	it('gives the path down to a group, and nothing for one that is not there', () => {
		const deep = group({ name: 'Servers' });
		const tree = group({
			name: 'Root',
			sections: [group({ name: 'Infrastructure', sections: [deep] })]
		});

		expect(pathTo(tree, deep.id)?.map((step) => step.name)).toEqual([
			'Root',
			'Infrastructure',
			'Servers'
		]);
		expect(pathTo(tree, 'not a group')).toBeNull();
	});

	it('lists the tree top down, with what nobody opened left out', () => {
		const servers = group({ name: 'Servers', entries: [row()] });
		const work = group({ name: 'Work', entries: [row()], sections: [servers] });
		const personal = group({ name: 'Personal' });
		const tree = group({
			sections: [work, personal, group({ name: 'Recycle Bin', isRecycleBin: true })]
		});

		expect(
			visible(tree, new Set()).map((line) => [line.group.name, line.depth, line.entries])
		).toEqual([
			['Work', 0, 2],
			['Personal', 0, 0]
		]);

		expect(
			visible(tree, new Set([work.id])).map((line) => [line.group.name, line.depth, line.entries])
		).toEqual([
			['Work', 0, 2],
			['Servers', 1, 1],
			['Personal', 0, 0]
		]);
	});

	/** A group nested a hundred deep is a group, and the pane draws it without a
	 * recursion the browser can run out of. */
	it('walks a hundred open levels without recursing', () => {
		let deepest = group({ name: 'level 100' });
		for (let level = 99; level > 0; level -= 1) {
			deepest = group({ name: `level ${level}`, sections: [deepest] });
		}
		const tree = group({ sections: [deepest] });

		function every(root: ReturnType<typeof group>): string[] {
			return [root.id, ...root.sections.flatMap(every)];
		}

		const lines = visible(tree, new Set(every(tree)));
		expect(lines).toHaveLength(100);
		expect(lines[99].depth).toBe(99);
		expect(lines[99].group.name).toBe('level 100');
	});

	/** The attack list asks for a hundred levels of nesting. */
	it('walks a hundred levels deep', () => {
		let tree = group({ name: 'level 100', entries: [row({ title: 'bottom' })] });
		for (let level = 99; level > 0; level -= 1) {
			tree = group({ name: `level ${level}`, sections: [tree] });
		}

		expect(entriesOf(tree)).toHaveLength(1);
		expect(liveEntries(tree)).toHaveLength(1);
		expect(pathTo(tree, 'nothing')).toBeNull();
	});
});
