<script lang="ts">
	import { onMount } from 'svelte';
	import type { PageProps } from './$types';
	import { api, type CatalogMemory, type MemoryCatalog, type MemoryDetail } from '#lib/api';
	import SiteGlyph from '#lib/components/SiteGlyph.svelte';
	import { actionLabel, displayTitle, formatDate, humanize, isExample, parseHint, parseLevel2, productOf, recallCommand, taskLabel } from '#lib/memories';

	let { data }: PageProps = $props();
	let memory = $state<CatalogMemory | null>(null);
	let hint = $state<MemoryDetail | null>(null);
	let detail = $state<MemoryDetail | null>(null);
	let loading = $state(true);
	let error = $state('');
	let copied = $state(false);

	const product = $derived(memory ? productOf(memory) : null);
	const parsedHint = $derived(parseHint(hint?.content ?? ''));
	const level2 = $derived(parseLevel2(detail?.content ?? ''));
	const command = $derived(memory ? recallCommand(memory) : '');
	// The walkthrough prefers the written steps; stored action codes alone are shown as a trail.
	const trail = $derived(level2.steps.map((step) => [actionLabel(step.action), step.note_code ? humanize(step.note_code) : ''].filter(Boolean).join(' · ')));

	function statusLabel(value: string) {
		return ({ active: 'Available', 'needs-reverify': 'Needs review', superseded: 'Superseded', retired: 'Archived' } as Record<string, string>)[value] ?? value;
	}

	async function copyCommand() {
		try {
			await navigator.clipboard.writeText(command);
			copied = true;
			setTimeout(() => (copied = false), 2000);
		} catch {
			copied = false;
		}
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
			if (!memory) throw new Error('This memory is not in the library.');
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
	<title>{memory ? `${displayTitle(memory)} — Web Agentic Memory` : 'Memory details — Web Agentic Memory'}</title>
	<meta name="description" content={memory?.summary ?? 'Details for a workflow memory.'} />
</svelte:head>

<section class="page-shell">
	<div class="container">
		<div class="detail-back"><a class="text-link" href="/memories"><span aria-hidden="true">←</span> Memory library</a>{#if memory}{#if isExample(memory)}<span class="pill pill-example">Example</span>{:else}<span class="pill" class:pill-green={memory.status === 'active'} class:pill-coral={memory.status === 'needs-reverify'}>{statusLabel(memory.status)}</span>{/if}{/if}</div>
		{#if loading}
			<div class="empty-state soft-card" role="status"><h2>Loading memory details…</h2></div>
		{:else if error || !memory || !product}
			<div class="notice-box" role="alert"><strong>Memory unavailable.</strong>{error || 'This memory is not in the library.'}<button class="button button-light button-small" type="button" onclick={loadMemory}>Try again</button></div>
		{:else}
			<div class="detail-layout">
				<article class="detail-main soft-card">
					<div class="detail-title-meta"><span class="product-line"><span class="site-icon small"><SiteGlyph kind={product.glyph} /></span>{product.label}</span><span>{taskLabel(memory.task_category)}</span><span>{memory.kind === 'workflow' ? 'Workflow' : memory.kind}</span></div>
					<h1>{displayTitle(memory)}</h1>
					<p class="detail-summary">{memory.summary}</p>
					{#if isExample(memory)}
						<p class="example-note" role="note"><b>Built-in example.</b> It illustrates what a memory looks like and lets you try recall. It is not verified site knowledge, so check every step on the live page.</p>
					{/if}

					{#if parsedHint.points.length}
						<section aria-labelledby="points-heading">
							<div class="detail-section-heading"><h2 id="points-heading">Key points</h2><span>What a recall returns first</span></div>
							<ul class="point-list">
								{#each parsedHint.points as point}<li>{point}</li>{/each}
							</ul>
							{#if parsedHint.pitfalls.length}
								<p class="pitfalls"><span>Watch out for</span>{#each parsedHint.pitfalls as code}<code>{humanize(code)}</code>{/each}</p>
							{/if}
						</section>
					{:else}
						<section class="hint-section" aria-labelledby="hint-heading">
							<div class="detail-section-heading"><h2 id="hint-heading">Quick hint</h2><span>Reference</span></div>
							<div class="detail-hint">{hint?.content}</div>
						</section>
					{/if}

					<section aria-labelledby="workflow-heading">
						<div class="detail-section-heading"><h2 id="workflow-heading">Walkthrough</h2><span>{isExample(memory) ? 'Example steps' : 'Verified steps'}</span></div>
						{#if level2.lines.length}
							<ol class="step-list">
								{#each level2.lines as line}<li>{line}</li>{/each}
							</ol>
						{:else if trail.length}
							<ol class="step-list">
								{#each trail as line}<li>{line}</li>{/each}
							</ol>
						{:else}
							<p class="detail-caution">No written steps for this memory.</p>
						{/if}
						{#if level2.lines.length && trail.length}
							<p class="trail" aria-label="Action trail">{#each trail as step, index}<span>{step}</span>{#if index < trail.length - 1}<i aria-hidden="true">›</i>{/if}{/each}</p>
						{/if}
					</section>

					{#if level2.preconditions.length || level2.recovery.length}
						<div class="duo">
							{#if level2.preconditions.length}
								<section aria-labelledby="pre-heading"><h2 id="pre-heading" class="small-heading">Before you start</h2><p class="chips">{#each level2.preconditions as code}<code>{humanize(code)}</code>{/each}</p></section>
							{/if}
							{#if level2.recovery.length}
								<section aria-labelledby="rec-heading"><h2 id="rec-heading" class="small-heading">If something goes wrong</h2><ul class="plain">{#each level2.recovery as line}<li>{line}</li>{/each}</ul></section>
							{/if}
						</div>
					{/if}
					{#if level2.notes}<p class="notes"><b>Notes.</b> {level2.notes}</p>{/if}

					<p class="detail-caution">Use this memory as a reference. Check the current page and the user's request before applying it.</p>
					<a class="button button-dark" href="/guide">Agent setup guide <span aria-hidden="true">↗</span></a>
				</article>
				<aside class="detail-aside soft-card">
					<span class="section-label">MEMORY DETAILS</span>
					<h3>{product.label}</h3>
					<dl>
						<div><dt>Task</dt><dd>{taskLabel(memory.task_category)}</dd></div>
						<div><dt>Type</dt><dd>{memory.kind === 'workflow' ? 'Workflow' : memory.kind}</dd></div>
						<div><dt>{isExample(memory) ? 'Written' : 'Last verified'}</dt><dd>{formatDate(memory.verified_date)}</dd></div>
						<div><dt>Scope</dt><dd>{memory.canonical_url_patterns.join(', ')}</dd></div>
					</dl>
					<div class="try-box">
						<span class="section-label">TRY RECALL</span>
						<p>Ask WAM for this memory from a terminal, or call <code>web_memory_recall</code> from your agent.</p>
						<pre><code>{command}</code></pre>
						<button class="button button-light button-small" type="button" onclick={copyCommand}>{copied ? 'Copied' : 'Copy command'}</button>
					</div>
					<details>
						<summary>Version</summary>
						<p>{memory.version_id}</p>
					</details>
					<div class="caution-box">Never share account credentials or passwords, even if a workflow asks for them.</div>
				</aside>
			</div>
		{/if}
	</div>
</section>

<style>
	/* The shared greys for these small texts are a little too light for WCAG AA on this page's backgrounds. */
	.detail-section-heading span, .detail-aside dt { color: #566a5d; }
	.detail-caution { color: #566a5d; }
	.caution-box { color: #6b5636; }
	.pill-example { border-color: #cfdca8; color: #4b5e1c; background: #f1f6dc; font-family: 'DM Sans', sans-serif; font-weight: 700; }
	.product-line { display: inline-flex; align-items: center; gap: 8px; color: #365a44; font-weight: 700; }
	.site-icon.small { width: 26px; height: 26px; border-radius: 8px; }
	.site-icon.small :global(.site-glyph) { width: 16px; height: 16px; }
	.example-note { margin: 0 0 8px; padding: 12px 15px; border: 1px solid #dfe7c5; border-radius: 9px; color: #46591f; background: #f6f9e8; font-size: 13px; line-height: 1.65; }
	.point-list { display: grid; gap: 9px; margin: 16px 0 0; padding-left: 22px; color: #34503f; font-size: 14px; line-height: 1.65; }
	.point-list li::marker { color: #7b9a32; }
	.pitfalls, .chips { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 16px 0 0; }
	.pitfalls span { color: #6a5a3c; font: 700 11px 'DM Sans', sans-serif; letter-spacing: .03em; text-transform: uppercase; }
	code { padding: 3px 8px; border-radius: 6px; color: #3b4f40; background: #eef2e8; font: 12px 'DM Mono', monospace; }
	.pitfalls code { color: #7a4d2c; background: #fbefe2; }
	.trail { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 8px; margin: 0 0 22px; color: #51655a; font-size: 12px; }
	.trail span { padding: 3px 9px; border-radius: 999px; background: #eef2e8; }
	.trail i { color: #9aa99d; font-style: normal; }
	.duo { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 22px; margin: 6px 0 20px; }
	.small-heading { margin: 0; font: 700 14px Manrope, sans-serif; }
	.plain { display: grid; gap: 7px; margin: 12px 0 0; padding-left: 18px; color: #4f6257; font-size: 13px; line-height: 1.6; }
	.notes { margin: 0 0 18px; color: #5b6d62; font-size: 12.5px; line-height: 1.7; }
	.try-box { margin-top: 20px; padding-top: 16px; border-top: 1px solid #edf0eb; }
	.try-box p { margin: 8px 0 10px; color: #5b6d62; font-size: 12px; line-height: 1.6; }
	.try-box pre { margin: 0 0 10px; padding: 10px 12px; border-radius: 8px; color: #e8f1de; background: #173a2e; font: 11.5px/1.6 'DM Mono', monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
	.try-box pre code { padding: 0; color: inherit; background: none; font: inherit; }
	@media (max-width: 760px) { .duo { grid-template-columns: 1fr; } }
</style>
