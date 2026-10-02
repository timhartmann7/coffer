<script lang="ts">
	import Icon from './Icon.svelte';

	/**
	 * The warning every screen that takes a new master password gives, at full
	 * size and in plain words: a forgotten password is the end of the data.
	 */
	let { class: classes = '' }: { class?: string } = $props();
</script>

<div class="rounded-sm border border-warn/35 bg-warnwash p-5 {classes}">
	<div class="flex gap-3">
		<Icon name="warn" class="mt-0.5 h-5 w-5 shrink-0 text-warn" />
		<div>
			<p class="text-lead font-medium text-txt">The password cannot be recovered.</p>
			<p class="mt-2 max-w-[52ch] text-body leading-relaxed text-txt2">
				It is not kept anywhere and it is not sent anywhere. If it is forgotten the vault becomes a
				file of random bytes, and nobody can get the data back - not us, not anybody.
			</p>
		</div>
	</div>
</div>
