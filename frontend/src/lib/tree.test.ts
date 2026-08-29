import { describe, expect, it } from 'vitest';
import { group, row } from './fixtures';
import { entriesOf, liveEntries, pathTo, projects, recycleBin } from './tree';

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
