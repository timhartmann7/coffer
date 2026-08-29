<script lang="ts">
	/** One of the symbols in {@link Sprite}, drawn in the current colour. */
	let { name, class: classes }: { name: string; class: string } = $props();
</script>

<svg class={classes} aria-hidden="true"><use href="#i-{name}" /></svg>
