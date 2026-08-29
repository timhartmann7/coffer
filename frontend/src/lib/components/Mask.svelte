<script lang="ts">
	/**
	 * What a secret looks like when it is not being shown: ten squares, always
	 * ten, whatever the value is. Neither the length nor the shape of the value
	 * is on the screen, because neither of them is in the window at all.
	 */
	let { class: classes = 'text-txt4' }: { class?: string } = $props();
</script>

<svg class="h-[8px] w-[107px] {classes}" aria-hidden="true"><use href="#redact" /></svg>
