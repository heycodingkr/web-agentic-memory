<script lang="ts" module>
	export type GlyphKind = 'draft' | 'blog' | 'slides' | 'doc' | 'sheet' | 'mail' | 'form' | 'calendar' | 'folder' | 'route' | 'search' | 'design';

	// Pick a pictogram from what the memory covers. Unknown tasks fall back to the draft page.
	export function glyphFor(domain: string, taskCategory: string): GlyphKind {
		if (/^(www\.)?(figma\.com|canva\.com)$/.test(domain) || domain === 'stitch.withgoogle.com') return 'design';
		if (domain.includes('slides') || taskCategory === 'present') return 'slides';
		if (domain.startsWith('docs.') || taskCategory === 'document') return 'doc';
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
	{:else if kind === 'doc'}
		<path d="M10 5.5h10.5l6 6V30a1.5 1.5 0 0 1-1.5 1.5H10A1.5 1.5 0 0 1 8.5 30V7A1.5 1.5 0 0 1 10 5.5z" />
		<path d="M20.5 5.5V12h6" />
		<path d="M13 16.5h7" stroke-width="2.7" />
		<rect x="13" y="21" width="10" height="6.5" rx="1.2" />
		<path d="M18 21v6.5M13 24.2h10" />
	{:else if kind === 'sheet'}
		<rect x="7" y="8" width="22" height="20" rx="3" />
		<path d="M7 14.7h22M7 21.3h22M16.3 8v20" />
	{:else if kind === 'mail'}
		<rect x="6.5" y="9" width="23" height="18" rx="3" />
		<path d="m8 11.5 10 7.5 10-7.5" />
	{:else if kind === 'form'}
		<rect x="9" y="6" width="18" height="24" rx="3.5" />
		<rect x="13" y="11" width="3.6" height="3.6" rx="1" /><path d="M19 12.8h5" />
		<rect x="13" y="18" width="3.6" height="3.6" rx="1" /><path d="M19 19.8h5M13 25.5h6" />
	{:else if kind === 'calendar'}
		<rect x="7" y="9" width="22" height="20" rx="3" />
		<path d="M7 15.5h22M13 6.5v5M23 6.5v5" />
		<path d="M12.5 21h2.6M20.9 21h2.6M12.5 25h2.6" stroke-width="2.3" />
	{:else if kind === 'design'}
		<rect x="6.5" y="7.5" width="23" height="21" rx="3" />
		<path d="M6.5 13.5h23M13.5 13.5v15M18.5 19h6M18.5 23.5h3.5" />
	{:else if kind === 'folder'}
		<path d="M6.5 12.5a3 3 0 0 1 3-3h5.4l2.8 3h9.8a3 3 0 0 1 3 3v9a3 3 0 0 1-3 3h-18a3 3 0 0 1-3-3z" />
	{:else if kind === 'route'}
		<circle cx="11" cy="26" r="2.6" />
		<circle cx="25.5" cy="10" r="2.6" fill="currentColor" stroke="none" />
		<path d="M13.6 26c9.5 0 1.5-16 9.3-16" />
	{:else}
		<circle cx="16" cy="16" r="7" />
		<path d="m21.2 21.2 6.8 6.8" />
	{/if}
</svg>
