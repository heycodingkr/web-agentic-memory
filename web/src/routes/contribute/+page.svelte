<script lang="ts">
	import { onMount } from 'svelte';
	import { api, setCsrfToken, type AuthSession, type Contribution, type ContributionAction, type ContributionList } from '#lib/api';

	const consentVersion = 'local-development-v1';
	const taskOptions = [
		{ value: 'compose-draft', label: 'Draft writing' },
		{ value: 'navigate', label: 'Site navigation' },
		{ value: 'research', label: 'Research' }
	];
	const actionOptions: Array<{ value: ContributionAction; label: string }> = [
		{ value: 'navigate', label: 'Open a site' },
		{ value: 'edit', label: 'Edit content' },
		{ value: 'save', label: 'Check saved state' }
	];

	let url = $state('https://example.com/guide?campaign=local#top');
	let task = $state('compose-draft');
	let actions = $state<ContributionAction[]>(['navigate', 'edit', 'save']);
	let token = $state('');
	let consent = $state(false);
	let busy = $state(false);
	let error = $state('');
	let notice = $state('');
	let contributions = $state<Contribution[]>([]);
	let signedIn = $state(false);
	let checkingSession = $state(true);

	const canonicalPreview = $derived.by(() => {
		try {
			const parsed = new URL(url);
			if (parsed.protocol !== 'https:' || parsed.username || parsed.password) {
				return 'Enter a public HTTPS URL.';
			}
			return parsed.origin + parsed.pathname;
		} catch {
			return 'Check the URL format.';
		}
	});
	const urlIsValid = $derived(!canonicalPreview.startsWith('Enter a public') && !canonicalPreview.startsWith('Check the URL'));
	const payloadPreview = $derived(
		JSON.stringify(
			{
				url: canonicalPreview,
				task_category: task,
				actions,
				consent,
				consent_version: consentVersion
			},
			null,
			2
		)
	);

	async function loadMine() {
		error = '';
		notice = '';
		busy = true;
		try {
			const result = await api<ContributionList>('/v1/me/contributions', {}, token || undefined);
			contributions = result.contributions;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not load your contributions.';
		} finally {
			busy = false;
		}
	}

	async function submitContribution(event: SubmitEvent) {
		event.preventDefault();
		error = '';
		notice = '';
		if ((!token && !signedIn) || !consent || !urlIsValid || actions.length === 0) return;
		busy = true;
		try {
			const receipt = await api<Contribution>(
				'/v1/me/contributions',
				{
					method: 'POST',
					body: JSON.stringify({
						url: canonicalPreview,
						task_category: task,
						actions,
						consent,
						consent_version: consentVersion
					})
				},
				token || undefined
			);
			notice = `The local server received contribution ${receipt.id}. Status: ${receipt.status}.`;
			const result = await api<ContributionList>('/v1/me/contributions', {}, token || undefined);
			contributions = result.contributions;
			consent = false;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not submit the contribution.';
		} finally {
			busy = false;
		}
	}

	async function withdrawContribution(id: string) {
		error = '';
		notice = '';
		busy = true;
		try {
			await api<void>(`/v1/me/contributions/${encodeURIComponent(id)}`, { method: 'DELETE' }, token || undefined);
			contributions = contributions.filter((item) => item.id !== id);
			notice = 'The unpublished local submission was withdrawn.';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not withdraw the submission.';
		} finally {
			busy = false;
		}
	}

	function actionLabel(action: ContributionAction) {
		return actionOptions.find((item) => item.value === action)?.label ?? action;
	}

	function submittedAt(seconds: number) {
		return new Date(seconds * 1000).toLocaleString('en-US');
	}

	onMount(async () => {
		try {
			const session = await api<AuthSession>('/v1/auth/session');
			setCsrfToken(session.csrf_token);
			signedIn = true;
			await loadMine();
		} catch (cause) {
			setCsrfToken();
			if (cause instanceof Error && 'status' in cause && cause.status !== 401) error = cause.message;
		} finally {
			checkingSession = false;
		}
	});
</script>

<svelte:head>
	<title>Contribute — Web Agentic Memory</title>
	<meta name="description" content="Submit an approved, normalized workflow summary to the local development API and follow its status." />
</svelte:head>

<section class="page-shell">
	<div class="container">
		<div class="page-heading">
			<span class="eyebrow">CONTRIBUTE SAFELY</span>
			<h1>Review what you share<br />before submitting.</h1>
			<p>Only the workflow summary you select is sent to the local API. Page contents, prompts, browser tool inputs, and results are not sent.</p>
				<div class="demo-note">Submit with a GitHub session or local demo token. Submissions and review states stay in a local file; they are not published and earn no points.</div>
		</div>

		{#if checkingSession}
			<section class="soft-card auth-panel contribution-auth"><p>Checking your sign-in…</p></section>
		{:else if signedIn}
			<section class="soft-card auth-panel contribution-auth"><p>Signed in with GitHub. You can submit after reviewing and giving explicit consent.</p><button class="button button-light button-small" type="button" onclick={loadMine} disabled={busy}>{busy ? 'Loading…' : 'Refresh my submissions'}</button><a class="text-link" href="/login">Account settings →</a></section>
		{:else}
		<section class="soft-card auth-panel contribution-auth">
			<label for="contribution-token">Local demo token</label>
			<div><input id="contribution-token" type="password" bind:value={token} autocomplete="off" placeholder="Value of WAM_DEMO_TOKEN" /><button class="button button-light button-small" type="button" onclick={loadMine} disabled={(!token && !signedIn) || busy}>{busy ? 'Loading…' : 'Load my submissions'}</button></div>
			<p>The token is sent in the authorization header and is not stored in the browser.</p>
				<a class="text-link" href="/login">Sign in with GitHub →</a>
		</section>
			{/if}

		{#if error}<div class="notice-box contribution-message" role="alert">{error}</div>{/if}
		{#if notice}<div class="success-box contribution-message" role="status">{notice}</div>{/if}

		<form class="form-layout" onsubmit={submitContribution}>
			<div class="soft-card form-panel">
				<div class="section-label">01 / OBSERVED WORKFLOW</div>
				<div class="field">
					<label for="contribution-url">Site where you did the work</label>
					<input id="contribution-url" type="url" bind:value={url} autocomplete="url" placeholder="https://example.com" />
					<span class="field-help">Query parameters and fragments are removed before sending. Preview: <code>{canonicalPreview}</code></span>
				</div>
				<div class="field">
					<label for="task-category">Task type</label>
					<select id="task-category" bind:value={task}>
						{#each taskOptions as item}<option value={item.value}>{item.label}</option>{/each}
					</select>
				</div>
				<fieldset class="action-field">
					<legend>Select the actions you observed</legend>
					<div class="action-options">
						{#each actionOptions as item}
							<label><input type="checkbox" bind:group={actions} value={item.value} />{item.label}</label>
						{/each}
					</div>
					<span class="field-help">Page contents are not read or collected. Only the selected fixed action codes are sent.</span>
				</fieldset>
					<label class="consent"><input type="checkbox" bind:checked={consent} /><span>I agree to store the normalized URL, task type, and selected action codes on the local server and show them in the review queue. Review records a status only; it does not publish the memory or award points. I can withdraw the submission at any time.</span></label>
				<div class="form-submit-row">
					<button class="button button-dark" type="submit" disabled={(!token && !signedIn) || !consent || !urlIsValid || actions.length === 0 || busy}>{busy ? 'Submitting…' : 'Agree and submit locally'} <span aria-hidden="true">↗</span></button>
					<span>Review the payload before submitting.</span>
				</div>
			</div>
			<aside class="contribution-aside">
				<div class="section-label">02 / EXACT PAYLOAD</div>
				<div class="code-panel">
					<div class="code-panel-head"><span>ONLY SELECTED FIELDS</span><span class="live-badge"><i></i> LOCAL</span></div>
					<pre><code>{payloadPreview}</code></pre>
				</div>
					<div class="notice-box"><strong>What review status means</strong>Submissions move from queued to in review, approved, or rejected. Approval does not publish a public memory or award points.</div>
			</aside>
		</form>

		<section class="soft-card submitted-panel" aria-labelledby="submitted-heading">
			<div class="panel-heading"><div><span class="section-label">03 / MY SUBMISSIONS</span><h2 id="submitted-heading">My local submissions</h2></div></div>
			{#if contributions.length}
				<div class="submitted-list">
					{#each contributions as item (item.id)}
						<div class="submitted-row">
							<div><strong>{item.id} · {item.status}</strong><small>{item.url_pattern} · {item.task_category}</small><small>{item.actions.map(actionLabel).join(' → ')} · {submittedAt(item.submitted_at)}</small></div>
							<button class="icon-button danger-button" type="button" onclick={() => withdrawContribution(item.id)} disabled={busy}>Withdraw</button>
						</div>
					{/each}
				</div>
			{:else}
				<p class="field-help">No submissions yet. Enter a demo token to load your submissions, or send your first contribution.</p>
			{/if}
		</section>
	</div>
</section>

<style>
	.contribution-auth { max-width: 820px; margin-bottom: 18px; padding: 18px 20px; }
	.contribution-auth label { color: #586a5e; font-size: 11px; font-weight: 700; }
	.contribution-auth > div { display: flex; gap: 10px; margin-top: 8px; }
	.contribution-auth input { min-width: 0; flex: 1; min-height: 42px; padding: 10px 12px; border: 1px solid #dce2d9; border-radius: 6px; color: #18312a; background: #fffef9; }
	.contribution-auth p { margin: 8px 0 0; color: #849087; font-size: 10px; }
	.contribution-message { margin: 12px 0; }
	.action-field { display: grid; gap: 10px; margin: 0; padding: 0; border: 0; }
	.action-field legend { margin-bottom: 8px; color: #586a5e; font-size: 11px; font-weight: 700; }
	.action-options { display: flex; flex-wrap: wrap; gap: 10px; }
	.action-options label { display: flex; align-items: center; gap: 7px; min-height: 38px; padding: 8px 11px; border: 1px solid #e2e8df; border-radius: 5px; color: #52675a; background: #fafbf7; font-size: 11px; }
	.submitted-panel { margin-top: 22px; padding: 24px; }
	.submitted-panel h2 { margin-top: 6px; color: #203b2d; font-size: 19px; }
	.submitted-list { display: grid; gap: 10px; }
	.submitted-row { display: flex; justify-content: space-between; align-items: center; gap: 14px; padding: 12px 0; border-top: 1px solid #e9eee7; }
	.submitted-row > div { display: grid; gap: 5px; min-width: 0; }
	.submitted-row strong { color: #315847; font-size: 12px; }
	.submitted-row small { color: #79877e; font-size: 10px; overflow-wrap: anywhere; }
	@media (max-width: 640px) {
		.contribution-auth > div { flex-direction: column; }
		.submitted-row { align-items: flex-start; }
	}
</style>
