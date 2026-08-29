<script lang="ts">
	import '../app.css';
	import Sprite from '$lib/components/Sprite.svelte';

	let { children } = $props();
</script>

<Sprite />
{@render children()}
