<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	let { label, code, disabled = false, korean = false }: { label: string; code: string; disabled?: boolean; korean?: boolean } = $props();
	let ready = $state(false);
	onMount(() => { ready = true; });
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
	<div class="code-panel-head"><span>{label}</span><button class="copy-button" type="button" disabled={disabled || !ready} aria-label={(korean ? '복사 ' : 'Copy ') + label} onclick={copy}>{copied ? (korean ? '복사됨' : 'COPIED') : (korean ? '복사' : 'COPY')}</button></div>
	<pre><code>{code}</code></pre>
</div>
<span class="sr-only" role="status">{copied ? (korean ? '클립보드에 복사했습니다.' : 'Command copied to clipboard.') : ''}</span>
{#if copyState === 'error'}<p class="field-help" role="status">{korean ? '클립보드를 사용할 수 없습니다. 위 내용을 선택해 직접 복사하세요.' : 'Clipboard unavailable. Select the command above and copy it manually.'}</p>{/if}
