<script lang="ts">
	import { onMount } from 'svelte';
	import { api, ApiError, setCsrfToken, type AuthSession, type ContributionList, type Dashboard, type MemoryAnalytics, type MemoryAnalyticsPoint, type OwnerUsage, type ScoreEventList } from '#lib/api';

	let token = $state('');
	let dashboard = $state<Dashboard | null>(null);
	let usage = $state<OwnerUsage | null>(null);
	let memoryAnalytics = $state<MemoryAnalytics | null>(null);
	let analyticsBucket = $state<'day' | 'week'>('day');
	let analyticsBusy = $state(false);
	let analyticsError = $state('');
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

	function clearPrivateDashboard() {
		dashboard = null; usage = null; memoryAnalytics = null; contributions = [];
		scoreEvents = []; scoreCursor = null;
		historyError = ''; historyBusy = false;
		analyticsError = ''; analyticsBusy = false;
	}

	function expireAuthentication(message: string) {
		++loadGeneration;
		clearPrivateDashboard();
		setCsrfToken(); signedIn = false; busy = false;
		error = message;
	}

	async function loadDashboard() {
		const generation = ++loadGeneration;
		const credential = token || undefined;
		error = ''; busy = true;
		clearPrivateDashboard();
		try {
			const [summary, personalUsage, submitted, history, impact] = await Promise.all([
				api<Dashboard>('/v1/me/dashboard', {}, credential),
				api<OwnerUsage>('/v1/me/usage', {}, credential),
				api<ContributionList>('/v1/me/contributions', {}, credential),
				api<ScoreEventList>('/v1/me/score-events', {}, credential),
				api<MemoryAnalytics>(`/v1/me/memories/analytics?bucket=${analyticsBucket}`, {}, credential)
			]);
			if (generation !== loadGeneration) return;
			dashboard = summary;
			usage = personalUsage;
			memoryAnalytics = impact;
			contributions = submitted.contributions;
			scoreEvents = history.events;
			scoreCursor = history.next_cursor;
		}
		catch (cause) {
			if (generation !== loadGeneration) return;
			error = cause instanceof Error ? cause.message : 'Could not load your rank.';
			if (cause instanceof ApiError && cause.status === 401) expireAuthentication(cause.message);
		}
		finally { if (generation === loadGeneration) busy = false; }
	}

	async function reloadMemoryAnalytics() {
		if (analyticsBusy || busy) return;
		const generation = loadGeneration;
		analyticsBusy = true;
		analyticsError = '';
		try {
			const impact = await api<MemoryAnalytics>(`/v1/me/memories/analytics?bucket=${analyticsBucket}`, {}, token || undefined);
			if (generation === loadGeneration) memoryAnalytics = impact;
		} catch (cause) {
			if (generation !== loadGeneration) return;
			if (cause instanceof ApiError && cause.status === 401) expireAuthentication(cause.message);
			else analyticsError = cause instanceof Error ? cause.message : 'Could not load memory usage.';
		} finally {
			if (generation === loadGeneration) analyticsBusy = false;
		}
	}

	function impactTotal(memory: MemoryAnalytics['memories'][number], field: keyof Omit<MemoryAnalyticsPoint, 'period_start'>): number {
		return memory.points.reduce((sum, point) => sum + point[field], 0);
	}

	function activityFor(point: MemoryAnalyticsPoint): number {
		return point.anonymous_recall_hits + point.other_account_recall_hits + point.anonymous_detail_views + point.other_account_detail_views;
	}

	function activityMax(memory: MemoryAnalytics['memories'][number]): number {
		return Math.max(1, ...memory.points.map(activityFor));
	}

	function activityHeight(point: MemoryAnalyticsPoint, max: number): number {
		const total = activityFor(point);
		return total === 0 ? 2 : Math.max(8, total / max * 100);
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
				expireAuthentication(cause.message);
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
			{#if usage}
				<section class="soft-card usage-panel" aria-labelledby="usage-heading">
					<div class="usage-head">
						<div><span class="section-label">API USAGE</span><h2 id="usage-heading">Your agent activity</h2><p>Weighted API units refresh as the rolling minute refills. Your personal usage stays private.</p></div>
						<div class="quota-count"><strong>{usage.remaining_units}</strong><span>of {usage.burst_capacity} burst units left</span></div>
					</div>
					<div class="quota-meter" role="progressbar" aria-label="Burst capacity remaining" aria-valuemin="0" aria-valuemax={usage.burst_capacity} aria-valuenow={usage.remaining_units}>
						<span style={`width:${Math.max(0, Math.min(100, usage.remaining_units / Math.max(1, usage.burst_capacity) * 100))}%`}></span>
					</div>
					<div class="quota-meta"><span>{usage.limit_per_minute} units per minute</span><span>Refills in {usage.resets_in_seconds}s</span></div>
					<div class="usage-chart-wrap">
						<div class="usage-chart-title"><strong>Last {usage.period_days} days</strong><span>{usage.daily.reduce((sum, day) => sum + day.requests, 0).toLocaleString('en-US')} requests</span></div>
						<div class="usage-chart" role="img" aria-label={`Daily admitted API requests for the last ${usage.period_days} days. ${usage.daily.reduce((sum, day) => sum + day.requests, 0)} total admitted requests and ${usage.daily.reduce((sum, day) => sum + day.denied_requests, 0)} rate limited requests.`}>
							{#each usage.daily as day (day.utc_date)}
								<span class:usage-day-active={day.requests > 0} title={`${day.utc_date}: ${day.requests} admitted requests · ${day.denied_requests} rate limited · ${day.units} units`} style={`--usage-height:${day.requests ? Math.max(8, day.requests / Math.max(1, ...usage.daily.map((item) => item.requests)) * 100) : 2}%`}></span>
							{/each}
						</div>
						<div class="usage-chart-labels"><span>{usage.daily[0]?.utc_date}</span><span>{usage.daily.at(-1)?.utc_date}</span></div>
					</div>
					<div class="usage-breakdown"><span><i class="usage-dot recall-dot"></i>Recall <strong>{usage.daily.reduce((sum, day) => sum + day.recall_requests, 0)}</strong></span><span><i class="usage-dot detail-dot"></i>Detail reads <strong>{usage.daily.reduce((sum, day) => sum + day.detail_requests, 0)}</strong></span><span><i class="usage-dot other-dot"></i>Other calls <strong>{usage.daily.reduce((sum, day) => sum + day.other_requests, 0)}</strong></span><span>Rate limited <strong>{usage.daily.reduce((sum, day) => sum + day.denied_requests, 0)}</strong></span></div>
				</section>
			{/if}
			<section class="soft-card memory-impact" aria-labelledby="memory-impact-heading" aria-busy={analyticsBusy}>
				<div class="impact-heading">
					<div><span class="section-label">MEMORY IMPACT</span><h2 id="memory-impact-heading">How your memories help other agents</h2><p>Daily aggregate reads by authored version. Reader identities and URLs are not recorded in this chart.</p></div>
					<div class="impact-controls"><label for="impact-bucket">Group by</label><select id="impact-bucket" bind:value={analyticsBucket}><option value="day">Day</option><option value="week">Week</option></select><button class="button button-light button-small" type="button" onclick={reloadMemoryAnalytics} disabled={analyticsBusy || busy}>{analyticsBusy ? 'Updating…' : 'Update'}</button></div>
				</div>
				{#if analyticsError}<p class="rank-error" role="alert">{analyticsError}</p>{/if}
				{#if memoryAnalytics?.memories.length}
					<p class="impact-range">{memoryAnalytics.bucket === 'day' ? 'Daily' : 'Weekly'} · {memoryAnalytics.from_date} to {memoryAnalytics.to_date} UTC</p>
					<div class="impact-memory-list">
						{#each memoryAnalytics.memories as memory (memory.version_id)}
							<article class="impact-memory">
								<div class="impact-memory-heading"><div><h3>{memory.title}</h3><p>{memory.site} · {memory.memory_id} · {memory.version_id}</p></div><a class="text-link" href={'/memories/' + encodeURIComponent(memory.memory_id)}>Open memory →</a></div>
								<div class="impact-stats" aria-label={`Usage totals for ${memory.title}`}>
									<div><span>Recall hits</span><strong>{(impactTotal(memory, 'anonymous_recall_hits') + impactTotal(memory, 'other_account_recall_hits')).toLocaleString('en-US')}</strong></div>
									<div><span>Detail views</span><strong>{(impactTotal(memory, 'anonymous_detail_views') + impactTotal(memory, 'other_account_detail_views')).toLocaleString('en-US')}</strong></div>
									<div><span>Successful uses</span><strong>{impactTotal(memory, 'success_outcomes').toLocaleString('en-US')}</strong></div>
								</div>
								<div class="impact-chart-shell">
									<div class="impact-chart-legend"><span><i class="impact-swatch recall-swatch"></i>Recall hits</span><span><i class="impact-swatch detail-swatch"></i>Detail views</span><span class="impact-max-label">Peak {activityMax(memory).toLocaleString('en-US')} reads</span></div>
									<div class="impact-chart" role="img" aria-label={`${memory.title}, ${memoryAnalytics.bucket} recall and detail reads from ${memoryAnalytics.from_date} to ${memoryAnalytics.to_date}.`} style={`--point-count:${memory.points.length}`}>
										{#each memory.points as point (point.period_start)}
											<div class="impact-chart-column" title={`${point.period_start}: ${activityFor(point)} reads · ${point.success_outcomes} successful uses`}>
												<span class="impact-bar recall-bar" style={`--bar-height:${Math.max(1, (point.anonymous_recall_hits + point.other_account_recall_hits) / activityMax(memory) * 100)}%`}></span>
												<span class="impact-bar detail-bar" style={`--bar-height:${Math.max(1, (point.anonymous_detail_views + point.other_account_detail_views) / activityMax(memory) * 100)}%`}></span>
											</div>
										{/each}
									</div>
									<div class="impact-chart-labels"><span>{memory.points[0]?.period_start}</span><span>{memory.points.at(-1)?.period_start}</span></div>
								</div>
								<details class="impact-data"><summary>View chart data</summary><div class="impact-table-wrap"><table><caption>{memory.title}: reads and outcomes by {memoryAnalytics.bucket} in UTC</caption><thead><tr><th scope="col">Period</th><th scope="col">Anonymous recall</th><th scope="col">Other-account recall</th><th scope="col">Anonymous detail</th><th scope="col">Other-account detail</th><th scope="col">Success</th><th scope="col">Partial</th><th scope="col">Failed</th></tr></thead><tbody>{#each memory.points as point (point.period_start)}<tr><th scope="row">{point.period_start}</th><td>{point.anonymous_recall_hits}</td><td>{point.other_account_recall_hits}</td><td>{point.anonymous_detail_views}</td><td>{point.other_account_detail_views}</td><td>{point.success_outcomes}</td><td>{point.partial_outcomes}</td><td>{point.failed_outcomes}</td></tr>{/each}</tbody></table></div></details>
							</article>
						{/each}
					</div>
				{:else if memoryAnalytics}
					<p class="status-empty">No authored memory versions are linked to this account yet.</p>
				{/if}
			</section>
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
	.rank-login p { margin: 8px 0 0; color: #5d6c62; font-size: 10px; }
	.rank-metrics { margin-top: 22px; }
	.usage-panel { margin-top: 16px; padding: clamp(18px, 3vw, 28px); overflow: hidden; }
	.usage-head { display: flex; justify-content: space-between; align-items: start; gap: 18px; }
	.usage-head h2 { margin: 7px 0 5px; color: #203b2d; font: 600 20px Manrope, sans-serif; letter-spacing: -.035em; }
	.usage-head p { max-width: 450px; margin: 0; color: #5d6c62; font-size: 11px; line-height: 1.7; }
	.quota-count { display: grid; gap: 3px; min-width: 125px; padding: 12px 15px; border: 1px solid #e5eadc; border-radius: 10px; background: #f7f8ef; text-align: right; }
	.quota-count strong { color: #264d3b; font: 700 25px Manrope, sans-serif; font-variant-numeric: tabular-nums; }
	.quota-count span, .quota-meta { color: #5d6c62; font-size: 10px; }
	.quota-meter { height: 8px; margin-top: 21px; overflow: hidden; border-radius: 10px; background: #e9ede5; }
	.quota-meter span { display: block; height: 100%; border-radius: inherit; background: linear-gradient(90deg, #58a376, #b1d55d); transition: width .25s ease; }
	.quota-meta { display: flex; justify-content: space-between; gap: 10px; margin-top: 7px; }
	.usage-chart-wrap { margin-top: 24px; padding: 15px 14px 10px; border: 1px solid #edf0e8; border-radius: 9px; background: linear-gradient(180deg, #fcfcf7, #f8f9f2); }
	.usage-chart-title, .usage-chart-labels { display: flex; justify-content: space-between; color: #5d6c62; font-size: 9px; }
	.usage-chart-title strong { color: #455c4d; font-size: 10px; }
	.usage-chart { display: grid; grid-template-columns: repeat(30, minmax(2px, 1fr)); align-items: end; gap: 4px; height: 76px; margin-top: 12px; border-bottom: 1px solid #dce4d7; }
	.usage-chart > span { display: block; height: var(--usage-height); min-height: 2px; border-radius: 3px 3px 0 0; background: #d8e5d4; }
	.usage-chart > span.usage-day-active { background: linear-gradient(180deg, #b6d96c, #5a9470); }
	.usage-chart-labels { margin-top: 6px; font-size: 8px; }
	.usage-breakdown { display: flex; flex-wrap: wrap; gap: 15px; margin-top: 15px; color: #5d6c62; font-size: 10px; }
	.usage-breakdown > span { display: inline-flex; align-items: center; gap: 5px; }
	.usage-breakdown strong { color: #315847; font-variant-numeric: tabular-nums; }
	.usage-dot { width: 7px; height: 7px; border-radius: 50%; background: #75a982; }
	.detail-dot { background: #b3cf60; }.other-dot { background: #aab6ae; }
	.memory-impact { margin-top: 20px; padding: clamp(18px, 3vw, 28px); }
	.impact-heading, .impact-memory-heading { display: flex; align-items: start; justify-content: space-between; gap: 16px; }
	.impact-heading h2 { margin: 7px 0 5px; color: #203b2d; font: 600 20px Manrope, sans-serif; letter-spacing: -.035em; }
	.impact-heading p, .impact-range { margin: 0; color: #5d6c62; font-size: 11px; line-height: 1.7; }
	.impact-controls { display: flex; align-items: center; gap: 8px; white-space: nowrap; }
	.impact-controls label { color: #5d6c62; font-size: 10px; }
	.impact-controls select { min-height: 34px; padding: 6px 26px 6px 9px; border: 1px solid #dce2d9; border-radius: 6px; background: #fffef9; color: #315847; font: inherit; font-size: 11px; }
	.impact-range { margin-top: 18px; font-size: 10px; }
	.impact-memory-list { display: grid; gap: 13px; margin-top: 10px; }
	.impact-memory { min-width: 0; padding: 17px; border: 1px solid #e8ece4; border-radius: 10px; background: linear-gradient(145deg, #fffefb, #f8faf3); }
	.impact-memory-heading { align-items: center; }
	.impact-memory-heading h3 { margin: 0; color: #244f3f; font: 600 15px Manrope, sans-serif; }
	.impact-memory-heading p { margin: 5px 0 0; color: #5d6c62; font-size: 10px; overflow-wrap: anywhere; }
	.impact-memory-heading .text-link { flex: none; font-size: 10px; }
	.impact-stats { display: grid; grid-template-columns: repeat(3, minmax(0,1fr)); gap: 8px; margin-top: 16px; }
	.impact-stats > div { display: grid; gap: 5px; padding: 10px 12px; border: 1px solid #edf0e8; border-radius: 7px; background: #fbfcf7; }
	.impact-stats span { color: #5d6c62; font-size: 9px; }
	.impact-stats strong { color: #315847; font: 650 17px Manrope, sans-serif; font-variant-numeric: tabular-nums; }
	.impact-chart-shell { margin-top: 15px; padding: 12px 12px 8px; border: 1px solid #edf0e8; border-radius: 8px; background: #fffefb; }
	.impact-chart-legend { display: flex; align-items: center; gap: 13px; color: #5d6c62; font-size: 9px; }
	.impact-chart-legend > span { display: inline-flex; align-items: center; gap: 5px; }
	.impact-chart-legend .impact-max-label { margin-left: auto; color: #5d6c62; }
	.impact-swatch { width: 7px; height: 7px; border-radius: 2px; background: #5a9470; }
	.detail-swatch { background: #b6d96c; }
	.impact-chart { display: grid; grid-template-columns: repeat(var(--point-count), minmax(2px,1fr)); align-items: end; gap: 3px; height: 76px; margin-top: 10px; border-bottom: 1px solid #dce4d7; }
	.impact-chart-column { display: flex; align-items: end; justify-content: center; gap: 1px; height: 100%; min-width: 0; }
	.impact-bar { display: block; width: 46%; height: var(--bar-height); min-height: 1px; border-radius: 2px 2px 0 0; }
	.recall-bar { background: linear-gradient(180deg, #a7cb6c, #578e6f); }
	.detail-bar { background: #c5d98a; }
	.impact-chart-labels { display: flex; justify-content: space-between; margin-top: 5px; color: #5d6c62; font-size: 8px; }
	.impact-data { margin-top: 12px; color: #547b63; font-size: 10px; }
	.impact-data summary { width: fit-content; cursor: pointer; }
	.impact-table-wrap { margin-top: 9px; overflow-x: auto; }
	.impact-table-wrap table { width: 100%; border-collapse: collapse; white-space: nowrap; text-align: right; font-size: 9px; }
	.impact-table-wrap caption { padding: 0 0 7px; color: #5d6c62; text-align: left; }
	.impact-table-wrap th, .impact-table-wrap td { padding: 7px 8px; border-top: 1px solid #e8ece6; }
	.impact-table-wrap thead th { color: #5d6c62; font-weight: 600; }
	.impact-table-wrap tbody th { color: #455c4d; font-weight: 600; text-align: left; }
	.metric-card { display: grid; gap: 9px; padding: 23px; }
	.metric-card span { color: #5d6c62; font-size: 11px; }
	.metric-card strong { color: #244f3f; font: 600 32px Manrope, sans-serif; }
	.metric-card small { color: #5d6c62; font-size: 10px; }
	.profile-action { margin-top: 23px; }
	.rank-error { color: #9a4f3f; font-size: 12px; }
	.score-history { margin-top: 20px; padding: 22px; }
	.score-history h2 { margin-top: 6px; color: #203b2d; font-size: 18px; }
	.history-note { color: #5d6c62; font-size: 11px; line-height: 1.7; }
	.history-table { overflow-x: auto; }
	.history-table table { width: 100%; border-collapse: collapse; text-align: left; font-size: 11px; }
	.history-table th { padding: 10px 8px; color: #5d6c62; font-weight: 600; }
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
	.status-row small, .status-empty { color: #5d6c62; font-size: 10px; overflow-wrap: anywhere; }
	@media (max-width: 700px) { .rank-metrics { grid-template-columns: 1fr; } }
	@media (max-width: 520px) { .usage-head, .impact-heading, .impact-memory-heading { flex-direction: column; }.quota-count { min-width: 0; width: 100%; text-align: left; }.usage-chart { gap: 2px; }.usage-breakdown { gap: 9px; }.impact-controls { width: 100%; }.impact-controls select { flex: 1; }.impact-stats { grid-template-columns: repeat(3,minmax(0,1fr)); }.impact-stats > div { padding: 8px; }.impact-chart { gap: 1px; }.impact-chart-legend { gap: 8px; flex-wrap: wrap; } }
</style>
