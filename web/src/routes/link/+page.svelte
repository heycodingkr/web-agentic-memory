<script lang="ts">
	import { onMount } from 'svelte';
	import { api, setCsrfToken, type AuthSession, type DeviceApprovalPreview } from '#lib/api';

	let session = $state<AuthSession | null>(null);
	let userCode = $state('');
	let preview = $state<DeviceApprovalPreview | null>(null);
	let loading = $state(true);
	let busy = $state(false);
	let complete = $state('');
	let error = $state('');

	const scopeLabels: Record<string, string> = {
		'read:memories': 'Read public memories',
			'read:dashboard': 'Read your profile, rank, usage, and score history',
		'write:contributions': 'Submit or withdraw your contributions',
		'report:outcome': 'Report an outcome for an issued receipt'
	};

	onMount(async () => {
		const initialCode = new URLSearchParams(window.location.search).get('code');
		if (initialCode) userCode = initialCode.toUpperCase();
		try {
			session = await api<AuthSession>('/v1/auth/session');
			setCsrfToken(session.csrf_token);
		} catch {
			session = null;
			setCsrfToken();
		} finally {
			loading = false;
		}
	});

	async function loadPreview() {
		busy = true;
		error = '';
		complete = '';
		preview = null;
		try {
			preview = await api<DeviceApprovalPreview>('/v1/link/preview', {
				method: 'POST',
				body: JSON.stringify({ user_code: userCode.trim() })
			});
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not find the device-link request.';
		} finally {
			busy = false;
		}
	}

	async function decide(approved: boolean) {
		if (!preview) return;
		busy = true;
		error = '';
		try {
			await api<{ approved?: boolean; denied?: boolean }>(
				approved ? '/v1/link/approve' : '/v1/link/deny',
				{ method: 'POST', body: JSON.stringify({ user_code: userCode.trim() }) }
			);
			complete = approved
				? 'Device approved. The CLI will receive its device token once during the next poll.'
				: 'Device request denied. No token will be issued for this request.';
			preview = null;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not process the request.';
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head>
	<title>Link a device — Web Agentic Memory</title>
	<meta name="description" content="Sign in and approve a local WAM device with a one-time code." />
</svelte:head>

<section class="page-shell">
	<div class="narrow">
		<div class="page-heading">
			<span class="eyebrow">DEVICE LINKING</span>
			<h1>Connect your device<br />securely.</h1>
			<p>Enter the one-time code shown by the CLI to review the device name and requested permissions. No account token is issued before approval.</p>
		</div>

		{#if loading}
			<div class="soft-card link-card"><p>Checking your sign-in…</p></div>
		{:else if !session}
			<div class="soft-card link-card">
				<h2>Sign in before approving.</h2>
				<p>A GitHub web session is required to link a device to this account.</p>
				<a class="button button-dark" href="/login">Go to sign in <span aria-hidden="true">↗</span></a>
			</div>
		{:else}
			<div class="soft-card link-card">
				<form onsubmit={(event) => { event.preventDefault(); loadPreview(); }}>
					<label for="user-code">Device code shown in the CLI</label>
					<div class="code-entry">
						<input id="user-code" bind:value={userCode} maxlength="12" autocomplete="one-time-code" autocapitalize="characters" placeholder="ABCD-EFGH" required />
						<button class="button button-dark" type="submit" disabled={busy || userCode.trim().length < 8}>{busy ? 'Checking…' : 'Review request'}</button>
					</div>
					<p class="field-help">Codes expire quickly, and failed attempts are limited per account.</p>
				</form>
			</div>

			{#if preview}
				<section class="soft-card request-preview" aria-labelledby="request-title">
					<span class="section-label">PERMISSION REVIEW</span>
					<h2 id="request-title">{preview.name}</h2>
					<p>This device requested the following permissions:</p>
					<ul>{#each preview.scopes as scope}<li>{scopeLabels[scope] ?? scope}</li>{/each}</ul>
					<p class="expires">Request expires: {new Date(preview.expires_at * 1000).toLocaleString('en-US')}</p>
					<div class="decision-actions">
						<button class="button button-dark" type="button" onclick={() => decide(true)} disabled={busy}>Approve device</button>
						<button class="button button-light" type="button" onclick={() => decide(false)} disabled={busy}>Deny</button>
					</div>
				</section>
			{/if}
			{#if complete}<div class="notice-box success" role="status">{complete}</div>{/if}
		{/if}
		{#if error}<div class="notice-box link-error" role="alert">{error}</div>{/if}

		<div class="link-footnote">The device token is returned once after approval. The server never stores the token in plaintext.</div>
	</div>
</section>

<style>
	.link-card { display: grid; gap: 14px; padding: 24px; }
	.link-card h2, .request-preview h2 { margin: 8px 0; color: #203b2d; font-size: 19px; }
	.link-card p, .request-preview p { color: #77847b; font-size: 12px; line-height: 1.7; }
	.link-card label { display: block; color: #53695a; font-size: 11px; font-weight: 700; }
	.code-entry { display: flex; gap: 10px; margin-top: 8px; }
	.code-entry input { min-width: 0; flex: 1; min-height: 46px; padding: 10px 13px; border: 1px solid #dce2d9; border-radius: 6px; background: #fffef9; color: #18312a; font: 700 18px Manrope, sans-serif; letter-spacing: .12em; text-transform: uppercase; }
	.code-entry .button { white-space: nowrap; }
	.field-help { margin: 8px 0 0; font-size: 10px !important; }
	.request-preview { margin-top: 14px; padding: 24px; }
	.request-preview ul { display: grid; gap: 8px; margin: 16px 0; padding-left: 20px; color: #315847; font-size: 12px; }
	.request-preview .expires { padding-top: 12px; border-top: 1px solid #e8ece6; font-size: 10px; }
	.decision-actions { display: flex; flex-wrap: wrap; gap: 9px; margin-top: 15px; }
	.success, .link-error { margin-top: 14px; }
	.link-footnote { margin-top: 15px; color: #8a968d; font-size: 10px; line-height: 1.7; }
	@media (max-width: 540px) { .code-entry { flex-direction: column; } }
</style>
