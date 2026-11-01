<script lang="ts">
	import { api, type PublicUsageMetrics } from '#lib/api';
	import { publicSiteOrigin } from '#lib/site.js';

	let metrics = $state<PublicUsageMetrics | null>(null);
	let error = $state('');
	let loading = $state(true);

	async function loadMetrics() {
		loading = true;
		error = '';
		try {
			metrics = await api<PublicUsageMetrics>('/v1/metrics');
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not load aggregate metrics.';
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void loadMetrics();
	});
</script>

<svelte:head>
	<title>Public usage metrics — Web Agentic Memory</title>
	<meta name="description" content="Public recall and outcome totals without personal identifiers or raw URLs." />
	<meta property="og:type" content="website" />
	<meta property="og:title" content="Public usage metrics — Web Agentic Memory" />
	<meta property="og:description" content="Review aggregate recall requests, memory hits, detail reads and reported outcomes, with privacy limits explained." />
	<meta property="og:url" content={`${publicSiteOrigin}/metrics`} />
	<link rel="canonical" href={`${publicSiteOrigin}/metrics`} />
</svelte:head>

<section class="page-shell">
	<div class="container">
		<div class="page-heading">
			<span class="eyebrow">PUBLIC USAGE METRICS</span>
			<h1>See how shared memories<br />are being used.</h1>
			<p>These totals count recall requests, detail reads, and consented outcomes. Public aggregates exclude account IDs, devices, receipts, individual memory IDs, and URLs.</p>
		</div>

		{#if loading}
			<div class="soft-card metrics-state" role="status">Loading aggregate metrics…</div>
		{:else if error}
			<div class="notice-box metrics-state" role="alert">{error}<button class="button button-light button-small" type="button" onclick={loadMetrics}>Try again</button></div>
		{:else if metrics}
			<section class="metric-grid" aria-label="Public usage metrics">
				<div class="soft-card metric-card"><span>Total recall requests</span><strong>{metrics.recall_requests}</strong><small>Signed in {metrics.authenticated_recall_requests} · anonymous {metrics.anonymous_recall_requests}</small></div>
				<div class="soft-card metric-card"><span>Memory hints returned</span><strong>{metrics.matched_memory_hits}</strong><small>Total hits in recall responses</small></div>
				<div class="soft-card metric-card"><span>Detail reads</span><strong>{metrics.detail_requests}</strong><small>Memory detail responses sent</small></div>
				<div class="soft-card metric-card"><span>Successful outcomes</span><strong>{metrics.success_outcomes}</strong><small>Reports tied to server receipts</small></div>
				<div class="soft-card metric-card"><span>Partial outcomes</span><strong>{metrics.partial_outcomes}</strong><small>Reports tied to server receipts</small></div>
				<div class="soft-card metric-card"><span>Failed outcomes</span><strong>{metrics.failed_outcomes}</strong><small>Reports tied to server receipts</small></div>
			</section>
			<div class="notice-box metrics-note"><strong>Limits of these metrics</strong>A view does not prove that someone read or followed a workflow. Only authenticated users with a recall receipt can report an outcome. These metrics are separate from the point ledger.</div>
		{/if}
	</div>
</section>

<style>
	.metric-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; margin: 22px 0; }
	.metric-card { display: grid; gap: 8px; padding: 18px 20px; }
	.metric-card span { color: #78877d; font-size: 11px; }
	.metric-card strong { color: #294b39; font-size: 28px; }
	.metric-card small { color: #849087; font-size: 10px; }
	.metrics-state { display: grid; gap: 12px; max-width: 680px; padding: 20px; color: #586a5e; font-size: 12px; }
	.metrics-note { margin-top: 18px; }
	@media (max-width: 680px) { .metric-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
	@media (max-width: 460px) { .metric-grid { grid-template-columns: 1fr; } }
</style>
