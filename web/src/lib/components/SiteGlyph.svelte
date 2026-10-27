<script lang="ts" module>
	export type GlyphKind = 'draft' | 'blog' | 'slides' | 'route' | 'search';

	// Pick a pictogram from what the memory covers. Unknown tasks fall back to the draft page.
	export function glyphFor(domain: string, taskCategory: string): GlyphKind {
		if (domain.includes('slides') || taskCategory === 'present') return 'slides';
		if (taskCategory === 'navigate') return 'route';
		if (taskCategory === 'research') return 'search';
		return 'draft';
	}
</script>

<script lang="ts">
	// Simple pictograms for the kinds of sites a memory covers. They are drawn for this project and do not
	// reproduce any third-party brand mark; swap in official artwork here if you have the right to use it.
	let { kind = 'draft' }: { kind?: GlyphKind } = $props();
</script>

<svg class="site-glyph" viewBox="0 0 36 36" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">
	{#if kind === 'draft'}
		<rect x="9" y="6" width="18" height="24" rx="3.5" />
		<path d="M13.5 12.5h9" stroke-width="2.7" />
		<path d="M13.5 17.5h9M13.5 21.5h9M13.5 25.5h5.5" />
	{:else if kind === 'blog'}
		<path d="M8 10.5A3.5 3.5 0 0 1 11.5 7h13a3.5 3.5 0 0 1 3.5 3.5v9a3.5 3.5 0 0 1-3.5 3.5H17.5l-5.5 4.6V23A3.5 3.5 0 0 1 8 19.5z" />
		<path d="M13 13h10M13 17.5h6" />
	{:else if kind === 'slides'}
		<rect x="6.5" y="8" width="23" height="15" rx="2.5" />
		<path d="M18 23v4.5M13 28.5h10" />
		<path d="m11.5 19 4-4 3 3 5-5.5" />
	{:else if kind === 'route'}
		<circle cx="11" cy="26" r="2.6" />
		<circle cx="25.5" cy="10" r="2.6" fill="currentColor" stroke="none" />
		<path d="M13.6 26c9.5 0 1.5-16 9.3-16" />
	{:else}
		<circle cx="16" cy="16" r="7" />
		<path d="m21.2 21.2 6.8 6.8" />
	{/if}
</svg>
