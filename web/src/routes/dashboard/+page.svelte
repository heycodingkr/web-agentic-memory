<script lang="ts">
	import { onMount } from 'svelte';
	import { api, ApiError, setCsrfToken, type AuthSession, type ContributionList, type Dashboard, type ScoreEventList } from '#lib/api';

	let token = $state('');
	let dashboard = $state<Dashboard | null>(null);
	let contributions = $state<ContributionList['contributions']>([]);
	let error = $state('');
	let busy = $state(false);
	let signedIn = $state(false);
	let checkingSession = $state(true);
	let scoreEvents = $state<ScoreEventList['events']>([]);
	let scoreCursor = $state<string | null>(null);
	let historyBusy = $state(false);
	let historyError = $state('');
	let loadGeneration = 0;

	async function loadDashboard() {
		const generation = ++loadGeneration;
		const credential = token || undefined;
		error = ''; busy = true;
		dashboard = null; contributions = [];
		scoreEvents = []; scoreCursor = null;
		historyError = ''; historyBusy = false;
		try {
			const [summary, submitted, history] = await Promise.all([
				api<Dashboard>('/v1/me/dashboard', {}, credential),
				api<ContributionList>('/v1/me/contributions', {}, credential),
				api<ScoreEventList>('/v1/me/score-events', {}, credential)
			]);
			if (generation !== loadGeneration) return;
			dashboard = summary;
			contributions = submitted.contributions;
			scoreEvents = history.events;
			scoreCursor = history.next_cursor;
		}
		catch (cause) {
			if (generation !== loadGeneration) return;
			error = cause instanceof Error ? cause.message : 'Could not load your rank.';
			if (cause instanceof ApiError && cause.status === 401 && signedIn) {
				setCsrfToken(); signedIn = false;
			}
		}
		finally { if (generation === loadGeneration) busy = false; }
	}

	async function loadMoreHistory() {
		if (!scoreCursor || historyBusy || busy) return;
		const generation = loadGeneration;
		const cursor = scoreCursor;
		historyBusy = true; historyError = '';
		try {
			const page = await api<ScoreEventList>(
				'/v1/me/score-events?before=' + encodeURIComponent(cursor), {}, token || undefined
			);
			if (generation !== loadGeneration) return;
			scoreEvents = [...scoreEvents, ...page.events];
			scoreCursor = page.next_cursor;
		} catch (cause) {
			if (generation !== loadGeneration) return;
			if (cause instanceof ApiError && cause.status === 401) {
				historyBusy = false;
				++loadGeneration;
				dashboard = null; contributions = []; scoreEvents = []; scoreCursor = null;
				setCsrfToken(); signedIn = false;
				error = cause.message;
			} else {
			historyError = cause instanceof Error ? cause.message : 'Could not load score history.';
			}
		} finally { if (generation === loadGeneration) historyBusy = false; }
	}

	onMount(async () => {
		try {
			const session = await api<AuthSession>('/v1/auth/session');
			setCsrfToken(session.csrf_token);
			signedIn = true;
			await loadDashboard();
		} catch (cause) {
			setCsrfToken();
			if (cause instanceof Error && 'status' in cause && cause.status !== 401) error = cause.message;
		} finally {
			checkingSession = false;
		}
	});
</script>

<svelte:head>
	<title>My rank — Web Agentic Memory</title>
	<meta name="description" content="Review your points, overall rank, and public rank." />
</svelte:head>

<section class="page-shell">
	<div class="container">
		<div class="page-heading">
			<span class="eyebrow">YOUR RANK</span>
			<h1>My contributions and rank.</h1>
			<p>Your overall rank is calculated among eligible accounts whether or not you opt in. Your public rank appears only when you choose to show your profile.</p>
			<div class="demo-note">Sign in with a GitHub web session or use a local demo token. Device tokens do not grant dashboard access.</div>
		</div>

		{#if checkingSession}
		<section class="soft-card rank-login"><p>Checking your sign-in…</p></section>
	{:else if signedIn}
		<section class="soft-card rank-login"><p>Signed in with GitHub. Your personal rank is private.</p><button class="button button-dark button-small" type="button" onclick={loadDashboard} disabled={busy}>{busy ? 'Loading…' : 'Refresh rank'}</button><a class="text-link" href="/login">Account and sign-out settings →</a></section>
	{:else}
	<section class="soft-card rank-login">
			<label for="rank-token">Local demo token</label>
			<div><input id="rank-token" type="password" bind:value={token} autocomplete="off" placeholder="Value of WAM_DEMO_TOKEN" /><button class="button button-dark button-small" type="button" onclick={loadDashboard} disabled={!token || busy}>{busy ? 'Loading…' : 'View my rank'}</button></div>
			<p>The token is sent in a request header and is not stored in this browser.</p>
		<a class="text-link" href="/login">Sign in with GitHub →</a>
		</section>
		{/if}

		{#if dashboard}
			<div class="grid-3 rank-metrics" aria-label="My rank and points">
				<section class="soft-card metric-card"><span>Total points</span><strong>{dashboard.total_points.toLocaleString('en-US')}</strong><small>Server-recorded total</small></section>
				<section class="soft-card metric-card"><span>Overall rank</span><strong>{dashboard.overall_rank ? `#${dashboard.overall_rank}` : '—'}</strong><small>Includes opted-out accounts</small></section>
				<section class="soft-card metric-card"><span>Public rank</span><strong>{dashboard.public_rank ? `#${dashboard.public_rank}` : 'Private'}</strong><small>Opted-in profiles only</small></section>
			</div>
			<section class="soft-card score-history" aria-busy={historyBusy}>
				<div class="panel-heading">
					<div><span class="section-label">SCORE HISTORY</span><h2>My point history</h2></div>
				</div>
				<p class="history-note">Points are awarded when another signed-in account reports a successful use. Seeded demo points are not included here.</p>
				{#if scoreEvents.length}
					<div class="history-table">
						<table>
							<caption class="sr-only">My point history, newest first</caption>
							<thead><tr><th scope="col">Date</th><th scope="col">Reason and memory</th><th scope="col">Points</th></tr></thead>
							<tbody>
								{#each scoreEvents as event (event.id)}
									<tr>
										<td><time datetime={new Date(event.created_at * 1000).toISOString()}>{new Date(event.created_at * 1000).toLocaleString('en-US', { dateStyle: 'medium', timeStyle: 'short' })}</time></td>
										<td><strong>{event.event_type === 'successful_use' ? 'Successful use by another account' : 'Point adjustment'}</strong><a href={'/memories/' + encodeURIComponent(event.memory_id)}>{event.memory_id} · {event.memory_version_id}</a></td>
										<td class="score-delta" class:positive={event.points_delta > 0}>{event.points_delta > 0 ? '+' : ''}{event.points_delta.toLocaleString('en-US')}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{:else}
					<p class="status-empty">No points have been awarded yet.</p>
				{/if}
				{#if scoreCursor}<button class="button button-light button-small history-more" type="button" onclick={loadMoreHistory} disabled={historyBusy || busy}>{historyBusy ? 'Loading…' : 'Load older entries'}</button>{/if}
				{#if historyError}<p class="rank-error" role="alert">{historyError}</p>{/if}
			</section>
			<section class="soft-card contribution-status">
	<div class="panel-heading"><div><span class="section-label">CONTRIBUTION STATUS</span><h2>My submissions</h2></div><a class="text-link" href="/contribute">Submit a contribution →</a></div>
	{#if contributions.length}
		<div class="status-list">{#each contributions.slice(0, 5) as item (item.id)}<div class="status-row"><span><strong>{item.id}</strong><small>{item.url_pattern} · {item.task_category}</small></span><span class="pill">{item.status}</span></div>{/each}</div>
	{:else}
		<p class="status-empty">No contributions submitted yet.</p>
	{/if}
</section>
<a class="button button-light profile-action" href="/settings/profile">Edit handle, bio, and visibility <span>→</span></a>
		{/if}
		{#if error}<p class="rank-error" role="alert">{error}</p>{/if}
	</div>
</section>

<style>
	.rank-login { max-width: 700px; padding: 20px; }
	.rank-login label { display: block; color: #586a5e; font-size: 11px; font-weight: 700; }
	.rank-login > div { display: flex; gap: 10px; margin-top: 8px; }
	.rank-login input { min-width: 0; flex: 1; min-height: 42px; padding: 10px 12px; border: 1px solid #dce2d9; border-radius: 6px; background: #fffef9; color: #18312a; }
	.rank-login p { margin: 8px 0 0; color: #849087; font-size: 10px; }
	.rank-metrics { margin-top: 22px; }
	.metric-card { display: grid; gap: 9px; padding: 23px; }
	.metric-card span { color: #718078; font-size: 11px; }
	.metric-card strong { color: #244f3f; font: 600 32px Manrope, sans-serif; }
	.metric-card small { color: #89958d; font-size: 10px; }
	.profile-action { margin-top: 23px; }
	.rank-error { color: #9a4f3f; font-size: 12px; }
	.score-history { margin-top: 20px; padding: 22px; }
	.score-history h2 { margin-top: 6px; color: #203b2d; font-size: 18px; }
	.history-note { color: #718078; font-size: 11px; line-height: 1.7; }
	.history-table { overflow-x: auto; }
	.history-table table { width: 100%; border-collapse: collapse; text-align: left; font-size: 11px; }
	.history-table th { padding: 10px 8px; color: #718078; font-weight: 600; }
	.history-table td { padding: 13px 8px; border-top: 1px solid #e8ece6; color: #64766a; vertical-align: top; }
	.history-table time { white-space: nowrap; font-size: 10px; }
	.history-table td:nth-child(2) { min-width: 155px; }
	.history-table strong, .history-table a { display: block; }
	.history-table a { margin-top: 5px; color: #547b63; overflow-wrap: anywhere; }
	.history-table .score-delta { text-align: right; color: #9a4f3f; font-weight: 700; font-size: 16px; }
	.history-table .score-delta.positive { color: #315847; }
	.history-more { margin-top: 16px; }
	.sr-only { position: absolute; width: 1px; height: 1px; padding: 0; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; }
	.contribution-status { margin-top: 20px; padding: 22px; }
	.contribution-status h2 { margin-top: 6px; color: #203b2d; font-size: 18px; }
	.status-list { display: grid; gap: 8px; }
	.status-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 10px 0; border-top: 1px solid #e8ece6; }
	.status-row > span:first-child { display: grid; gap: 4px; min-width: 0; }
	.status-row strong { color: #315847; font-size: 11px; }
	.status-row small, .status-empty { color: #849087; font-size: 10px; overflow-wrap: anywhere; }
	@media (max-width: 700px) { .rank-metrics { grid-template-columns: 1fr; } }
</style>
