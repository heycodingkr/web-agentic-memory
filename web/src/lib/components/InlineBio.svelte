<script lang="ts">
	import { safeExternalUrl, type BioInline } from '../api';

	let { items }: { items: BioInline[] } = $props();
</script>

{#each items as item}
	{#if item.type === 'strong'}
		<strong>{item.text}</strong>
	{:else if item.type === 'emphasis'}
		<em>{item.text}</em>
	{:else if item.type === 'link' && safeExternalUrl(item.url)}
		<a href={item.url} target="_blank" rel="noopener noreferrer">{item.text}</a>
	{:else if item.type === 'link'}
		<span>{item.text}</span>
	{:else}
		{item.text}
	{/if}
{/each}
