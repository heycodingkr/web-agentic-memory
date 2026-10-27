<script lang="ts">
	import { api, type ReviewQueue, type ReviewQueueItem, type Contribution } from '#lib/api';

	let reviewerToken = $state('');
	let queue = $state<ReviewQueue | null>(null);
	let busy = $state(false);
	let busyId = $state('');
	let error = $state('');
	let notice = $state('');

	const statusLabels: Record<Contribution['status'], string> = {
		queued: 'Queued',
		in_review: 'In review',
		approved: 'Approved',
		rejected: 'Rejected'
	};

	async function refreshQueue() {
		queue = await api<ReviewQueue>('/v1/review/contributions', {}, reviewerToken);
	}

	async function loadQueue() {
		error = '';
		notice = '';
		busy = true;
		try {
			await refreshQueue();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not load the review queue.';
		} finally {
			busy = false;
		}
	}

	async function claim(item: ReviewQueueItem) {
		busyId = item.record.id;
		error = '';
		notice = '';
		try {
			await api<ReviewQueueItem>(
				`/v1/review/contributions/${encodeURIComponent(item.record.id)}/claim`,
				{ method: 'POST' },
				reviewerToken
			);
			await refreshQueue();
			notice = `Started reviewing ${item.record.id}.`;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not start the review.';
		} finally {
			busyId = '';
		}
	}

	async function decide(item: ReviewQueueItem, decision: 'approve' | 'reject') {
		busyId = item.record.id;
		error = '';
		notice = '';
		try {
			await api<Contribution>(
				`/v1/review/contributions/${encodeURIComponent(item.record.id)}/decision`,
				{
					method: 'POST',
					body: JSON.stringify({ review_generation: item.review_generation, decision })
				},
				reviewerToken
			);
			await refreshQueue();
			notice = `Recorded a review decision for ${item.record.id}.`;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not save the review decision.';
		} finally {
			busyId = '';
		}
	}
</script>

<svelte:head>
	<title>Local review — Web Agentic Memory</title>
	<meta name="description" content="Review local contribution submissions and record their status." />
</svelte:head>

<section class="page-shell">
	<div class="container">
		<div class="page-heading">
			<span class="eyebrow">LOCAL REVIEW CONSOLE</span>
			<h1>Review submissions<br />and record a decision.</h1>
			<p>This page only handles submissions from the local development server. Approval or rejection changes review status only; it does not publish a memory or award points.</p>
		</div>

		<section class="soft-card auth-panel" aria-labelledby="review-auth-heading">
			<label id="review-auth-heading" for="review-token">Local review token</label>
			<div class="auth-row">
				<input id="review-token" type="password" bind:value={reviewerToken} autocomplete="off" placeholder="Value of WAM_REVIEW_TOKEN" />
				<button class="button button-dark button-small" type="button" onclick={loadQueue} disabled={!reviewerToken || busy}>{busy ? 'Loading…' : 'Load review queue'}</button>
			</div>
			<p>The review token is sent in the request header and is not stored in the browser. Configure it separately from contributor tokens.</p>
		</section>

		{#if error}<div class="notice-box message" role="alert">{error}</div>{/if}
		{#if notice}<div class="success-box message" role="status">{notice}</div>{/if}

		{#if queue}
			<section class="count-grid" aria-label="Contribution review counts and queue health">
				<div class="soft-card count-card"><span>Queued</span><strong>{queue.counts.queued}</strong></div>
				<div class="soft-card count-card"><span>In review</span><strong>{queue.counts.in_review}</strong></div>
				<div class="soft-card count-card"><span>Approved</span><strong>{queue.counts.approved}</strong></div>
				<div class="soft-card count-card"><span>Rejected</span><strong>{queue.counts.rejected}</strong></div>
				<div class="soft-card count-card"><span>Active queue jobs</span><strong>{queue.queue_health.active_jobs}</strong></div>
				<div class="soft-card count-card"><span>Oldest wait</span><strong>{queue.queue_health.oldest_age_secs}s</strong><small>{queue.queue_health.alert ? 'Needs attention' : 'Healthy'}</small></div>
			</section>

			<section class="soft-card queue-panel" aria-labelledby="queue-heading">
				<div class="panel-heading"><div><span class="section-label">REVIEW QUEUE</span><h2 id="queue-heading">Local submissions</h2></div><button class="button button-light button-small" type="button" onclick={loadQueue} disabled={busy}>Refresh</button></div>
				{#if queue.contributions.length}
					<div class="queue-list">
						{#each queue.contributions as item (item.record.id)}
							<article class="queue-row">
								<div class="queue-info">
									<div class="queue-title"><strong>{item.record.id}</strong><span class="status-chip" data-status={item.record.status}>{statusLabels[item.record.status]}</span></div>
									<p>{item.record.url_pattern} · {item.record.task_category}</p>
										<p>{item.record.actions.join(' → ')} · {new Date(item.record.submitted_at * 1000).toLocaleString('en-US')}</p>
								</div>
								<div class="queue-actions">
									{#if item.record.status === 'queued'}
										<button class="button button-light button-small" type="button" onclick={() => claim(item)} disabled={busy || !!busyId}>Start review</button>
									{:else if item.record.status === 'in_review'}
										<button class="button button-light button-small" type="button" onclick={() => decide(item, 'reject')} disabled={busy || !!busyId}>Reject</button>
										<button class="button button-dark button-small" type="button" onclick={() => decide(item, 'approve')} disabled={busy || !!busyId}>Record approval</button>
									{/if}
								</div>
							</article>
						{/each}
					</div>
				{:else}
					<p class="empty-state">No saved contributions.</p>
				{/if}
			</section>

			<section class="soft-card history-panel" aria-labelledby="history-heading">
				<div class="panel-heading"><div><span class="section-label">REVIEW AUDIT</span><h2 id="history-heading">Review decision history</h2></div></div>
				{#if queue.decision_history.length}
					<div class="history-list">
						{#each queue.decision_history as event, index (`${event.contribution_id}-${event.review_generation}-${index}`)}
							<div class="history-row"><strong>{event.contribution_id} · {statusLabels[event.decision]}</strong><span>{event.reviewer} · generation {event.review_generation} · {new Date(event.decided_at * 1000).toLocaleString('en-US')}</span></div>
						{/each}
					</div>
				{:else}
					<p class="empty-state">No review decisions recorded yet.</p>
				{/if}
			</section>
		{/if}
	</div>
</section>

<style>
	.auth-panel { max-width: 820px; margin-bottom: 18px; padding: 18px 20px; }
	.auth-panel label { color: #586a5e; font-size: 11px; font-weight: 700; }
	.auth-row { display: flex; gap: 10px; margin-top: 8px; }
	.auth-row input { min-width: 0; flex: 1; min-height: 42px; padding: 10px 12px; border: 1px solid #dce2d9; border-radius: 6px; color: #18312a; background: #fffef9; }
	.auth-panel p { margin: 8px 0 0; color: #849087; font-size: 10px; }
	.message { max-width: 820px; margin: 12px 0; }
	.count-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; margin: 20px 0; }
	.count-card { display: grid; gap: 8px; padding: 16px 18px; }
	.count-card span { color: #78877d; font-size: 11px; }
	.count-card strong { color: #294b39; font-size: 26px; }
	.count-card small { color: #78877d; font-size: 9px; }
	.queue-panel { margin-top: 22px; padding: 24px; }
	.history-panel { margin-top: 16px; padding: 24px; }
	.queue-panel h2 { margin-top: 6px; color: #203b2d; font-size: 19px; }
	.panel-heading { display: flex; justify-content: space-between; align-items: center; gap: 12px; margin-bottom: 14px; }
	.queue-list { display: grid; }
	.queue-row { display: flex; justify-content: space-between; align-items: center; gap: 20px; padding: 15px 0; border-top: 1px solid #e9eee7; }
	.queue-info { display: grid; gap: 6px; min-width: 0; }
	.queue-title { display: flex; align-items: center; gap: 9px; color: #315847; font-size: 12px; }
	.queue-info p { margin: 0; color: #79877e; font-size: 10px; overflow-wrap: anywhere; }
	.status-chip { padding: 4px 7px; border-radius: 99px; color: #4b6656; background: #edf2eb; font-size: 9px; }
	.status-chip[data-status='approved'] { color: #315847; background: #e4f1dc; }
	.status-chip[data-status='rejected'] { color: #89554c; background: #f7e9e5; }
	.queue-actions { display: flex; flex-shrink: 0; gap: 8px; }
	.history-list { display: grid; }
	.history-row { display: grid; gap: 5px; padding: 11px 0; border-top: 1px solid #e9eee7; }
	.history-row strong { color: #315847; font-size: 11px; }
	.history-row span { color: #79877e; font-size: 10px; }
	.empty-state { padding: 24px 0; color: #79877e; font-size: 12px; }
	@media (max-width: 700px) {
		.count-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
		.queue-row { align-items: flex-start; flex-direction: column; }
	}
	@media (max-width: 560px) {
		.auth-row { flex-direction: column; }
		.panel-heading { align-items: flex-start; flex-direction: column; }
		.queue-actions { width: 100%; }
	}
</style>
