<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type CatalogMemory, type MemoryCatalog } from '#lib/api';
	import { rankCatalogMemories } from '#lib/local-search';
	import SiteGlyph, { glyphFor } from '#lib/components/SiteGlyph.svelte';

	let memories = $state<CatalogMemory[]>([]);
	let query = $state('');
	let category = $state('all');
	let loading = $state(true);
	let error = $state('');

	const categories = ['all', 'compose-draft', 'navigate', 'research'];
	const filtered = $derived(rankCatalogMemories(memories, query, category));

	function taskLabel(value: string) {
		return ({
			'compose-draft': 'Draft writing',
			navigate: 'Navigation',
		research: 'Research'
		} as Record<string, string>)[value] ?? value;
	}

	function statusLabel(value: string) {
		return ({
			active: 'Available',
			'needs-reverify': 'Needs review',
			superseded: 'Superseded',
			retired: 'Archived'
		} as Record<string, string>)[value] ?? value;
	}

	async function loadCatalog() {
		loading = true;
		error = '';
		try {
			const result = await api<MemoryCatalog>('/v1/memories');
			memories = result.memories;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not load shared workflows. Check the connection and try Refresh.';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		void loadCatalog();
	});
</script>

<svelte:head>
	<title>Shared workflows — Web Agentic Memory</title>
	<meta name="description" content="Search the public catalog of approved workflow memories." />
</svelte:head>

<section class="page-shell">
	<div class="container">
		<div class="page-heading">
			<span class="eyebrow">Reference catalog</span>
			<h1>Shared workflows.<br />Ready for your agent.</h1>
			<p>These are the verified hints your agent can recall through WAM. Browse them to inspect a workflow, or connect your agent to use them as you work. Search stays on this device.</p>
			<a class="button button-dark button-small" href="/guide">Connect your agent <span aria-hidden="true">↗</span></a>
		</div>

		<div class="toolbar">
			<label class="search-field">
				<span class="sr-only">Search memories</span>
				<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><circle cx="10.8" cy="10.8" r="6.4" stroke="currentColor" stroke-width="1.7" /><path d="m15.5 15.5 4.2 4.2" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" /></svg>
				<input type="search" bind:value={query} placeholder="Search a site or task" />
			</label>
			<button class="button button-light button-small" type="button" onclick={loadCatalog} disabled={loading}>{loading ? 'Loading…' : 'Refresh'}</button>
		</div>
		<div class="filter-tabs" aria-label="Filter by task">
			{#each categories as item}
				<button class:active={category === item} aria-pressed={category === item} class="filter-tab" type="button" onclick={() => (category = item)}>{item === 'all' ? 'All tasks' : taskLabel(item)}</button>
			{/each}
		</div>

		{#if error}
			<div class="notice-box" role="alert">{error}</div>
		{:else if loading}
			<div class="empty-state soft-card" role="status"><h2>Loading public memories…</h2></div>
		{:else}
			<div class="results-line"><span><strong>{filtered.length}</strong> workflow{filtered.length === 1 ? '' : 's'}</span><span>Public catalog</span></div>
			{#if !memories.length}
				<div class="empty-state soft-card">
					<span class="empty-rule" aria-hidden="true"></span>
					<h2>No public memories yet.</h2>
					<p>Contribute a workflow and follow its review status.</p>
					<a class="button button-light" href="/contribute">How to contribute <span aria-hidden="true">↗</span></a>
				</div>
			{:else if filtered.length}
				<div class="memory-grid">
					{#each filtered as memory (memory.memory_id)}
						<a class="memory-card" href={'/memories/' + encodeURIComponent(memory.memory_id)}>
							<div class="memory-card-top">
								<span class="site-icon"><SiteGlyph kind={glyphFor(memory.domain, memory.task_category)} /></span>
								<span class="pill" class:pill-green={memory.status === 'active'} class:pill-coral={memory.status === 'needs-reverify'}>{statusLabel(memory.status)}</span>
							</div>
							<h3>{memory.title}</h3>
							<p>{memory.summary}</p>
							<div class="memory-card-meta"><span>{memory.domain} · {taskLabel(memory.task_category)}</span><span>Verified {memory.verified_date}</span></div>
						</a>
					{/each}
				</div>
			{:else}
				<div class="empty-state soft-card">
					<span class="empty-icon" aria-hidden="true">⌕</span>
					<h2>No matching memories</h2>
					<p>Try another site, task, title, or description.</p>
					<button class="button button-light" type="button" onclick={() => { query = ''; category = 'all'; }}>Clear filters</button>
				</div>
			{/if}
		{/if}
	</div>
</section>
