<script lang="ts">
	import InlineBio from './InlineBio.svelte';
	import type { BioBlock } from '../api';

	let { blocks }: { blocks: BioBlock[] } = $props();
</script>

<div class="bio-view">
	{#each blocks as block}
		{#if block.type === 'heading' && block.level === 2}
			<h2><InlineBio items={block.content} /></h2>
		{:else if block.type === 'heading'}
			<h3><InlineBio items={block.content} /></h3>
		{:else if block.type === 'paragraph'}
			<p><InlineBio items={block.content} /></p>
		{:else if block.type === 'bullet_list'}
			<ul>{#each block.items as item}<li><InlineBio items={item} /></li>{/each}</ul>
		{:else}
			<ol>{#each block.items as item}<li><InlineBio items={item} /></li>{/each}</ol>
		{/if}
	{/each}
</div>

<style>
	.bio-view { color: #526259; line-height: 1.8; overflow-wrap: anywhere; }
	.bio-view :global(h2), .bio-view :global(h3) { color: #18312a; margin: 20px 0 8px; }
	.bio-view :global(p), .bio-view :global(ul), .bio-view :global(ol) { margin: 10px 0; }
	.bio-view :global(a) { color: #315d4a; text-decoration: underline; text-underline-offset: 3px; }
</style>
