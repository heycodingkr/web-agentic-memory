<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api, ApiError, type Contribution, type ContributionList, type Dashboard, type MemoryAnalytics, type OwnerUsage, type ScoreEventList } from '#lib/api';
  import { browserSession, loadBrowserSession, setBrowserSession } from '#lib/browser-session';
  import { contributionDescription, contributionStatus, contributionTitle, impactTotals, pointReason, relativeTime, type ContributionFilter } from '#lib/my-page';
  import MyPageIcon from '#lib/components/MyPageIcon.svelte';
  import MyContributions from '#lib/components/MyContributions.svelte';
  import MyMemoryImpact from '#lib/components/MyMemoryImpact.svelte';
  import ContributionBadge from '#lib/components/ContributionBadge.svelte';

  type Panel = 'summary' | 'contributions' | 'impact' | 'history' | 'usage';
  type Tab = 'contributions' | 'memories' | 'impact' | 'history';
  const keys: Panel[] = ['summary', 'contributions', 'impact', 'history', 'usage'];
  const tabs: { key: Tab; label: string; icon: string }[] = [{ key: 'contributions', label: 'My contributions', icon: 'memory' }, { key: 'memories', label: 'My memories', icon: 'memory' }, { key: 'impact', label: 'Memory impact', icon: 'chart' }, { key: 'history', label: 'Point history', icon: 'history' }];
  const isPreview = import.meta.env.DEV && import.meta.env.VITE_WAM_API_URL === '/__preview';
  let previewCase = $state('populated');
  let dashboard = $state<Dashboard | null>(null);
  let contributions = $state<ContributionList['contributions']>([]);
  let contributionList = $state<ContributionList | null>(null);
  let memoryAnalytics = $state<MemoryAnalytics | null>(null);
  let usage = $state<OwnerUsage | null>(null);
  let scoreEvents = $state<ScoreEventList['events']>([]);
  let scoreCursor = $state<string | null>(null);
  let errors = $state<Record<Panel, string>>({ summary: '', contributions: '', impact: '', history: '', usage: '' });
  let loading = $state<Record<Panel, boolean>>({ summary: false, contributions: false, impact: false, history: false, usage: false });
  let retryAt = $state<Record<Panel, number>>({ summary: 0, contributions: 0, impact: 0, history: 0, usage: 0 });
  let checkingSession = $state(true);
  let signedIn = $state(false);
  let pageBusy = $state(false);
  let authError = $state('');
  let token = $state('');
  let tab = $state<Tab>('contributions');
  let filter = $state<ContributionFilter>('all');
  let period = $state(30);
  let bucket = $state<'day' | 'week'>('day');
  let lastRead = $state<number | null>(null);
  let now = $state(Date.now());
  let loadGeneration = 0;
  let impactGeneration = 0;
  let selected = $state<Contribution | null>(null);
  let detailLoading = $state(false);
  let detailError = $state('');
  let detailDialog: HTMLDialogElement;

  const listComplete = $derived(!!contributionList && !contributionList.next_cursor && (contributionList.total_count === undefined || contributionList.total_count <= contributions.length));
  const pending = $derived(dashboard?.pending_contributions ?? (listComplete ? contributions.filter(c => contributionStatus(c).pending).length : undefined));
  const actionCount = $derived(dashboard?.action_required_contributions ?? (listComplete ? contributions.filter(c => contributionStatus(c).action).length : undefined));
  const attention = $derived(contributions.find(c => contributionStatus(c).action));
  const totals = $derived(impactTotals(memoryAnalytics));
  function retrySeconds(key: Panel) { return Math.max(0, Math.ceil((retryAt[key] - now) / 1000)); }
  function options() { return isPreview ? { headers: { 'x-wam-preview-case': previewCase } } : {}; }
  function message(cause: unknown) {
    if (cause instanceof ApiError) {
      if (cause.status === 429) return `Rate limit reached. Try again${cause.retryAfterSeconds ? ` in ${cause.retryAfterSeconds} seconds` : ' shortly'}.`;
      if (cause.status === 403) return 'You do not have permission to view this section.';
      if (cause.status === 503) return 'This section is temporarily unavailable. Try again later.';
    }
    return 'Could not load this section. Check your connection and try again.';
  }
  function expireAuthentication(text = 'Your session expired. Sign in again to continue.') {
    ++loadGeneration; ++impactGeneration;
    signedIn = false; pageBusy = false; authError = text;
    dashboard = null; contributions = []; contributionList = null; memoryAnalytics = null; usage = null;
    scoreEvents = []; scoreCursor = null; selected = null; detailLoading = false; detailError = ''; lastRead = null;
    for (const key of keys) { loading[key] = false; errors[key] = ''; retryAt[key] = 0; }
    detailDialog?.close(); setBrowserSession(null);
  }
  async function fetchPanel<T>(key: Panel, path: string, apply: (value: T) => void, generation = loadGeneration, current = () => true) {
    if (retryAt[key] > Date.now()) return;
    loading[key] = true; errors[key] = '';
    try {
      const value = await api<T>(path, options(), token || undefined);
      if (generation !== loadGeneration || !current()) return;
      apply(value); retryAt[key] = 0; lastRead = Date.now();
    } catch (cause) {
      if (generation !== loadGeneration || !current()) return;
      if (cause instanceof ApiError && cause.status === 401) expireAuthentication();
      else { errors[key] = message(cause); retryAt[key] = cause instanceof ApiError && cause.status === 429 ? Date.now() + (cause.retryAfterSeconds || 15) * 1000 : 0; }
    } finally { if (generation === loadGeneration && current()) loading[key] = false; }
  }
  function loadContributions(generation = loadGeneration) {
    return fetchPanel<ContributionList>('contributions', '/v1/me/contributions', value => { contributions = value.contributions; contributionList = value; }, generation);
  }
  function loadImpact(generation = loadGeneration) {
    const request = ++impactGeneration;
    const to = new Date(); to.setUTCHours(0, 0, 0, 0);
    const from = new Date(to); from.setUTCDate(from.getUTCDate() - period + 1);
    return fetchPanel<MemoryAnalytics>('impact', `/v1/me/memories/analytics?from=${from.toISOString().slice(0, 10)}&to=${to.toISOString().slice(0, 10)}&bucket=${bucket}`, value => memoryAnalytics = value, generation, () => request === impactGeneration);
  }
  async function loadDashboard() {
    const generation = ++loadGeneration;
    pageBusy = true;
    await Promise.allSettled([
      fetchPanel<Dashboard>('summary', '/v1/me/dashboard', value => dashboard = value, generation),
      loadContributions(generation), loadImpact(generation),
      fetchPanel<ScoreEventList>('history', '/v1/me/score-events', value => { scoreEvents = value.events; scoreCursor = value.next_cursor; }, generation),
      fetchPanel<OwnerUsage>('usage', '/v1/me/usage', value => usage = value, generation)
    ]);
    if (generation === loadGeneration) pageBusy = false;
  }
  async function loadMoreHistory() {
    if (!scoreCursor || loading.history || pageBusy) return;
    await fetchPanel<ScoreEventList>('history', '/v1/me/score-events?before=' + encodeURIComponent(scoreCursor), value => {
      const existing = new Set(scoreEvents.map(e => e.id));
      scoreEvents = [...scoreEvents, ...value.events.filter(e => !existing.has(e.id))]; scoreCursor = value.next_cursor;
    });
  }
  function selectSummary(next: ContributionFilter) { tab = 'contributions'; filter = next; }
  function changeRange(days: number, grouping: 'day' | 'week') { period = days; bucket = grouping; void loadImpact(); }
  async function openDetails(item: Contribution) {
    selected = item; detailError = ''; detailLoading = true;
    const generation = loadGeneration;
    await tick(); detailDialog.showModal();
    try {
      const result = await api<Contribution>('/v1/me/contributions/' + encodeURIComponent(item.id), options(), token || undefined);
      if (generation === loadGeneration && selected?.id === item.id) selected = result;
    } catch (cause) {
      if (generation !== loadGeneration) return;
      if (cause instanceof ApiError && cause.status === 401) expireAuthentication();
      else detailError = 'Could not refresh the details. The last loaded submission is shown.';
    } finally { if (generation === loadGeneration) detailLoading = false; }
  }
  onMount(() => {
    if (isPreview) previewCase = new URLSearchParams(window.location.search).get('preview') || 'populated';
    void (async () => {
      try {
        const session = isPreview && previewCase === 'signed-out' ? await api('/v1/auth/session', options()) : await loadBrowserSession();
        if (session) { signedIn = true; await loadDashboard(); }
      } catch (cause) {
        if (cause instanceof ApiError && cause.status === 401) expireAuthentication('Sign in to see your contributions, memories, and points.');
        else authError = 'Could not check your session. Refresh the page to try again.';
      }
      finally { checkingSession = false; }
    })();
    const unsubscribe = browserSession.subscribe(session => { if (!session && signedIn && !token) expireAuthentication(); });
    const clock = window.setInterval(() => { now = Date.now(); }, 1000);
    const poll = window.setInterval(() => {
      if (signedIn && !pageBusy && document.visibilityState === 'visible' && (pending || 0) > 0 && !loading.contributions && !errors.contributions) {
        void loadContributions();
        void fetchPanel<Dashboard>('summary', '/v1/me/dashboard', value => dashboard = value);
      }
    }, 30000);
    return () => { ++loadGeneration; unsubscribe(); window.clearInterval(clock); window.clearInterval(poll); };
  });
</script>

<svelte:head><title>My Page — Web Agentic Memory</title><meta name="description" content="Track your contributions, published memories, points, and memory impact."/><meta name="robots" content="noindex, nofollow"/></svelte:head>

<section class="mypage">
  <div class="mypage-shell">
    {#if isPreview}<aside class="preview-banner" aria-label="Local UI preview"><span><span class="preview-dot"></span>Local UI preview <strong>Sample data</strong></span><div><label><span class="sr-only">Preview dataset</span><select aria-label="Preview dataset" value={previewCase} onchange={e => window.location.assign('/dashboard?preview=' + e.currentTarget.value)}><option value="populated">All states</option><option value="empty">New account</option><option value="error">Analytics unavailable</option><option value="signed-out">Signed out</option></select></label><a href="/__preview/reset">Reset</a></div></aside>{/if}
    <header class="page-top"><div><span class="section-kicker">YOUR CONTRIBUTOR SPACE</span><h1>My Page<span class="title-dot">.</span></h1><p>Your workflows, their progress, and the useful paths they leave.</p></div><div class="page-actions"><button class="refresh-button" type="button" aria-label="Refresh My Page" onclick={loadDashboard} disabled={pageBusy || !signedIn}><MyPageIcon name="refresh" size={15}/><span>{pageBusy ? 'Refreshing…' : 'Refresh'}</span></button><a class="button button-dark button-small" href="/contribute"><MyPageIcon name="plus" size={15}/>Submit a contribution</a></div></header>
    {#if checkingSession}<section class="session-state" role="status"><span class="loading-dot"></span>Checking your sign-in…</section>
    {:else if !signedIn}<section class="session-state signed-out"><span class="welcome-icon"><MyPageIcon name="user" size={28}/></span><h2>Your contributions belong here.</h2><p>Sign in to follow your submissions, open your memories, and see your points.</p>{#if authError}<p class="auth-error" role="alert">{authError}</p>{/if}<a class="button button-dark" href="/login">Sign in with email <MyPageIcon name="arrow" size={16}/></a>
    {#if import.meta.env.DEV && !isPreview}<details class="demo-login"><summary>Local development access</summary><label for="rank-token">Local demo token</label><input id="rank-token" type="password" bind:value={token} autocomplete="off"/><button type="button" onclick={() => { signedIn = true; void loadDashboard(); }} disabled={!token}>View demo account</button></details>{/if}</section>
    {:else}
      {#if errors.summary}<div class="summary-error" role="alert">{errors.summary}<button type="button" onclick={loadDashboard} disabled={loading.summary || retrySeconds('summary') > 0}>{retrySeconds('summary') > 0 ? `Try again in ${retrySeconds('summary')}s` : 'Try again'}</button></div>{/if}
      <div class="metric-grid" aria-label="My rank and points">
        <article class="overview-metric featured"><div class="metric-label"><MyPageIcon name="points" size={19}/><span>Total points</span></div><strong>{dashboard ? dashboard.total_points.toLocaleString('en-US') : '—'}</strong><small>Server-recorded lifetime total</small></article>
        <article class="overview-metric"><div class="metric-label"><MyPageIcon name="rank" size={19}/><span>Overall rank</span></div><strong>{dashboard?.overall_rank ? '#' + dashboard.overall_rank : '—'}</strong><small>{dashboard ? dashboard.overall_rank ? 'Among eligible accounts' : 'No rank yet' : 'Loading your rank'}</small></article>
        <button class="overview-metric metric-button" type="button" onclick={() => tab = 'memories'}><span class="metric-label"><MyPageIcon name="memory" size={19}/><span>{dashboard?.published_memories !== undefined ? 'Published memories' : 'Authored versions'}</span></span><strong>{dashboard?.published_memories ?? (memoryAnalytics ? memoryAnalytics.memories.length : '—')}</strong><small>{dashboard?.published_memories !== undefined ? 'Current published memories' : 'Linked to your account'}</small></button>
        <button class="overview-metric metric-button" type="button" onclick={() => selectSummary('pending')}><span class="metric-label"><MyPageIcon name="clock" size={19}/><span>Pending</span></span><strong>{pending ?? '—'}</strong><small>Queued or being checked</small></button>
        <button class="overview-metric metric-button" class:attention-card={(actionCount || 0) > 0} type="button" onclick={() => selectSummary('action')}><span class="metric-label"><MyPageIcon name="alert" size={19}/><span>Needs your action</span></span><strong>{actionCount ?? '—'}</strong><small>{actionCount === undefined ? 'Status not yet available' : actionCount ? 'A little input from you' : 'You are all caught up'}</small></button>
      </div>
      <div class="rank-context"><span><span class="private-dot"></span>Your overall rank stays private. <a href="/settings/profile">Manage public visibility</a>{#if dashboard?.public_rank} · Public rank #{dashboard.public_rank}{/if}</span><span>{lastRead ? 'Last refreshed ' + new Date(lastRead).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' }) : 'Loading your workspace'}</span></div>
      <div class="mypage-tabs" role="tablist" aria-label="My Page sections">{#each tabs as option}<button type="button" role="tab" id={'tab-' + option.key} aria-selected={tab === option.key} aria-controls="my-page-panel" tabindex={tab === option.key ? 0 : -1} onclick={() => tab = option.key} onkeydown={event => { if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) { event.preventDefault(); const index = tabs.findIndex(t => t.key === tab); const next = event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length; tab = tabs[next].key; void tick().then(() => document.getElementById('tab-' + tab)?.focus()); } }}><MyPageIcon name={option.icon} size={16}/>{option.label}{#if option.key === 'contributions' && contributions.length}<span>{contributionList?.total_count ?? contributions.length}</span>{/if}</button>{/each}</div>
      <div role="tabpanel" id="my-page-panel" aria-labelledby={'tab-' + tab}>
        {#if tab === 'contributions'}
          <div class="workspace-grid"><MyContributions items={contributions} loading={loading.contributions} error={errors.contributions} retryIn={retrySeconds('contributions')} retry={() => void loadContributions()} onview={openDetails} bind:filter/>
            <aside class="workspace-sidebar" aria-label="Contribution guidance">
              {#if attention}<section class="attention-panel"><span class="aside-kicker"><MyPageIcon name="alert" size={16}/>Needs your action</span><h2>{contributionTitle(attention)}</h2><p>Your sharing consent needs a review before this workflow can move forward.</p><button type="button" class="attention-button" onclick={() => void openDetails(attention)}>Review submission <MyPageIcon name="arrow" size={15}/></button><small>This concerns sharing your workflow, not access to an external account.</small></section>
              {:else if contributionList && !errors.contributions}<section class="caught-up"><span><MyPageIcon name="check" size={19}/></span><h2>You're all caught up.</h2><p>We'll show anything that needs your input here.</p></section>{/if}
              <section class="points-guide"><h2>How points work</h2><div><span class="guide-icon"><MyPageIcon name="memory" size={18}/></span><p><strong>+20 for a published version</strong><span>After a new or improved workflow is verified and published.</span></p></div><div><span class="guide-icon"><MyPageIcon name="check" size={18}/></span><p><strong>+2 for an eligible success</strong><span>When another signed-in account reports using your version successfully.</span></p></div><div><span class="guide-icon muted-icon"><MyPageIcon name="clock" size={18}/></span><p><strong>Reads alone earn no points</strong><span>Support, self-use, and anonymous reads do not create a new publication credit.</span></p></div><a href="/leaderboard">Explore the leaderboard <MyPageIcon name="arrow" size={14}/></a></section>
            </aside>
          </div>
          <MyMemoryImpact data={memoryAnalytics} loading={loading.impact} error={errors.impact} retryIn={retrySeconds('impact')} retry={() => void loadImpact()} mode="summary" {period} {bucket} onrange={changeRange}/>
        {:else if tab === 'memories' || tab === 'impact'}<MyMemoryImpact data={memoryAnalytics} loading={loading.impact} error={errors.impact} retryIn={retrySeconds('impact')} retry={() => void loadImpact()} mode={tab === 'memories' ? 'memories' : 'impact'} {period} {bucket} onrange={changeRange}/>
        {:else}
          <section class="history-panel" aria-labelledby="history-heading" aria-busy={loading.history}><div class="history-heading"><div><span class="section-kicker">EVERY CREDIT HAS A REASON</span><h2 id="history-heading">My point history</h2></div><span class="ledger-note">Lifetime total lives in the summary above</span></div><p>Publication credits, reported successes, and reversals. API usage units are separate.</p>
            {#if errors.history}<p class="panel-error" role="alert">{errors.history}</p>{/if}
            {#if loading.history && !scoreEvents.length}<p role="status">Loading point history…</p>{:else if scoreEvents.length}<div class="history-scroll"><table><caption class="sr-only">My point history, newest first</caption><thead><tr><th scope="col">Reason and memory</th><th scope="col">Date</th><th scope="col">Points</th></tr></thead><tbody>{#each scoreEvents as event (event.id)}<tr><td><strong>{pointReason(event)}</strong><a href={'/memories/' + encodeURIComponent(event.memory_id)}>{event.memory_id} · {event.memory_version_id}</a></td><td><time datetime={new Date(event.created_at * 1000).toISOString()}>{new Date(event.created_at * 1000).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })}</time></td><td class="credit" class:negative={event.points_delta < 0}>{event.points_delta > 0 ? '+' : ''}{event.points_delta}</td></tr>{/each}</tbody></table></div>{:else if !errors.history}<div class="history-empty"><MyPageIcon name="points" size={30}/><h3>Your first credit is ahead of you.</h3><p>Published workflows and eligible success reports will appear here.</p></div>{/if}
            {#if scoreCursor}<button class="button button-light button-small" type="button" onclick={loadMoreHistory} disabled={loading.history || retrySeconds('history') > 0}>{loading.history ? 'Loading…' : 'Load older entries'}</button>{/if}
          </section>
        {/if}
      </div>
      <details class="usage-panel"><summary><span><MyPageIcon name="chart" size={17}/><strong>Your agent activity</strong><small>Your own API calls and request allowance</small></span><span class="usage-toggle">View usage <span aria-hidden="true">⌄</span></span></summary><div class="usage-body">{#if errors.usage}<p class="panel-error" role="alert">{errors.usage}<button type="button" onclick={() => void fetchPanel<OwnerUsage>('usage', '/v1/me/usage', value => usage = value)} disabled={loading.usage || retrySeconds('usage') > 0}>{retrySeconds('usage') > 0 ? `Try again in ${retrySeconds('usage')}s` : 'Try again'}</button></p>{/if}{#if usage}<div class="quota-heading"><div><h2>Your personal API allowance</h2><p>This is your own agent's activity, separate from other accounts reading your memories.</p></div><strong>{usage.remaining_units}<small> / {usage.burst_capacity} burst units left</small></strong></div><div class="quota-meter" role="progressbar" aria-label="Burst capacity remaining" aria-valuemin="0" aria-valuemax={usage.burst_capacity} aria-valuenow={usage.remaining_units}><span style={`width:${Math.min(100, usage.remaining_units / Math.max(1, usage.burst_capacity) * 100)}%`}></span></div><div class="quota-meta"><span>{usage.limit_per_minute} units/minute</span><span>Full refill in {usage.resets_in_seconds}s</span></div><div class="usage-counts"><span><strong>{usage.daily.reduce((n, d) => n + d.requests, 0)}</strong> admitted requests</span><span><strong>{usage.daily.reduce((n, d) => n + d.denied_requests, 0)}</strong> rate limited</span><span>Last {usage.period_days} UTC dates</span></div>{:else if loading.usage}<p role="status">Loading API activity…</p>{/if}</div></details>
    {/if}
  </div>
</section>

<dialog bind:this={detailDialog} class="contribution-dialog" onclose={() => selected = null} aria-labelledby="detail-heading">
  {#if selected}{@const status = contributionStatus(selected)}<div class="dialog-head"><span class="section-kicker">CONTRIBUTION DETAILS</span><button type="button" class="dialog-close" aria-label="Close contribution details" onclick={() => detailDialog.close()}><MyPageIcon name="close" size={19}/></button></div><h2 id="detail-heading">{contributionTitle(selected)}</h2><ContributionBadge item={selected}/><p class="detail-reason">{contributionDescription(selected)}</p>{#if detailLoading}<p class="detail-loading" role="status">Refreshing submission…</p>{/if}{#if detailError}<p class="panel-error" role="alert">{detailError}</p>{/if}
    <dl><div><dt>Submission</dt><dd>{selected.id}</dd></div><div><dt>Submitted</dt><dd>{new Date(selected.submitted_at * 1000).toLocaleString('en-US')}</dd></div><div><dt>Last status update</dt><dd>{selected.progress?.updated_at ? new Date(selected.progress.updated_at * 1000).toLocaleString('en-US') : 'Not recorded'}</dd></div>{#if selected.progress?.next_attempt_at}<div><dt>Next attempt</dt><dd>{new Date(selected.progress.next_attempt_at * 1000).toLocaleString('en-US')}<small>This is not an estimated completion time.</small></dd></div>{/if}{#if selected.reflection}<div><dt>Result</dt><dd>{selected.reflection.kind === 'supported' ? 'Supports an existing memory' : selected.reflection.kind === 'updated' ? 'Improved version published' : 'New version published'}{#if selected.reflection.version_id}<small>Version {selected.reflection.version_id}</small>{/if}</dd></div>{/if}</dl>
    {#if selected.reflection?.availability === 'superseded'}<p class="detail-footnote">A newer version of this memory is current.</p>{:else if selected.reflection?.availability === 'retired'}<p class="detail-footnote">This memory version is no longer public.</p>{/if}
    <div class="dialog-actions">{#if selected.reflection?.memory_id && selected.reflection.availability !== 'retired'}<a class="button button-dark button-small" href={'/memories/' + encodeURIComponent(selected.reflection.memory_id)}>Open memory <MyPageIcon name="arrow" size={15}/></a>{/if}<a class="button button-light button-small" href="/contribute">{status.action ? 'Review and resubmit' : 'Manage submissions'}</a></div>{#if status.action}<p class="detail-footnote">Changing your sharing consent does not connect an external service account.</p>{/if}
  {/if}
</dialog>

<style>
.mypage{padding:28px 0 20px}.mypage-shell{width:min(1220px,calc(100% - 64px));margin:0 auto}.preview-banner{display:flex;justify-content:space-between;align-items:center;gap:12px;margin-bottom:20px;padding:9px 12px;border:1px dashed #cad4ba;border-radius:7px;background:#f1f5e8;color:#4b5d40;font-size:10px}.preview-banner>span,.preview-banner>div{display:flex;align-items:center;gap:8px}.preview-banner strong{padding:3px 6px;border-radius:4px;background:#e5edda;font-size:9px;font-weight:600}.preview-dot{width:5px;height:5px;border-radius:50%;background:#8da960}.preview-banner select{max-width:180px;padding:4px 6px;border:1px solid #d6dfcb;border-radius:4px;background:#fafcf5;color:#5c6c50;font-size:10px}.preview-banner a{text-decoration:underline}.page-top{display:flex;justify-content:space-between;align-items:center;gap:24px;margin-bottom:23px}.section-kicker{color:#52634f;font:500 9px 'DM Mono',monospace;letter-spacing:.09em}.page-top h1{margin:11px 0 10px;font:650 42px/1.05 Manrope,sans-serif;letter-spacing:-.065em}.title-dot{color:#8ca556}.page-top p{margin:0;color:#52634f;font-size:12px;line-height:1.7}.page-actions{display:flex;gap:10px;align-items:center}.page-actions .button{min-height:38px;padding:0 14px;font-size:11px;box-shadow:0 4px 9px #173b2c0a}.refresh-button{display:flex;align-items:center;gap:7px;min-height:37px;padding:0 11px;border:1px solid #dfe5d7;border-radius:6px;background:#fafbf5;color:#5f725f;font-size:10px;cursor:pointer}.refresh-button:disabled{opacity:.5;cursor:default}.metric-grid{display:grid;grid-template-columns:repeat(5,minmax(0,1fr));gap:12px}.overview-metric{display:flex;flex-direction:column;align-items:flex-start;min-width:0;min-height:128px;padding:17px 18px;border:1px solid #dfe5d9;border-radius:10px;background:var(--paper);text-align:left;box-shadow:0 3px 12px #223e2702}.metric-label{display:flex;align-items:center;gap:8px;color:#52634f;font-size:12px;font-weight:500}.metric-label :global(svg){flex:none}.overview-metric>strong{display:block;margin:14px 0 8px;color:#204b35;font:650 31px/1 Manrope,sans-serif;letter-spacing:-.04em}.overview-metric small{font-size:11px;color:#52634f;line-height:1.5}.overview-metric.featured{background:#edf3e3;border-color:#dbe5cc}.overview-metric.attention-card{background:#fcf3e9;border-color:#efdec9}.attention-card>strong,.attention-card .metric-label{color:#9b5b36}.metric-button{cursor:pointer;font-family:inherit}.metric-button:hover{border-color:#adc09c;box-shadow:0 4px 14px #29452a08}.rank-context{display:flex;justify-content:space-between;gap:16px;margin:13px 0 25px;color:#52634f;font-size:9px;line-height:1.7}.rank-context a{color:#556f50;text-decoration:underline;text-underline-offset:2px}.private-dot{display:inline-block;width:5px;height:5px;margin-right:5px;border-radius:50%;background:#afbc9b}.mypage-tabs{display:flex;flex-wrap:nowrap;gap:20px;margin-bottom:22px;border-bottom:1px solid #dce3d2}.mypage-tabs>button{display:flex;flex-shrink:0;align-items:center;gap:9px;position:relative;padding:13px 5px 15px;border:0;border-bottom:2px solid transparent;background:transparent;color:#52634f;font:550 12px Manrope,sans-serif;cursor:pointer;white-space:nowrap}.mypage-tabs>button[aria-selected=true]{color:#244e36;border-bottom-color:#456c38;font-weight:700}.mypage-tabs>button>span{padding:2px 5px;border-radius:4px;background:#e9eee1;color:#49673b;font:500 9px 'DM Mono',monospace}.workspace-grid{display:grid;grid-template-columns:minmax(0,1fr) 260px;gap:18px;align-items:start}.workspace-sidebar{display:grid;gap:15px}.attention-panel{padding:19px;border:1px solid #eedac6;border-radius:11px;background:#fcf3e8}.aside-kicker{display:flex;align-items:center;gap:7px;color:#9a5c39;font-size:11px;font-weight:650}.attention-panel h2{margin:18px 0 10px;color:#5c4b35;font:650 16px/1.5 Manrope,sans-serif;letter-spacing:-.035em}.attention-panel p{margin:0;color:#715c46;font-size:11px;line-height:1.7}.attention-button{display:flex;justify-content:space-between;align-items:center;gap:10px;width:100%;margin-top:18px;padding:10px 12px;border:1px solid #dac8b2;border-radius:6px;background:#fffaf1;color:#69583f;font-size:10px;font-weight:650;cursor:pointer}.attention-panel>small{display:block;margin-top:12px;font-size:9px;color:#715c46;line-height:1.6}.points-guide{padding:20px;border:1px solid #e0e6d8;border-radius:11px;background:#fffef8}.points-guide h2{margin:0 0 5px;font:650 15px Manrope,sans-serif;letter-spacing:-.035em}.points-guide>div{display:flex;align-items:flex-start;gap:10px;padding:15px 0;border-bottom:1px solid #edf0e5}.guide-icon{color:#5c7b49;flex:none}.muted-icon{color:#8c987c}.points-guide p{display:grid;gap:6px;margin:0}.points-guide strong{color:#3c5e3e;font-size:10px;font-weight:650;line-height:1.5}.points-guide p span{color:#52634f;font-size:10px;line-height:1.6}.points-guide>a{display:flex;justify-content:space-between;gap:8px;margin-top:15px;color:#567748;font-size:10px;font-weight:600}.caught-up{padding:22px 20px;border:1px solid #dce6cf;border-radius:11px;background:#f3f7eb}.caught-up>span{color:#799562}.caught-up h2{margin:13px 0 8px;font:650 16px Manrope,sans-serif;letter-spacing:-.03em}.caught-up p{margin:0;color:#52634f;font-size:11px;line-height:1.7}.usage-panel{margin-top:22px;border:1px solid #dfe5d6;border-radius:10px;background:#fafbf4}.usage-panel>summary{display:flex;justify-content:space-between;align-items:center;gap:18px;padding:18px 22px;list-style:none;cursor:pointer}.usage-panel>summary::-webkit-details-marker{display:none}.usage-panel>summary>span:first-child{display:flex;align-items:center;gap:10px}.usage-panel strong{color:#4a6845;font-size:12px;font-weight:650}.usage-panel small{color:#52634f;font-size:10px;font-weight:400}.usage-toggle{color:#52634f;font-size:10px;white-space:nowrap}.usage-toggle>span{margin-left:10px}.usage-body{padding:0 22px 22px;border-top:1px solid #e7eddd}.quota-heading{display:flex;justify-content:space-between;align-items:center;gap:20px;margin-top:22px}.quota-heading h2{margin:0 0 7px;font-size:16px}.quota-heading p{margin:0;color:#52634f;font-size:11px;line-height:1.6}.quota-heading>strong{flex:none;font-size:24px}.quota-meter{height:7px;margin-top:18px;border-radius:6px;overflow:hidden;background:#e3e9d8}.quota-meter span{display:block;height:100%;border-radius:6px;background:#9cb570}.quota-meta,.usage-counts{display:flex;justify-content:space-between;gap:16px;margin-top:10px;color:#52634f;font-size:10px}.usage-counts{justify-content:flex-start;flex-wrap:wrap;margin-top:22px}.usage-counts strong{margin-right:4px}.history-panel{padding:26px;border:1px solid #dfe5d6;border-radius:12px;background:var(--paper)}.history-heading{display:flex;justify-content:space-between;align-items:center;gap:20px}.history-heading h2{margin:8px 0;font:650 22px Manrope,sans-serif;letter-spacing:-.045em}.ledger-note{color:#52634f;font-size:10px}.history-panel>p{color:#52634f;font-size:11px;line-height:1.7}.history-scroll{max-width:100%;overflow:auto}table{width:100%;border-collapse:collapse;text-align:left}th{padding:14px 10px;border-bottom:1px solid #dfe7d4;color:#52634f;font:500 9px 'DM Mono',monospace;text-transform:uppercase}td{padding:18px 10px;border-bottom:1px solid #e9eede;font-size:11px}td strong{display:block;margin-bottom:7px;color:#4a6441;font-size:11px}td a{color:#52634f;font-size:10px}time{color:#52634f;white-space:nowrap}.credit{color:#3d6c30;font:650 18px Manrope,sans-serif;text-align:right}.credit.negative{color:#875339}.history-panel>.button{margin-top:18px}.history-empty{text-align:center;padding:50px 20px;color:#789264}.history-empty h3{margin:18px 0 8px;font:600 20px Manrope,sans-serif}.history-empty p{color:#52634f;font-size:12px}.session-state{padding:45px;border:1px solid #dfe6d4;border-radius:12px;background:var(--paper);color:#52634f;font-size:13px}.signed-out{text-align:center;max-width:620px;margin:40px auto}.signed-out h2{margin:18px 0 10px;font:650 28px Manrope,sans-serif;letter-spacing:-.055em}.signed-out p{margin:0 0 22px;line-height:1.8;font-size:13px}.welcome-icon{display:inline-grid;place-items:center;width:60px;height:60px;border-radius:16px;background:#edf3e2;color:#68854f}.summary-error,.panel-error{padding:13px;border:1px solid #ead7c0;border-radius:7px;background:#fcf2e5;color:#795330;font-size:11px;line-height:1.6}.summary-error{margin-bottom:16px}.summary-error button,.panel-error button{margin-left:10px;border:0;background:none;font:inherit;text-decoration:underline;cursor:pointer}.auth-error{color:#9c6647}.demo-login{margin-top:25px;font-size:11px}.demo-login summary{cursor:pointer}.demo-login label{display:block;margin:15px 0 8px}.demo-login input{max-width:100%;padding:8px;border:1px solid #dce5cd;border-radius:5px}.demo-login button{margin-left:8px;padding:8px;border:1px solid #dce5cd;border-radius:5px;background:#f4f7eb;font-size:11px}.loading-dot{display:inline-block;width:7px;height:7px;margin-right:8px;border-radius:50%;background:#8aab6e}.contribution-dialog{width:min(530px,calc(100% - 32px));max-height:85vh;padding:27px;border:1px solid #dce3d1;border-radius:15px;background:#fffef8;color:#294b32;box-shadow:0 30px 100px #172f272e}.contribution-dialog::backdrop{background:#16291f66;backdrop-filter:blur(3px)}.dialog-head{display:flex;align-items:center;justify-content:space-between;gap:16px}.dialog-close{display:grid;place-items:center;width:30px;height:30px;padding:0;border:0;border-radius:6px;background:#f0f4e8;color:#718267;cursor:pointer}.contribution-dialog h2{margin:18px 0;font:650 25px/1.3 Manrope,sans-serif;letter-spacing:-.045em}.detail-reason{margin:17px 0 22px;color:#52634f;font-size:12px;line-height:1.8}dl{margin:0}dl>div{padding:13px 0;border-top:1px solid #e6ecdc;display:grid;grid-template-columns:130px minmax(0,1fr);gap:15px}dt{color:#52634f;font-size:11px}dd{margin:0;color:#46663d;font-size:11px;overflow-wrap:anywhere}dd small{display:block;margin-top:5px;color:#52634f;font-size:10px}.dialog-actions{display:flex;flex-wrap:wrap;gap:8px;margin-top:23px}.detail-loading,.detail-footnote{font-size:10px;color:#52634f;line-height:1.7}.detail-footnote{margin-top:16px}.sr-only{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0,0,0,0)}
@media(max-width:1150px){.workspace-grid{grid-template-columns:minmax(0,1fr) 235px;gap:14px}.overview-metric{padding:17px 13px}.metric-label{font-size:11px;gap:6px}.overview-metric>strong{font-size:29px}.overview-metric small{font-size:10px}}
@media(max-width:900px){.mypage-shell{width:calc(100% - 40px)}.page-top{align-items:flex-start}.page-actions{flex-wrap:wrap;justify-content:flex-end}.workspace-grid{grid-template-columns:1fr}.workspace-sidebar{grid-template-columns:1fr 1fr}.metric-grid{grid-template-columns:repeat(3,minmax(0,1fr))}.overview-metric{min-height:125px}.metric-label{font-size:10px}.overview-metric small{font-size:11px}.mypage-tabs{gap:17px}.rank-context{flex-wrap:wrap}.usage-panel>summary small{display:none}}
@media(max-width:600px){.mypage{padding-top:22px}.mypage-shell{width:calc(100% - 32px)}.preview-banner{margin-bottom:18px;padding:8px 9px;gap:7px}.preview-banner>span{gap:5px;font-size:9px}.preview-banner strong{font-size:8px}.preview-banner select{max-width:115px;font-size:9px}.preview-banner a{display:none}.page-top{gap:14px;flex-direction:column;margin-bottom:19px}.page-top h1{font-size:36px;margin:10px 0}.page-top p{font-size:11px;max-width:300px}.page-actions{width:100%;justify-content:flex-start}.page-actions>.button{flex:1;justify-content:center}.refresh-button{min-height:38px}.metric-grid{grid-template-columns:repeat(6,minmax(0,1fr));gap:9px}.overview-metric{grid-column:span 2;padding:12px 10px;min-height:113px}.overview-metric:nth-child(-n+2){grid-column:span 3;min-height:112px}.overview-metric small{font-size:10px}.overview-metric>strong{margin:13px 0 7px;font-size:27px}.metric-label{font-size:11px;gap:6px;align-items:flex-start}.metric-label :global(svg){width:16px;height:16px}.rank-context{margin:12px 0 12px;gap:5px;font-size:10px}.rank-context>span:last-child{display:none}.mypage-tabs{overflow-x:auto;gap:17px;margin-bottom:18px;padding-bottom:0;scrollbar-width:none}.mypage-tabs::-webkit-scrollbar{display:none}.mypage-tabs>button{gap:7px;font-size:11px;padding:12px 0}.mypage-tabs>button :global(svg){width:14px;height:14px}.mypage-tabs>button>span{display:none}.workspace-sidebar{grid-template-columns:1fr;gap:14px}.points-guide{display:none}.attention-panel{padding:18px}.attention-panel h2{margin-top:13px;font-size:15px}.attention-panel>small{font-size:9px}.attention-panel p{font-size:11px}.caught-up{display:none}.usage-panel>summary{padding:16px}.usage-panel strong{font-size:11px}.usage-toggle{font-size:9px}.usage-body{padding:0 16px 18px}.quota-heading{flex-direction:column;align-items:flex-start;gap:15px}.quota-heading>strong{font-size:22px}.quota-meta{font-size:9px}.usage-counts{gap:12px;font-size:9px}.history-panel{padding:20px 17px}.history-heading{flex-direction:column;align-items:flex-start;gap:8px}.history-heading h2{font-size:21px}.ledger-note{display:none}th,td{padding:12px 5px}td strong{font-size:10px}td a{font-size:9px;display:block;max-width:150px;overflow-wrap:anywhere}time{font-size:9px}.credit{font-size:16px}.signed-out{padding:30px 20px}.signed-out h2{font-size:25px}.contribution-dialog{padding:22px}.contribution-dialog h2{font-size:22px}dl>div{grid-template-columns:105px minmax(0,1fr);gap:10px}}
@media(prefers-reduced-motion:reduce){.mypage :global(*){scroll-behavior:auto;transition:none!important}}
</style>
