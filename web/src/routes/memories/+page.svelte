<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { afterNavigate } from '$app/navigation';
	import { api, type CatalogMemory, type MemoryCatalog } from '#lib/api';
	import { rankCatalogMemories } from '#lib/local-search';
	import SiteGlyph from '#lib/components/SiteGlyph.svelte';
	import { publicSiteOrigin } from '#lib/site.js';
	import { DOCUMENT_TEMPLATES, SUGGESTIONS, TASKS, WORKFLOW_CATEGORIES, applyFilters, categoryCounts, displayTitle, expandQuery, formatDate, isExample, libraryOrder, productCounts, productOf, siteScope, taskLabel, type Source, type WorkflowCategory } from '#lib/memories';

	let memories = $state<CatalogMemory[]>([]);
	let query = $state('');
	let product = $state('all');
	let category = $state<WorkflowCategory | 'all'>('all');
	let task = $state('all');
	let source = $state<Source>('all');
	let loading = $state(true);
	let error = $state('');

	const scope = $derived(siteScope(query));
	const categories = $derived(categoryCounts(memories));
	const chosenCategory = $derived(WORKFLOW_CATEGORIES.find(item => item.key === category));
	const templates = $derived(DOCUMENT_TEMPLATES.filter(template => memories.some(memory => memory.memory_id === template.id)));
	const siteMemories = $derived(applyFilters(memories, { query, category, product: 'all', task: 'all', source: 'all' }));
	const products = $derived(productCounts(siteMemories));
	const exampleCount = $derived(memories.filter(isExample).length);
	// Filters narrow the library first, then the words rank what is left. The query never leaves the browser.
	const results = $derived(rankCatalogMemories(libraryOrder(applyFilters(memories, { query, category, product, task, source })), expandQuery(query), 'all'));
	const filtering = $derived(!!query.trim() || category !== 'all' || product !== 'all' || task !== 'all' || source !== 'all');

	const sources: { value: Source; label: string }[] = [
		{ value: 'all', label: 'All' },
		{ value: 'examples', label: 'Built-in examples' },
		{ value: 'contributed', label: 'Contributed' }
	];

	function statusLabel(value: string) {
		return ({ active: 'Available', 'needs-reverify': 'Needs review', superseded: 'Superseded', retired: 'Archived' } as Record<string, string>)[value] ?? value;
	}

	let appliedSearchHash: string | undefined;
	function syncSearchHash() {
		const hash = window.location.hash;
		if (hash === appliedSearchHash) return;
		appliedSearchHash = hash;
		query = new URLSearchParams(hash.slice(1)).get('q') ?? '';
		category = 'all';
		product = 'all';
		task = 'all';
		source = 'all';
	}
	afterNavigate(syncSearchHash);

	function reset() {
		query = '';
		category = 'all';
		product = 'all';
		task = 'all';
		source = 'all';
		if (window.location.hash.startsWith('#q=')) window.history.replaceState(window.history.state, '', window.location.pathname + window.location.search);
		appliedSearchHash = window.location.hash;
	}

	function selectCategory(value: WorkflowCategory | 'all') {
		category = value;
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
		syncSearchHash();
		window.addEventListener('hashchange', syncSearchHash);
		void loadCatalog().then(async () => {
			if (window.location.hash === "#document-templates") {
				await tick();
				document.getElementById("document-templates")?.scrollIntoView();
			}
		});
		return () => window.removeEventListener('hashchange', syncSearchHash);
	});
</script>

<svelte:head>
	<title>Memory library — Web Agentic Memory</title>
	<meta name="description" content="Find Google Docs request form, meeting notes and project proposal memories, plus workflows for Google Sheets, Gmail and other sites." />
	<meta property="og:type" content="website" />
	<meta property="og:title" content="Memory library — Web Agentic Memory" />
	<meta property="og:description" content="Find shared website workflows by site, product or task, with clear labels for examples and verified memories." />
	<meta property="og:url" content={`${publicSiteOrigin}/memories`} />
	<link rel="canonical" href={`${publicSiteOrigin}/memories`} />
</svelte:head>

<section class="page-shell">
	<div class="container">
		<div class="page-heading">
			<span class="eyebrow">Memory library</span>
			<h1>Find the workflow<br />your agent needs.</h1>
			<p>Paste a website URL to see what your agent can do there. Or search by site, product or task. Your search stays on this device.</p>
			<a class="button button-dark button-small" href="/guide">Connect your agent <span aria-hidden="true">↗</span></a>
		</div>

		<div class="library-search">
			<label class="search-title" for="memory-search">Search memories</label>
			<div class="toolbar">
				<div class="search-field">
					<svg viewBox="0 0 24 24" fill="none" aria-hidden="true"><circle cx="10.8" cy="10.8" r="6.4" stroke="currentColor" stroke-width="1.7" /><path d="m15.5 15.5 4.2 4.2" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" /></svg>
					<input id="memory-search" type="search" bind:value={query} placeholder="Paste a URL, or search a site or task…" autocomplete="off" spellcheck="false" aria-describedby="memory-search-note" />
				</div>
				<button class="button button-light button-small" type="button" onclick={loadCatalog} disabled={loading}>{loading ? 'Loading…' : 'Refresh'}</button>
			</div>
			<p id="memory-search-note" class="search-note">A URL finds workflows for that website and page type. It does not open or read your page.</p>
			{#if memories.length > 0}
				<div class="suggest" role="group" aria-label="Search suggestions">
					<span>Try a task</span>
					{#each SUGGESTIONS as suggestion}
						<button type="button" class="chip" onclick={() => (query = suggestion)}>{suggestion}</button>
					{/each}
				</div>
			{/if}
		</div>

		{#if memories.length}
			<div class="workflow-browse">
				<span class="search-title" id="workflow-category-label">Explore by workflow</span>
				<div class="category-list" role="group" aria-labelledby="workflow-category-label">
					<button type="button" class:chosen={category === 'all'} aria-pressed={category === 'all'} onclick={() => selectCategory('all')}><SiteGlyph kind="folder" /><span>All workflows</span><small>{memories.length}</small></button>
					{#each categories as item (item.key)}
						<button type="button" class:chosen={category === item.key} aria-pressed={category === item.key} onclick={() => selectCategory(item.key)}><SiteGlyph kind={item.glyph} /><span>{item.label}</span><small>{item.count}</small></button>
					{/each}
				</div>
				<p class="category-description">{chosenCategory?.description ?? 'Choose a kind of work, then narrow it by website or search for a task.'}</p>
			</div>
		{/if}

		<div id="document-templates" class="template-anchor">
		{#if templates.length && !filtering}
			<section class="template-showcase" aria-labelledby="document-template-title">
				<div class="template-heading">
					<span class="eyebrow">Google Docs · Example memories</span>
					<h2 id="document-template-title">Create documents from reusable templates.</h2>
					<p>Request forms, meeting notes and project proposals. Ask your connected agent to create a document; these memories guide the layout, fields and saving.</p>
				</div>
				<div class="template-grid">
					{#each templates as template (template.id)}
						<a class="template-card" href={'/memories/' + encodeURIComponent(template.id)}>
							<div class="template-sheet" aria-hidden="true">
								<SiteGlyph kind={template.glyph} />
								<b>{template.label}</b><i></i><i></i>
								<div class="template-table">{#each Array(8) as _}<span></span>{/each}</div>
							</div>
							<h3>{template.label}</h3>
							<p>{template.description}</p>
							<blockquote><span>Ask your agent</span>“{template.prompt}”</blockquote>
							<span class="template-read">Read memory <span aria-hidden="true">→</span></span>
						</a>
					{/each}
				</div>
			</section>
		{/if}
		</div>

		<div class="facets">
			<div class="facet" role="group" aria-label="Filter by product">
				<span class="facet-name">Product</span>
				<button class:active={product === 'all'} aria-pressed={product === 'all'} class="filter-tab" type="button" onclick={() => (product = 'all')}>All</button>
				{#each products as item (item.key)}
					<button class:active={product === item.key} aria-pressed={product === item.key} class="filter-tab" type="button" onclick={() => (product = item.key)}>{item.label} <small>{item.count}</small></button>
				{/each}
			</div>
			<details class="advanced-filters">
				<summary>Task &amp; source filters{#if task !== 'all' || source !== 'all'} · {Number(task !== 'all') + Number(source !== 'all')} active{/if}</summary>
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
			</details>
		</div>

		{#if error}
			<div class="notice-box" role="alert">{error}</div>
		{:else if loading}
			<div class="empty-state soft-card" role="status"><h2>Loading the library…</h2></div>
		{:else}
			{#if scope}
				<div class="site-match"><span class="site-icon"><SiteGlyph kind="route" /></span><div><strong>Workflows for {products.length === 1 ? products[0].label : scope.host}</strong><p>Matched by website and page type. Open a memory to read its steps.</p></div></div>
			{/if}
			<div class="results-line" role="status"><span><strong>{results.length}</strong> of {memories.length} workflow{memories.length === 1 ? '' : 's'}{#if exampleCount}{' · '}{exampleCount} built-in example{exampleCount === 1 ? '' : 's'}{/if}</span><span>Searched on this device</span>{#if filtering && results.length}<button class="clear-filters" type="button" onclick={reset}>Clear search and filters</button>{/if}</div>
			{#if exampleCount}<p class="example-explainer"><b>Example</b> memories are built-in starting points, not verified site knowledge. Connect your agent to try a workflow.</p>{/if}
			{#if !memories.length}
				<div class="empty-state soft-card">
					<span class="empty-rule" aria-hidden="true"></span>
					<h2>No public memories yet.</h2>
					<p>The public catalog returned no workflows. Refresh to check again, or visit the setup guide to learn how memories work.</p>
					<button class="button button-dark" type="button" onclick={loadCatalog}>Try again</button>
					<a class="text-link" href="/guide">Read the setup guide <span aria-hidden="true">→</span></a>
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
					<p>{scope ? 'No workflows match this website and page type with the current filters. Try a broader site address, or clear the filters.' : 'Try another site, product or task, or fewer words.'}</p>
					{#if filtering}<button class="button button-light" type="button" onclick={reset}>Clear search and filters</button>{/if}
				</div>
			{/if}
		{/if}
	</div>
</section>

<style>
	.workflow-browse { margin-bottom: 28px; }
	.category-list { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 9px; }
	.category-list button { display: flex; align-items: center; gap: 10px; min-width: 0; min-height: 54px; padding: 11px 14px; border: 1px solid #d9e2d1; border-radius: 9px; color: #3c5940; background: #fffef9; text-align: left; font: 600 12px 'DM Sans', sans-serif; cursor: pointer; }
	.category-list button:hover { background: #edf3e4; }
	.category-list button.chosen { border-color: #315c3f; color: #f7f8ee; background: #254b3c; }
	.category-list button:focus-visible { outline: 2px solid #55783a; outline-offset: 3px; }
	.category-list :global(svg) { flex: none; width: 23px; height: 23px; }
	.category-list button > span { flex: 1; min-width: 0; line-height: 1.4; }
	.category-list small { flex: none; padding: 2px 6px; border-radius: 5px; color: #4e6547; background: #eaf0df; font-size: 10px; }
	.category-list .chosen small { color: #1d3c2b; background: #d8ed9d; }
	.category-description { margin: 10px 0 0; color: #566b50; font-size: 12px; line-height: 1.7; }
	@media (max-width: 640px) { .category-list { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; } .category-list button { gap: 6px; padding: 10px; font-size: 11px; } .category-list :global(svg) { width: 18px; height: 18px; } }
	.page-heading { margin-bottom: 28px; }
	.page-heading > .button { margin-top: 18px; }
	.library-search { margin: 0 0 28px; padding: 24px; border: 1px solid #d7e1cc; border-radius: 14px; background: #edf3e5; }
	.search-title { display: block; margin-bottom: 12px; color: #284730; font-size: 14px; font-weight: 700; }
	.library-search .toolbar { margin-bottom: 9px; }
	.search-field { min-width: 0; }
	.search-field input { border-color: #a9ba9c; background: #fffef9; }
	.search-field input::placeholder { color: #5e715d; opacity: 1; }
	.search-field svg { color: #4f684c; }
	.search-note { margin: 0; color: #53684e; font-size: 12px; line-height: 1.65; }
	.library-search .suggest { margin: 16px 0 0; }
	.site-match { display: flex; align-items: center; gap: 14px; margin-top: 28px; padding: 18px 20px; border-left: 3px solid #6c8b4c; border-radius: 0 8px 8px 0; background: #ecf2e1; overflow-wrap: anywhere; }
	.site-match strong { color: #2b4a31; font-size: 14px; }
	.site-match p { margin: 4px 0 0; color: #4f654d; font-size: 12px; line-height: 1.6; }
	.example-explainer { margin: -3px 0 20px; color: #566a52; font-size: 12px; line-height: 1.65; }
	.example-explainer b { color: #405d31; }
	.memory-card p { color: #526453; }
	.results-line { gap: 12px; flex-wrap: wrap; }
	.clear-filters { padding: 0; border: 0; color: #365d35; background: transparent; font: inherit; text-decoration: underline; text-underline-offset: 3px; cursor: pointer; }
	.empty-state p { max-width: 440px; color: #526453; line-height: 1.8; }
	@media (max-width: 520px) { .library-search { padding: 18px; } .search-field input { font-size: 12px; } }
	.template-anchor { scroll-margin-top: 110px; }
	.template-showcase { margin: 0 0 38px; }
	.template-heading { max-width: 680px; margin-bottom: 20px; }
	.template-heading h2 { margin: 10px 0; color: #213e30; font: 600 clamp(22px, 3vw, 30px)/1.2 Manrope, sans-serif; letter-spacing: -.045em; }
	.template-heading p { margin: 0; color: #536757; font-size: 13px; line-height: 1.7; }
	.template-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 16px; }
	.template-card { display: flex; flex-direction: column; min-width: 0; padding: 22px; border: 1px solid #d9e2d0; border-radius: 12px; background: #f3f6eb; transition: border-color .2s, transform .2s; }
	.template-card:hover { border-color: #8da67d; transform: translateY(-3px); }
	.template-sheet { position: relative; width: 130px; margin: 0 auto 22px; padding: 16px; border: 1px solid #d9dfd1; border-radius: 2px; background: #fffef9; box-shadow: 4px 5px 0 #dfe7d3; transform: rotate(-3deg); }
	.template-sheet :global(svg) { width: 18px; height: 18px; color: #54764b; }
	.template-sheet b { display: block; margin: 10px 0; color: #314b39; font-size: 9px; }
	.template-sheet i { display: block; height: 3px; margin: 7px 0; background: #dce3d5; }
	.template-sheet i + i { width: 70%; }
	.template-table { display: grid; grid-template-columns: repeat(4, 1fr); margin-top: 12px; border-top: 1px solid #dce3d5; border-left: 1px solid #dce3d5; }
	.template-table span { height: 11px; border-right: 1px solid #dce3d5; border-bottom: 1px solid #dce3d5; }
	.template-card h3 { margin: 0 0 8px; color: #294833; font: 600 17px Manrope, sans-serif; letter-spacing: -.025em; }
	.template-card > p { margin: 0; color: #4e6353; font-size: 12px; line-height: 1.7; }
	.template-card blockquote { margin: 18px 0; color: #425b48; font-size: 11px; line-height: 1.7; }
	.template-card blockquote > span { display: block; margin-bottom: 4px; color: #5b6d57; font-weight: 700; font-size: 10px; }
	.template-read { margin-top: auto; color: #335b35; font-size: 12px; font-weight: 700; }
	@media (max-width: 760px) { .template-grid { grid-template-columns: 1fr; } .template-sheet { float: right; margin: 0 0 18px 20px; } .template-card { display: block; } }
	@media (max-width: 520px) { .template-sheet { width: 96px; padding: 12px; margin-left: 14px; } }
	.suggest { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: -8px 0 20px; color: #5a6c61; font-size: 12px; }
	.chip { padding: 5px 11px; border: 1px solid #dfe5dc; border-radius: 999px; color: #365a44; background: #f3f7ec; font: 600 12px 'DM Sans', sans-serif; cursor: pointer; }
	.chip:hover { background: #e8f0da; }
	.chip:focus-visible, .filter-tab:focus-visible { outline: 2px solid #55783a; outline-offset: 2px; }
	.facets { display: grid; gap: 10px; margin: 0 0 6px; }
	.advanced-filters summary { width: fit-content; padding: 8px 0; color: #4f6257; font-size: 12px; font-weight: 600; cursor: pointer; }
	.advanced-filters summary:focus-visible { outline: 2px solid #55783a; outline-offset: 3px; }
	.advanced-filters .facet { margin-top: 10px; }
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
