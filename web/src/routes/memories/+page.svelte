<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type CatalogMemory, type MemoryCatalog } from '#lib/api';
	import { rankCatalogMemories } from '#lib/local-search';
	import SiteGlyph from '#lib/components/SiteGlyph.svelte';
	import { SUGGESTIONS, TASKS, applyFilters, displayTitle, expandQuery, formatDate, isExample, libraryOrder, productCounts, productOf, taskLabel, type Source } from '#lib/memories';

	let memories = $state<CatalogMemory[]>([]);
	let query = $state('');
	let product = $state('all');
	let task = $state('all');
	let source = $state<Source>('all');
	let loading = $state(true);
	let error = $state('');

	const products = $derived(productCounts(memories));
	const exampleCount = $derived(memories.filter(isExample).length);
	// Filters narrow the library first, then the words rank what is left. The query never leaves the browser.
	const results = $derived(rankCatalogMemories(libraryOrder(applyFilters(memories, { query, product, task, source })), expandQuery(query), 'all'));
	const filtering = $derived(!!query.trim() || product !== 'all' || task !== 'all' || source !== 'all');

	const sources: { value: Source; label: string }[] = [
		{ value: 'all', label: 'All' },
		{ value: 'examples', label: 'Built-in examples' },
		{ value: 'contributed', label: 'Contributed' }
	];

	function statusLabel(value: string) {
		return ({ active: 'Available', 'needs-reverify': 'Needs review', superseded: 'Superseded', retired: 'Archived' } as Record<string, string>)[value] ?? value;
	}

	function reset() {
		query = '';
		product = 'all';
		task = 'all';
		source = 'all';
	}

	async function loadCatalog() {
		loading = true;
		error = '';
		try {
			memories = (await api<MemoryCatalog>('/v1/memories')).memories;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not load the memory library. Check the connection and try Refresh.';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		void loadCatalog();
	});
</script>

<svelte:head>
	<title>Memory library — Web Agentic Memory</title>
	<meta name="description" content="Search workflow memories for sites like Google Docs, Google Sheets and Gmail, including built-in examples you can use to try recall." />
</svelte:head>

<section class="page-shell">
	<div class="container">
		<div class="page-heading">
			<span class="eyebrow">Memory library</span>
			<h1>Find the workflow<br />your agent needs.</h1>
			<p>Search shared memories by site, product or task. Entries marked <b>Example</b> are built-in illustrations for trying recall: they are not verified site knowledge. Your search stays on this device.</p>
			<a class="button button-dark button-small" href="/guide">Connect your agent <span aria-hidden="true">↗</span></a>
		</div>

		<div class="toolbar">
			<label class="search-field">
				<span class="sr-only">Search memories</span>
				<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><circle cx="10.8" cy="10.8" r="6.4" stroke="currentColor" stroke-width="1.7" /><path d="m15.5 15.5 4.2 4.2" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" /></svg>
				<input type="search" bind:value={query} placeholder="Search a site, a product or a task, for example “work log”" />
			</label>
			<button class="button button-light button-small" type="button" onclick={loadCatalog} disabled={loading}>{loading ? 'Loading…' : 'Refresh'}</button>
		</div>
		<div class="suggest" role="group" aria-label="Search suggestions">
			<span>Try</span>
			{#each SUGGESTIONS as suggestion}
				<button type="button" class="chip" onclick={() => (query = suggestion)}>{suggestion}</button>
			{/each}
		</div>

		<div class="facets">
			<div class="facet" role="group" aria-label="Filter by product">
				<span class="facet-name">Product</span>
				<button class:active={product === 'all'} aria-pressed={product === 'all'} class="filter-tab" type="button" onclick={() => (product = 'all')}>All</button>
				{#each products as item (item.key)}
					<button class:active={product === item.key} aria-pressed={product === item.key} class="filter-tab" type="button" onclick={() => (product = item.key)}>{item.label} <small>{item.count}</small></button>
				{/each}
			</div>
			<div class="facet" role="group" aria-label="Filter by task">
				<span class="facet-name">Task</span>
				<button class:active={task === 'all'} aria-pressed={task === 'all'} class="filter-tab" type="button" onclick={() => (task = 'all')}>All</button>
				{#each TASKS as item (item.code)}
					<button class:active={task === item.code} aria-pressed={task === item.code} class="filter-tab" type="button" onclick={() => (task = item.code)}>{item.label}</button>
				{/each}
			</div>
			<div class="facet" role="group" aria-label="Filter by source">
				<span class="facet-name">Source</span>
				{#each sources as item (item.value)}
					<button class:active={source === item.value} aria-pressed={source === item.value} class="filter-tab" type="button" onclick={() => (source = item.value)}>{item.label}</button>
				{/each}
			</div>
		</div>

		{#if error}
			<div class="notice-box" role="alert">{error}</div>
		{:else if loading}
			<div class="empty-state soft-card" role="status"><h2>Loading the library…</h2></div>
		{:else}
			<div class="results-line" role="status"><span><strong>{results.length}</strong> of {memories.length} workflow{memories.length === 1 ? '' : 's'}{#if exampleCount}{' · '}{exampleCount} built-in example{exampleCount === 1 ? '' : 's'}{/if}</span><span>Searched on this device</span></div>
			{#if !memories.length}
				<div class="empty-state soft-card">
					<span class="empty-rule" aria-hidden="true"></span>
					<h2>No memories yet.</h2>
					<p>Contribute a workflow and follow its review status.</p>
					<a class="button button-light" href="/contribute">How to contribute <span aria-hidden="true">↗</span></a>
				</div>
			{:else if results.length}
				<div class="memory-grid">
					{#each results as memory (memory.memory_id)}
						{@const item = productOf(memory)}
						<a class="memory-card" href={'/memories/' + encodeURIComponent(memory.memory_id)}>
							<div class="memory-card-top">
								<span class="site-icon"><SiteGlyph kind={item.glyph} /></span>
								{#if isExample(memory)}
									<span class="pill pill-example">Example</span>
								{:else}
									<span class="pill" class:pill-green={memory.status === 'active'} class:pill-coral={memory.status === 'needs-reverify'}>{statusLabel(memory.status)}</span>
								{/if}
							</div>
							<span class="card-product">{item.label}</span>
							<h3>{displayTitle(memory)}</h3>
							<p>{memory.summary}</p>
							<div class="memory-card-meta"><span>{taskLabel(memory.task_category)} · {memory.domain}</span><span>{isExample(memory) ? 'Written' : 'Verified'} {formatDate(memory.verified_date)}</span></div>
						</a>
					{/each}
				</div>
			{:else}
				<div class="empty-state soft-card">
					<span class="empty-icon" aria-hidden="true">⌕</span>
					<h2>No matching memories</h2>
					<p>Try another site, product or task, or fewer words.</p>
					{#if filtering}<button class="button button-light" type="button" onclick={reset}>Clear search and filters</button>{/if}
				</div>
			{/if}
		{/if}
	</div>
</section>

<style>
	.suggest { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: -8px 0 20px; color: #5a6c61; font-size: 12px; }
	.chip { padding: 5px 11px; border: 1px solid #dfe5dc; border-radius: 999px; color: #365a44; background: #f3f7ec; font: 600 12px 'DM Sans', sans-serif; cursor: pointer; }
	.chip:hover { background: #e8f0da; }
	.chip:focus-visible, .filter-tab:focus-visible { outline: 2px solid #55783a; outline-offset: 2px; }
	.facets { display: grid; gap: 10px; margin: 0 0 6px; }
	.facet { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
	.facet-name { min-width: 62px; color: #4f6257; font: 700 11px 'DM Sans', sans-serif; letter-spacing: .04em; text-transform: uppercase; }
	/* The shared greys for these small texts are a little too light for WCAG AA on this page's backgrounds. */
	.results-line { color: #4a5c50; }
	.results-line strong { color: #2f4638; }
	.memory-card-meta { color: #566a5d; }
	.filter-tab { color: #46594d; }
	.filter-tab.active { color: #f7f8ee; }
	.filter-tab small { margin-left: 4px; font-size: 10px; font-weight: 700; }
	.card-product { margin: -4px 0 7px; color: #4f6a55; font: 700 11px 'DM Sans', sans-serif; letter-spacing: .02em; }
	.pill-example { border-color: #cfdca8; color: #4b5e1c; background: #f1f6dc; font-family: 'DM Sans', sans-serif; font-weight: 700; }
</style>
