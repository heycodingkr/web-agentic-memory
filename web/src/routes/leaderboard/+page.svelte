<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type Leaderboard } from '#lib/api';
	import { publicSiteOrigin } from '#lib/site.js';

	let board = $state<Leaderboard | null>(null);
	let error = $state('');
	let busy = $state(true);

	async function loadBoard() {
		busy = true;
		error = '';
		try { board = await api<Leaderboard>('/v1/leaderboard'); }
		catch (cause) { error = cause instanceof Error ? cause.message : 'Could not load the leaderboard.'; }
		finally { busy = false; }
	}

	onMount(() => { void loadBoard(); });
</script>

<svelte:head>
	<title>Leaderboard — Web Agentic Memory</title>
	<meta name="description" content="Ranks for contributors who chose to appear publicly." />
	<meta property="og:type" content="website" />
	<meta property="og:title" content="Public contributor leaderboard — Web Agentic Memory" />
	<meta property="og:description" content="See opted-in contributors who share useful website workflows." />
	<meta property="og:url" content={`${publicSiteOrigin}/leaderboard`} />
	<link rel="canonical" href={`${publicSiteOrigin}/leaderboard`} />
</svelte:head>

<section class="page-shell">
	<div class="container">
		<div class="page-heading">
			<span class="eyebrow">PUBLIC CONTRIBUTORS</span>
			<h1>People who share<br />useful workflows.</h1>
			<p>Only contributors who opt in appear here. Open a handle to read their bio and visit their service links.</p>
			<div class="period-note"><span aria-hidden="true"></span> All time · opted-in profiles</div>
		</div>

		<div class="leaderboard-intro">
			<div class="leaderboard-copy">
				<span class="section-label">HOW POINTS ARE EARNED</span>
				<h2>Useful paths deserve<br />visible credit.</h2>
				<p>A new workflow that passes verification earns its author 20 points. Eligible successful-use reports from another signed-in account add 2. Views, anonymous use, and self-use do not earn points.</p>
			</div>
			<div class="leaderboard-policy">
				<div><span>Opt-in profiles</span><p>Only accounts that choose public handle and point totals appear here.</p></div>
				<div><span>Private overall rank</span><p>Your overall rank is visible only to you in your signed-in dashboard.</p></div>
				<a href="/dashboard">View my rank and activity <span aria-hidden="true">↗</span></a>
			</div>
		</div>

		<div class="leaderboard-controls"><h2>All-time ranking</h2><span>Tied point totals share a rank.</span></div>
		<div class="data-table-wrap leaderboard-table-wrap">
			{#if busy}
				<div class="leaderboard-state" role="status"><span class="state-mark" aria-hidden="true"></span><p>Loading public ranks…</p></div>
			{:else if error}
				<div class="leaderboard-state" role="alert"><p>{error}</p><button class="button button-light button-small" type="button" onclick={loadBoard}>Retry</button></div>
			{:else if board?.entries.length}
				<table class="leaderboard-table">
					<caption class="sr-only">All-time public contributor ranking</caption>
					<thead><tr><th>Rank</th><th>Contributor</th><th>Total points</th></tr></thead>
					<tbody>
						{#each board.entries as person}
							<tr class:rank-leader={person.rank === 1}>
								<td class="leader-rank"><span>{String(person.rank).padStart(2, '0')}</span></td>
								<td><a class="contributor-cell contributor-link" href={'/u/' + encodeURIComponent(person.handle)}><span class="avatar">{person.handle.slice(0, 1).toUpperCase()}</span><span><strong>@{person.handle}</strong><small>View bio and links ↗</small></span></a></td>
								<td class="score">{person.total_points.toLocaleString('en-US')}<span>pts</span></td>
							</tr>
						{/each}
					</tbody>
				</table>
			{:else}
				<div class="leaderboard-state leaderboard-empty" role="status">
					<span class="empty-rule" aria-hidden="true"></span>
					<h3>No public contributors yet.</h3>
					<p>Start contributing and choose whether to appear in your profile settings.</p>
					<a class="button button-dark button-small" href="/contribute">How to contribute <span aria-hidden="true">↗</span></a>
				</div>
			{/if}
		</div>
		<p class="leaderboard-footnote">The public board shows handles and point totals only. Email addresses and internal account details stay private.</p>
	</div>
</section>

<style>
	.leaderboard-intro { display: grid; grid-template-columns: minmax(0, 1.15fr) minmax(270px, .85fr); gap: clamp(24px, 5vw, 70px); padding: clamp(25px, 4vw, 42px); border-radius: 15px; color: #f3f5e9; background: #17372e; }
	.leaderboard-copy .section-label { color: #b4c7ad; }
	.leaderboard-copy h2 { margin: 15px 0 12px; color: #f6f5ea; font: 600 clamp(26px, 3.4vw, 38px)/1.15 Manrope, sans-serif; letter-spacing: -.055em; }
	.leaderboard-copy p { max-width: 520px; margin: 0; color: #c2d0c4; font-size: 13px; line-height: 1.8; }
	.leaderboard-policy { display: grid; align-content: center; gap: 16px; }
	.leaderboard-policy > div { padding: 0 0 15px; border-bottom: 1px solid rgba(237, 244, 225, .18); }
	.leaderboard-policy span { color: #d8ef78; font-size: 12px; font-weight: 700; }
	.leaderboard-policy p { margin: 6px 0 0; color: #c2d0c4; font-size: 11px; line-height: 1.7; }
	.leaderboard-policy > a { display: flex; justify-content: space-between; align-items: center; color: #f4f5eb; font-size: 12px; font-weight: 700; }
	.period-note { display: inline-flex; align-items: center; gap: 9px; margin-top: 18px; color: #69796f; font-size: 11px; }
	.period-note span { width: 7px; height: 7px; border-radius: 50%; background: #829f60; }
	.leaderboard-controls { display: flex; justify-content: space-between; align-items: baseline; gap: 14px; margin: 36px 0 13px; }
	.leaderboard-controls h2 { margin: 0; font: 600 19px Manrope, sans-serif; letter-spacing: -.035em; }
	.leaderboard-controls > span { color: #829087; font-size: 11px; }
	.leaderboard-table-wrap { overflow: hidden; border-radius: 12px; }
	.leaderboard-table tbody tr { height: 76px; transition: background .15s ease; }
	.leaderboard-table tbody tr.rank-leader { background: #f1f5e8; }
	.leaderboard-table .avatar { display: grid; place-items: center; width: 42px; height: 42px; border-radius: 13px; color: #f6f5eb; background: #315d4a; font: 700 14px Manrope, sans-serif; }
	.leader-rank span { display: inline-grid; place-items: center; min-width: 31px; min-height: 31px; border: 1px solid #e0e7dc; border-radius: 50%; }
	.rank-leader .leader-rank span { border-color: #b7cd6a; color: #34513b; background: #e5efbc; }
	.contributor-link { display: inline-flex; align-items: center; gap: 12px; }
	.contributor-link span:last-child { display: grid; gap: 4px; }
	.contributor-link small { color: #859189; font-size: 10px; }
	.leaderboard-table .score { text-align: right; font-size: 15px !important; font-variant-numeric: tabular-nums; }
	.leaderboard-table .score span { margin-left: 5px; color: #89938a; font: 10px 'DM Sans', sans-serif; }
	.leaderboard-state { display: grid; justify-items: start; gap: 11px; padding: 28px; color: #66766b; font-size: 12px; }
	.leaderboard-state p { margin: 0; line-height: 1.7; }
	.leaderboard-empty { justify-items: center; padding: 56px 24px; text-align: center; }
	.leaderboard-empty h3 { margin: 4px 0 0; font: 600 21px Manrope, sans-serif; letter-spacing: -.04em; }
	.leaderboard-empty p { max-width: 400px; color: #7a887e; }
	.empty-rule { width: 42px; height: 2px; background: #9bb15e; }
	.leaderboard-footnote { margin: 13px 0 0; color: #879189; font-size: 10px; line-height: 1.7; }
	@media (max-width: 760px) { .leaderboard-intro { grid-template-columns: 1fr; gap: 25px; } }
	@media (max-width: 480px) { .leaderboard-controls { align-items: start; flex-direction: column; } .leaderboard-table th, .leaderboard-table td { padding: 11px 9px; } .leaderboard-table tbody tr { height: 68px; } .leaderboard-table .avatar { width: 35px; height: 35px; border-radius: 11px; } .contributor-link { gap: 8px; } .contributor-link small { display: none; } .leaderboard-state { padding: 22px 16px; } }
</style>
