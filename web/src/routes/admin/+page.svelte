<script lang="ts">
    import { onMount } from 'svelte';
    import { adminApi, AdminError, type AdminSession } from '#lib/admin-api';
    import type { ReviewQueue, ReviewQueueItem, Contribution } from '#lib/api';
    type Detail = { record: Contribution; review_generation: number; submitter_account_id: string; proposed_memory: unknown | null; replay: { samples: number; baseline_successes: number; candidate_successes: number; baseline_tool_calls: number; candidate_tool_calls: number } | null };
    let session = $state<AdminSession | null>(null);
    let loading = $state(true);
    let busy = $state(false);
    let email = $state('');
    let sent = $state(false);
    let resendAt = $state(0);
    let cooldown = $state(0);
    let queue = $state<ReviewQueue | null>(null);
    let selected = $state<Detail | null>(null);
    let detailBusy = $state(false);
    let search = $state('');
    let filter = $state('all');
    let error = $state('');
    let notice = $state('');
    let confirm = $state<'approve' | 'reject' | null>(null);
    let revision = 0;
    let selectedRequest = 0;
    let expiration: ReturnType<typeof setTimeout>;
    const labels: Record<string, string> = { queued:'Queued', in_review:'In review', approved:'Approved', rejected:'Rejected', withdrawn:'Withdrawn', expired:'Expired', consent_required:'Consent required' };
    const stages: Record<string, string> = { queued:'Awaiting validation', processing:'Validation in progress', retrying:'Waiting to retry', waiting_budget:'Waiting for validation budget', manual_review:'Manual review required', published:'Memory published', rejected:'Validation rejected' };
    const actions: Record<string,string> = {navigate:'Navigate',edit:'Edit',save:'Save'};
    const filtered = $derived((queue?.contributions || []).filter(item => (filter === 'all' || item.record.status === filter)
        && [item.record.id, item.record.url_pattern, item.record.task_category].join(' ').toLowerCase().includes(search.toLowerCase()))
        .toSorted((a,b) => b.record.submitted_at - a.record.submitted_at));
    function date(value: number) { return new Date(value * 1000).toLocaleString('en-US'); }
    function clearSession() { revision++; selectedRequest++; session=null; queue=null; selected=null; confirm=null; clearTimeout(expiration); }
    function failed(cause: unknown) {
        if (cause instanceof AdminError && (cause.status === 401 || cause.status === 403)) clearSession();
        error = cause instanceof Error ? cause.message : 'Connection failed. Please try again.';
    }
    async function restore() {
        try {
            session = await adminApi<AdminSession>('/v1/admin/auth/session');
            expiration = setTimeout(() => { clearSession(); error='Your sign-in session has expired. Please sign in again.'; }, Math.max(0, session.expires_at * 1000-Date.now()));
            await refresh();
        } catch (cause) { if (cause instanceof AdminError && cause.status === 401) clearSession(); else failed(cause); }
        finally { loading=false; }
    }
    onMount(() => {
        void restore();
        const timer = setInterval(() => cooldown=Math.max(0,Math.ceil((resendAt-Date.now())/1000)),1000);
        return () => { clearTimeout(expiration); clearInterval(timer); revision++; selectedRequest++; };
    });
    async function sendLink(event: SubmitEvent) {
        event.preventDefault(); busy=true; error=''; notice='';
        try { await adminApi('/v1/admin/auth/start',{method:'POST',body:JSON.stringify({email})}); sent=true; resendAt=Date.now()+60000; cooldown=60; }
        catch(cause) { failed(cause); } finally { busy=false; }
    }
    async function refresh() {
        const current=revision;
        const result=await adminApi<ReviewQueue>('/v1/admin/contributions');
        if(current===revision) queue=result;
    }
    async function reload() {
        busy=true; error=''; notice='';
        try { await refresh(); if(selected) await openDetail(selected.record.id); }
        catch(cause) { queue=null; selected=null; failed(cause); } finally { busy=false; }
    }
    async function openDetail(id: string) {
        const current=++selectedRequest; const authRevision=revision;
        detailBusy=true; selected=null; confirm=null; error='';
        try { const result=await adminApi<Detail>('/v1/admin/contributions/'+encodeURIComponent(id)); if(current===selectedRequest && authRevision===revision) selected=result; }
        catch(cause) { if(current===selectedRequest) failed(cause); }
        finally { if(current===selectedRequest) detailBusy=false; }
    }
    async function claim() {
        if(!selected || !session) return;
        const id=selected.record.id; busy=true; error=''; notice='';
        try { await adminApi('/v1/admin/contributions/'+encodeURIComponent(id)+'/claim',{method:'POST'},session.csrf_token); await refresh(); await openDetail(id); notice='Review started. Check the details before making a decision.'; }
        catch(cause) { failed(cause); } finally { busy=false; }
    }
    async function decide() {
        if(!selected || !session || !confirm) return;
        const id=selected.record.id; busy=true; error=''; notice='';
        try {
            const saved=await adminApi<Contribution>('/v1/admin/contributions/'+encodeURIComponent(id)+'/decision',{method:'POST',body:JSON.stringify({review_generation:selected.review_generation,decision:confirm})},session.csrf_token);
            confirm=null; await refresh(); await openDetail(id);
            notice=saved.status==='queued'?'Validation requested. Publication will be decided when validation completes.':saved.status==='rejected'?'Rejection recorded.':'Review decision recorded.';
        } catch(cause) { confirm=null; failed(cause); } finally { busy=false; }
    }
    async function logout() {
        if(!session) return; busy=true; error='';
        try { await adminApi('/v1/admin/auth/logout',{method:'POST'},session.csrf_token); clearSession(); sent=false; email=''; notice='Signed out.'; }
        catch(cause) { failed(cause); } finally { busy=false; }
    }
</script>
<svelte:head><title>Contribution Review Admin · WAM</title><meta name="robots" content="noindex, nofollow" /><meta name="referrer" content="no-referrer" /></svelte:head>
<div class="admin-shell">
<header class="admin-header"><a href="/admin" class="wordmark"><img src="/brand/wam-mark.svg" alt="" width="30" height="30" /><strong>web agentic memory</strong><span>ADMIN</span></a>{#if session}<button type="button" class="text-button" onclick={logout} disabled={busy}>Sign out ↗</button>{/if}</header>
<main class="admin-content">
{#if loading}<p class="loading" role="status">Checking administrator sign-in…</p>
{:else if !session}
    <section class="login-card">
        <span class="eyebrow">ADMINISTRATOR ACCESS</span><h1>Review contributed<br />memories.</h1><p class="intro">Sign-in links are sent to approved administrator email addresses.</p>
        {#if error}<p class="feedback error" role="alert">{error}</p>{/if}
        {#if notice}<p class="feedback" role="status">{notice}</p>{/if}
        {#if sent}<div class="sent" role="status"><strong>We've received your sign-in link request.</strong><p>If this email address is approved for administrator access, we'll send a sign-in link. Open the “Sign in to the administrator console” button in that email in this browser within 10 minutes. Check your spam folder if you don't see it.</p></div>{/if}
        <form onsubmit={sendLink}><label for="admin-email">Administrator email</label><input id="admin-email" type="email" autocomplete="email" bind:value={email} required maxlength="254" placeholder="Enter administrator email" /><button type="submit" class="button button-dark" disabled={busy || cooldown>0}>{busy?'Sending email…':cooldown>0?'Send again in '+cooldown+'s':sent?'Resend sign-in email':'Email me a sign-in link'} <span aria-hidden="true">↗</span></button></form>
        <p class="login-note">Only approved administrators can sign in.</p>
    </section>
{:else}
    <div class="page-title"><div><span class="eyebrow">CONTRIBUTION REVIEW</span><h1>Contribution review</h1><p>Review submitted workflows and manage their review and publication status.</p></div><button class="button button-light" type="button" onclick={reload} disabled={busy}>Refresh ↻</button></div>
    {#if error}<p class="feedback error" role="alert">{error}</p>{/if}{#if notice}<p class="feedback" role="status">{notice}</p>{/if}
    {#if queue}
    <div class="stats"><div><span>Total requests</span><strong>{queue.contributions.length}</strong></div><div><span>Queued</span><strong>{queue.counts.queued}</strong></div><div><span>In review</span><strong>{queue.counts.in_review}</strong></div><div><span>Approved</span><strong>{queue.counts.approved}</strong></div><div><span>Rejected</span><strong>{queue.counts.rejected}</strong></div></div>
    <div class="workspace">
    <section class="list-panel" aria-label="Contribution requests">
        <div class="filters"><label class="search-label"><span class="sr-only">Search contributions</span><input type="search" bind:value={search} placeholder="Search URL patterns, tasks, or request IDs" /></label><label><span class="sr-only">Status filter</span><select bind:value={filter}><option value="all">All statuses</option>{#each Object.entries(labels) as [value,label]}<option {value}>{label}</option>{/each}</select></label></div>
        <div class="list-meta"><span>{filtered.length} request{filtered.length === 1 ? '' : 's'}</span><span>Newest first</span></div>
        <div class="request-list">
        {#each filtered as item (item.record.id)}<button type="button" class="request" class:active={selected?.record.id===item.record.id} onclick={()=>openDetail(item.record.id)} disabled={busy}><div class="request-top"><span class="status" data-status={item.record.status}>{labels[item.record.status] || item.record.status}</span><time>{date(item.record.submitted_at)}</time></div><strong>{item.record.url_pattern}</strong><p>{item.record.task_category} · {item.record.actions.map(action=>actions[action] || action).join(' → ')}</p><small>{item.record.id} <span aria-hidden="true">↗</span></small></button>
        {:else}<div class="empty"><strong>{search || filter!=='all'?'No requests match these filters.':'No contribution requests yet.'}</strong><p>{search || filter!=='all'?'Try changing your search or status filter.':'New contribution requests will appear here.'}</p></div>{/each}
        </div>
    </section>
    <section class="detail-panel" aria-label="Contribution details" aria-busy={detailBusy}>
    {#if detailBusy}<p class="empty" role="status">Loading details…</p>
    {:else if selected}<div class="detail-heading"><span class="eyebrow">REQUEST DETAIL</span><span class="status" data-status={selected.record.status}>{labels[selected.record.status]}</span></div><h2>{selected.record.task_category}</h2><p class="detail-url">{selected.record.url_pattern}</p>
        <dl><div><dt>Request ID</dt><dd>{selected.record.id}</dd></div><div><dt>Submitting account</dt><dd>{selected.submitter_account_id}</dd></div><div><dt>Submitted</dt><dd>{date(selected.record.submitted_at)}</dd></div><div><dt>Workflow steps</dt><dd>{selected.record.actions.map(action=>actions[action] || action).join(' → ')}</dd></div><div><dt>Progress</dt><dd>{stages[selected.record.progress?.stage || ''] || selected.record.progress?.stage || labels[selected.record.status]}</dd></div>{#if selected.record.progress?.reason}<div><dt>Waiting reason</dt><dd>{selected.record.progress.reason}</dd></div>{/if}</dl>
        {#if selected.replay}<div class="verification"><h3>Replay verification</h3><p>Candidate succeeded {selected.replay.candidate_successes} of {selected.replay.samples} runs · baseline {selected.replay.baseline_successes}</p><p>Tool calls {selected.replay.baseline_tool_calls} → {selected.replay.candidate_tool_calls}</p></div>{/if}
        {#if selected.proposed_memory}<h3>Proposed memory</h3><pre>{JSON.stringify(selected.proposed_memory,null,2)}</pre>{:else}<p class="detail-note">No validated memory proposal yet. Approval will trigger automated validation before publication.</p>{/if}
        {#if selected.record.reflection}<div class="verification"><h3>Publication result</h3><p>{selected.record.reflection.kind} · {selected.record.reflection.memory_id}</p><p>{selected.record.reflection.version_id}</p></div>{/if}
        {#if selected.record.status==='queued'}<button type="button" class="button button-dark wide" onclick={claim} disabled={busy}>Start review</button>
        {:else if selected.record.status==='in_review'}
            {#if confirm}<div class="confirmation" role="group" aria-label="Confirm review decision"><strong>{confirm==='reject'?'Reject this request?':'Approve this request?'}</strong><p>{confirm==='reject'?'The rejection will be recorded in the review history.':'A validated proposal will be published; otherwise, validation will be requested.'}</p><div class="button-row"><button type="button" class="button button-light" onclick={()=>confirm=null} disabled={busy}>Cancel</button><button type="button" class="button button-dark" onclick={decide} disabled={busy}>{busy?'Processing…':'Confirm'}</button></div></div>
            {:else}<div class="button-row"><button class="button button-light" type="button" onclick={()=>confirm='reject'} disabled={busy}>Reject</button><button class="button button-dark" type="button" onclick={()=>confirm='approve'} disabled={busy}>Approve / request validation</button></div>{/if}
        {/if}
    {:else}<div class="empty detail-empty"><span aria-hidden="true">↗</span><strong>Select a request.</strong><p>Choose a contribution from the list to view<br />its details and review actions.</p></div>{/if}
    </section></div>
    <section class="history"><div class="history-title"><h2>Recent decision history</h2><span>{queue.decision_history.length} decisions</span></div>{#each queue.decision_history.toReversed().slice(0,30) as event,index (index)}<div class="history-row"><strong>{event.contribution_id}</strong><span>{event.decision==='verification_requested'?'Validation requested':labels[event.decision]}</span><time>{date(event.decided_at)}</time></div>{:else}<p>No decisions recorded yet.</p>{/each}</section>
    {/if}
{/if}
</main><footer class="admin-footer"><span>Web Agentic Memory · Administrator console</span>{#if session}<span>Session expires {date(session.expires_at)}</span>{/if}</footer>
</div>
<style>
.admin-shell{min-height:100vh;background:#f4f5ee;color:#243e30;font-size:14px}.admin-header{height:80px;border-bottom:1px solid #dce3d4;display:flex;align-items:center;justify-content:space-between;padding:0 max(24px,calc((100vw - 1360px)/2));background:#fffef9}.wordmark{display:flex;align-items:center;gap:10px;letter-spacing:-.03em;font-size:17px}.wordmark>span{font-size:10px;letter-spacing:.12em;color:#4d6749;border:1px solid #d9e2cf;padding:4px 7px;border-radius:5px;margin-left:8px}.admin-content{max-width:1360px;margin:auto;padding:42px 24px 64px;min-height:calc(100vh - 146px)}.text-button{border:0;background:none;color:#3b5944;padding:10px;cursor:pointer;font:inherit;font-size:13px}.login-card{max-width:490px;margin:7vh auto 4vh;background:#fffef9;padding:40px;border:1px solid #dce3d4;border-radius:18px;box-shadow:0 15px 45px #243e3007}.login-card h1{font-size:34px;letter-spacing:-.05em;line-height:1.35;margin:16px 0}.intro,.login-note{color:#536750;line-height:1.7}.login-card form{display:grid;gap:12px;margin-top:26px}.login-card label{font-weight:600;font-size:13px}.login-card input,.filters input,.filters select{min-height:46px;border:1px solid #ccd7c5;border-radius:8px;background:#fffef9;color:#243e30;padding:11px 13px;font:inherit}.login-card form button{justify-content:space-between;margin-top:5px}.login-note{font-size:12px;text-align:center;margin:22px 0 0}.sent{margin-top:24px;padding:17px;border-radius:9px;background:#edf3e5;font-size:13px;line-height:1.7}.sent p{margin:7px 0 0}.feedback{background:#e8f1df;border:1px solid #ccdcba;border-radius:8px;padding:14px 18px;font-size:13px;line-height:1.7}.error{background:#fff0ea;border-color:#e9c9bc;color:#903e2f}.page-title{display:flex;justify-content:space-between;align-items:center;gap:20px;margin-bottom:30px}.page-title h1{font-size:32px;letter-spacing:-.05em;margin:10px 0}.page-title p{margin:0;color:#52654e;line-height:1.7}.stats{display:grid;grid-template-columns:repeat(5,1fr);gap:12px;margin-bottom:24px}.stats>div{display:grid;gap:12px;background:#fffef9;border:1px solid #dce3d4;border-radius:12px;padding:20px 24px}.stats span{font-size:12px;color:#4e634d}.stats strong{font-size:29px;font-weight:600}.workspace{display:grid;grid-template-columns:1.07fr 1fr;gap:22px;align-items:start}.list-panel,.detail-panel,.history{border:1px solid #dce3d4;border-radius:13px;background:#fffef9;overflow:hidden}.filters{display:flex;gap:10px;padding:20px 20px 12px}.search-label{flex:1;min-width:0}.filters input{width:100%;font-size:12px}.filters select{font-size:12px;max-width:135px}.list-meta{display:flex;justify-content:space-between;padding:0 22px 15px;color:#52664e;font-size:11px}.request-list{max-height:760px;overflow-y:auto}.request{display:block;width:100%;text-align:left;border:0;border-top:1px solid #e4eadf;background:none;padding:20px 22px;cursor:pointer;color:inherit;font:inherit;overflow-wrap:anywhere}.request:hover,.request.active{background:#f0f4e8}.request.active{box-shadow:inset 3px 0 #456b36}.request-top{display:flex;align-items:center;justify-content:space-between;gap:12px;margin-bottom:13px}.request time{font-size:10px;color:#576b53}.request>strong{font-size:13px;font-weight:600;line-height:1.6}.request p{font-size:12px;color:#52664e;margin:8px 0 12px}.request small{display:flex;justify-content:space-between;font-size:10px;color:#536c4c}.status{display:inline-block;font-size:10px;padding:5px 9px;border-radius:30px;background:#edf0e7;color:#4b6045;white-space:nowrap}.status[data-status='in_review']{background:#f7edcc;color:#705615}.status[data-status='approved']{background:#e2f0d6;color:#315421}.status[data-status='rejected']{background:#f7e6e0;color:#8f4034}.detail-panel{padding:26px;min-height:460px}.detail-heading{display:flex;justify-content:space-between;align-items:center;gap:12px}.detail-panel h2{font-size:23px;margin:24px 0 12px;letter-spacing:-.03em}.detail-url{font-size:13px;overflow-wrap:anywhere;line-height:1.7;color:#4a6446}.detail-panel dl{display:grid;gap:16px;margin:26px 0;border-top:1px solid #e1e7d9;border-bottom:1px solid #e1e7d9;padding:22px 0;font-size:12px}.detail-panel dl>div{display:grid;grid-template-columns:80px 1fr;gap:12px}.detail-panel dt{color:#576c51}.detail-panel dd{margin:0;overflow-wrap:anywhere;line-height:1.6}.detail-panel h3{font-size:13px;margin:0 0 12px}.detail-panel pre{max-height:360px;overflow:auto;background:#f2f4eb;border:1px solid #dfe5d5;border-radius:8px;padding:16px;font-size:11px;line-height:1.8;white-space:pre-wrap;overflow-wrap:anywhere}.verification{background:#f0f4e7;padding:16px;border-radius:8px;margin-bottom:20px}.verification p{font-size:12px;margin:7px 0;overflow-wrap:anywhere}.detail-note{font-size:12px;line-height:1.8;color:#55684e;padding:16px;background:#f4f5ed;border-radius:8px;margin:0 0 22px}.button-row{display:flex;gap:10px;justify-content:flex-end;flex-wrap:wrap}.button-row .button{font-size:12px;min-height:42px}.wide{width:100%;font-size:13px}.confirmation{border:1px solid #d5dfc6;background:#f1f5e9;padding:18px;border-radius:9px}.confirmation>strong{font-size:14px}.confirmation p{font-size:12px;line-height:1.7}.empty{padding:50px 20px;text-align:center;line-height:1.8;color:#51664c;font-size:13px}.empty strong{display:block;color:#304d30}.empty p{font-size:12px;margin-bottom:0}.detail-empty{padding:80px 0}.detail-empty>span{display:block;font-size:36px;margin-bottom:16px;color:#6f875a}.history{padding:24px;margin-top:24px}.history-title{display:flex;align-items:center;justify-content:space-between;margin-bottom:16px}.history h2{font-size:16px;margin:0}.history-title>span,.history>p{font-size:12px;color:#53664f}.history-row{display:grid;grid-template-columns:1fr 1fr 1fr;gap:15px;border-top:1px solid #e4e9dc;padding:13px 0;font-size:11px}.history-row time{text-align:right;color:#52664c}.admin-footer{display:flex;justify-content:space-between;gap:18px;max-width:1360px;padding:0 24px 24px;margin:auto;font-size:11px;color:#52664c}.loading{padding:90px 0;text-align:center}.sr-only{position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;clip:rect(0,0,0,0);white-space:nowrap;border:0}button:focus-visible,input:focus-visible,select:focus-visible,a:focus-visible{outline:3px solid #8bad67;outline-offset:3px}button:disabled{cursor:wait}
@media(max-width:850px){.workspace{grid-template-columns:1fr}.request-list{max-height:420px}.stats>div{padding:16px}.admin-header{height:68px}.wordmark{font-size:14px}.wordmark>span{margin-left:0}.admin-content{padding-top:30px}.detail-panel{min-height:300px}.detail-empty{padding:45px 0}}
@media(max-width:540px){.admin-header{padding:0 18px}.wordmark img{width:25px;height:25px}.wordmark>strong{font-size:12px}.wordmark>span{font-size:9px}.text-button{font-size:11px}.admin-content{padding:26px 16px 40px}.page-title{align-items:flex-start}.page-title h1{font-size:25px}.page-title p{font-size:12px}.page-title>.button{font-size:11px;padding:10px;white-space:nowrap}.stats{grid-template-columns:repeat(3,1fr);gap:8px}.stats>div{padding:14px;gap:9px}.stats span{font-size:11px}.stats strong{font-size:24px}.filters{padding:15px 12px 12px;gap:8px}.filters input{font-size:11px;padding:10px}.filters select{max-width:105px;padding:10px 5px;font-size:11px}.request{padding:18px 16px}.detail-panel{padding:21px}.login-card{padding:28px 23px;margin:5vh auto}.login-card h1{font-size:30px}.intro{font-size:13px}.admin-footer{flex-direction:column;padding:0 20px 20px;gap:8px}.history{padding:20px}.history-row{grid-template-columns:1fr 1fr}.history-row time{text-align:left;grid-column:1 / -1}.detail-panel dl>div{grid-template-columns:68px 1fr}}
</style>
