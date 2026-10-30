<script lang="ts">
	import { onDestroy } from 'svelte';
	let { label, code, disabled = false }: { label: string; code: string; disabled?: boolean } = $props();
	let copyState = $state<'idle' | 'copied' | 'error'>('idle');
	let copiedValue = $state('');
	let timer: ReturnType<typeof setTimeout> | undefined;
	const copied = $derived(copyState === 'copied' && copiedValue === code);
	async function copy() {
		const value = code;
		try {
			await navigator.clipboard.writeText(value);
			copiedValue = value;
			copyState = 'copied';
			clearTimeout(timer);
			timer = setTimeout(() => { copyState = 'idle'; }, 2000);
		} catch { copyState = 'error'; }
	}
	onDestroy(() => clearTimeout(timer));
</script>

<div class="code-panel">
	<div class="code-panel-head"><span>{label}</span><button class="copy-button" type="button" {disabled} aria-label={'Copy ' + label} onclick={copy}>{copied ? 'COPIED' : 'COPY'}</button></div>
	<pre><code>{code}</code></pre>
</div>
<span class="sr-only" role="status">{copied ? 'Command copied to clipboard.' : ''}</span>
{#if copyState === 'error'}<p class="field-help" role="status">Clipboard unavailable. Select the command above and copy it manually.</p>{/if}
