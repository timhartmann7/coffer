<script lang="ts">
	/**
	 * The mark beside a field whose typed text has not been written yet.
	 *
	 * There is no save button, so a reader half way through a note has nothing
	 * to tell them the note is not in the vault until the field is left. A dot
	 * in the colour the status bar says "Not saved" in, the size of the window's
	 * smallest step; the words are there for anything that reads the screen
	 * aloud, and under the pointer.
	 */
	let { class: classes = '' }: { class?: string } = $props();
</script>

<span
	data-unsaved
	title="Not saved yet"
	class="h-1.5 w-1.5 shrink-0 rounded-full bg-warn {classes}"
>
	<span class="sr-only">Not saved yet</span>
</span>
