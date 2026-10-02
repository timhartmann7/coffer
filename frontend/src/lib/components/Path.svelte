<script lang="ts">
	/**
	 * The names of the folders down to something, written the way the mockup
	 * writes them above an entry's title: a dot between two.
	 *
	 * Each name is isolated on its own. A name from another client can hold a
	 * right-to-left override or an embedding it never closes, and one isolate
	 * around the whole line would let that turn round every name after it, so
	 * that one folder's path read as another's. The line above the title, plain
	 * or pressable, and the folders above each line of the folder list are all
	 * written here.
	 */
	let {
		names
	}: {
		/** Outermost first. */
		names: readonly string[];
	} = $props();

	/** Held in a name rather than written between the tags, where the
	 * compiler would trim the spaces off either side of the dot. */
	const BETWEEN = ' · ';
</script>

{#each names as name, at (at)}{#if at > 0}{BETWEEN}{/if}<bdi>{name}</bdi>{/each}
