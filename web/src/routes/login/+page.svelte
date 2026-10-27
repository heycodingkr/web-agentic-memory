<script lang="ts">
	import { onMount } from 'svelte';
	import { api, apiUrl, setCsrfToken, type AuthConfig, type AuthSession } from '#lib/api';

	let config = $state<AuthConfig | null>(null);
	let session = $state<AuthSession | null>(null);
	let loading = $state(true);
	let busy = $state(false);
	let error = $state('');

	onMount(async () => {
		const params = new URLSearchParams(window.location.search);
		if (params.get('auth_error')) error = 'GitHub sign-in failed. Please try again.';
		try {
			config = await api<AuthConfig>('/v1/auth/config');
		} catch {
			error = 'Could not connect to the local WAM API.';
		}
		try {
			session = await api<AuthSession>('/v1/auth/session');
			setCsrfToken(session.csrf_token);
		} catch (cause) {
			if (cause instanceof Error && 'status' in cause && cause.status !== 401) {
				error = cause.message;
			}
			session = null;
			setCsrfToken();
		} finally {
			loading = false;
		}
	});

	async function logout() {
		busy = true;
		error = '';
		try {
			await api<void>('/v1/auth/logout', { method: 'POST' });
			session = null;
			setCsrfToken();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : 'Could not log out.';
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head>
	<title>Log in — Web Agentic Memory</title>
	<meta name="description" content="Connect a GitHub account and manage WAM device access." />
</svelte:head>

<section class="page-shell">
	<div class="narrow login-wrap">
		<div class="page-heading">
			<span class="eyebrow">ACCOUNT ACCESS</span>
			<h1>Keep useful paths<br />moving forward.</h1>
			<p>Your GitHub account is linked to an internal WAM account. WAM does not use your email as an account ID or public profile detail.</p>
		</div>

		{#if error}
			<div class="notice-box login-error" role="alert">{error}</div>
		{/if}

		<div class="login-card soft-card">
			<div class="login-mark" aria-hidden="true">
				<svg viewBox="0 0 40 40" fill="none"><path d="M7 29.5 17.3 19l7.1 7.2L33 15.5" stroke="currentColor" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round" /><circle cx="7" cy="29.5" r="3" fill="#D7F06A" /><circle cx="17.3" cy="19" r="3" fill="#D7F06A" /><circle cx="24.4" cy="26.2" r="3" fill="#D7F06A" /><circle cx="33" cy="15.5" r="3" fill="#D7F06A" /></svg>
			</div>
			<span class="section-label">ACCOUNT SESSION</span>
			{#if loading}
				<h2>Checking your sign-in…</h2>
			{:else if session}
				<h2>Account connected.</h2>
				<p>Your session expires {new Date(session.expires_at * 1000).toLocaleString('en-US')}. Connected devices remain active until you revoke them in settings.</p>
				<div class="login-actions">
					<a class="button button-dark full-width" href="/dashboard">View my rank <span aria-hidden="true">↗</span></a>
					<a class="button button-light full-width" href="/settings/devices">Manage devices <span aria-hidden="true">↗</span></a>
					<button class="button button-light full-width" type="button" onclick={logout} disabled={busy}>{busy ? 'Signing out…' : 'Sign out'}</button>
				</div>
			{:else if config?.github_oauth_enabled}
				<h2>Sign in with GitHub</h2>
				<p>Sign-in requests use state and PKCE checks. WAM does not store your GitHub email.</p>
				<a class="button button-dark full-width" href={apiUrl('/v1/auth/github/start')}>Connect GitHub account <span aria-hidden="true">↗</span></a>
			{:else}
				<h2>GitHub OAuth setup required.</h2>
				<p>Set WAM_GITHUB_CLIENT_ID, WAM_GITHUB_CLIENT_SECRET, and WAM_GITHUB_REDIRECT_URI for the local API to enable sign-in.</p>
				<div class="notice-box oauth-state">The local API has no OAuth provider configured.</div>
			{/if}
			<div class="login-footnote">Signing out does not revoke device tokens.</div>
		</div>

		<div class="soft-card demo-access">
			<span class="section-label">LOCAL DEMO</span>
			<h2>Preview with a demo account</h2>
			<p>For local development, enter the local server's WAM_DEMO_TOKEN in the dashboard. It is not a production credential.</p>
			<a class="button button-light full-width" href="/dashboard">Open demo dashboard <span aria-hidden="true">↗</span></a>
			<a class="text-link" href="/memories">Browse memories without signing in →</a>
		</div>
	</div>
</section>

<style>
	.login-actions { display: grid; gap: 9px; margin-top: 18px; }
	.login-error { margin-bottom: 14px; }
	.oauth-state { margin-top: 16px; }
	.demo-access { display: grid; gap: 12px; margin-top: 14px; padding: 22px; }
	.demo-access h2 { margin: 0; color: #203b2d; font-size: 18px; }
	.demo-access p { margin: 0; color: #748279; font-size: 12px; line-height: 1.7; }
	.demo-access .text-link { justify-self: center; }
</style>
