<script lang="ts">
	import '../app.css';
	import Sprite from '$lib/components/Sprite.svelte';
	import { plain } from '$lib/context.svelte';

	let { children } = $props();
</script>

<!-- WebKit's menu under the pointer is a web page's, and nothing in this window
     is one: every screen, the unlock and creation screens included, draws
     Coffer's menu where it knows what was clicked and none anywhere else. -->
<svelte:window oncontextmenu={plain} />

<Sprite />
{@render children()}
