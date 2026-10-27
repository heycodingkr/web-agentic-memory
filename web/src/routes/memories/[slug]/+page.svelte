<script lang="ts">
	import { onMount } from 'svelte';
	import type { PageProps } from './$types';
	import { api, type CatalogMemory, type MemoryCatalog, type MemoryDetail } from '#lib/api';

	let { data }: PageProps = $props();
	let memory = $state<CatalogMemory | null>(null);
	let hint = $state<MemoryDetail | null>(null);
	let detail = $state<MemoryDetail | null>(null);
	let loading = $state(true);
	let error = $state('');

	const steps = $derived(
		detail?.content.split(/\s*\d+\.\s*/).filter(Boolean) ?? []
	);

	function statusLabel(value: string) {
		return ({ active: 'Available', 'needs-reverify': 'Needs review', superseded: 'Superseded', retired: 'Archived' } as Record<string, string>)[value] ?? value;
	}

	function taskLabel(value: string) {
		return ({ 'compose-draft': 'Draft writing', navigate: 'Site navigation', research: 'Research' } as Record<string, string>)[value] ?? value;
	}

	async function loadMemory() {
		loading = true;
		error = '';
		try {
			const encodedId = encodeURIComponent(data.memoryId);
			const [catalog, shortHint, fullDetail] = await Promise.all([
				api<MemoryCatalog>('/v1/memories'),
				api<MemoryDetail>(`/v1/memories/${encodedId}?level=1`),
				api<MemoryDetail>(`/v1/memories/${encodedId}?level=2`)
			]);
			memory = catalog.memories.find((item) => item.memory_id === data.memoryId) ?? null;
			if (!memory) throw new Error('This memory is not in the public catalog.');
			hint = shortHint;
			detail = fullDetail;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not load memory details.';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		void loadMemory();
	});
</script>

<svelte:head>
	<title>{memory ? `${memory.title} — Web Agentic Memory` : 'Memory details — Web Agentic Memory'}</title>
	<meta name="description" content={memory?.summary ?? 'Details for an approved public memory.'} />
</svelte:head>

<section class="page-shell">
	<div class="container">
		<div class="detail-back"><a class="text-link" href="/memories"><span aria-hidden="true">←</span> Explore memories</a>{#if memory}<span class="pill" class:pill-green={memory.status === 'active'} class:pill-coral={memory.status === 'needs-reverify'}>{statusLabel(memory.status)}</span>{/if}</div>
		{#if loading}
			<div class="empty-state soft-card" role="status"><h2>Loading memory details…</h2></div>
		{:else if error || !memory}
			<div class="notice-box" role="alert"><strong>Memory unavailable.</strong>{error || 'This memory is not in the public catalog.'}<button class="button button-light button-small" type="button" onclick={loadMemory}>Try again</button></div>
		{:else}
			<div class="detail-layout">
				<article class="detail-main soft-card">
					<div class="detail-title-meta"><span>{memory.site}</span><span>{taskLabel(memory.task_category)}</span><span>{memory.kind === 'workflow' ? 'Workflow' : memory.kind}</span></div>
					<h1>{memory.title}</h1>
					<p class="detail-summary">{memory.summary}</p>
					<section class="hint-section" aria-labelledby="hint-heading">
						<div class="detail-section-heading"><h2 id="hint-heading">Quick hint</h2><span>Reference</span></div>
						<div class="detail-hint">{hint?.content}</div>
					</section>
					<section aria-labelledby="workflow-heading">
						<div class="detail-section-heading"><h2 id="workflow-heading">Workflow</h2><span>Verified steps</span></div>
						<ol class="step-list">
							{#each steps as step}
								<li>{step}</li>
							{/each}
						</ol>
					</section>
					<p class="detail-caution">Use this memory as a reference. Check the current page and the user's request before applying it.</p>
					<a class="button button-dark" href="/guide">Agent setup guide <span aria-hidden="true">↗</span></a>
				</article>
				<aside class="detail-aside soft-card">
					<span class="section-label">MEMORY DETAILS</span>
					<h3>{memory.site}</h3>
					<dl>
						<div><dt>Task</dt><dd>{taskLabel(memory.task_category)}</dd></div>
						<div><dt>Type</dt><dd>{memory.kind === 'workflow' ? 'Workflow' : memory.kind}</dd></div>
						<div><dt>Last verified</dt><dd>{memory.verified_date}</dd></div>
					</dl>
					<details>
						<summary>Version and URL scope</summary>
						<p>{memory.version_id}</p>
						<p>{memory.canonical_url_patterns.join(', ')}</p>
					</details>
					<div class="caution-box">Never share account credentials or passwords, even if a workflow asks for them.</div>
				</aside>
			</div>
		{/if}
	</div>
</section>
