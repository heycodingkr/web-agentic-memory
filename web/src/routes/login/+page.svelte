<script lang="ts">
	import { onMount } from 'svelte';
	import { setBrowserSession } from '#lib/browser-session';
	import { normalizeDeviceLinkCode } from '#lib/device-link-code';
	import { api, apiUrl, ApiError, setCsrfToken, type AuthConfig, type AuthSession } from '#lib/api';

	let config = $state<AuthConfig | null>(null);
	let session = $state<AuthSession | null>(null);
	let loading = $state(true);
	let sessionCheckFailed = $state(false);
	let busy = $state(false);
	let error = $state('');
	let email = $state('');
	let code = $state('');
	let codeSent = $state(false);
	let retryAt = $state(0);
	let cooldown = $state(0);
	let oauthRequest = $state('');
	let linkCode = $state('');
	let returnTo = $state('/dashboard');
	let sentTo = $state('');

	function returnToApproval() {
		const destination = oauthRequest
			? '/oauth/approve?request=' + encodeURIComponent(oauthRequest)
			: linkCode
				? '/link?code=' + encodeURIComponent(linkCode)
				: returnTo;
		window.location.assign(destination);
	}

	onMount(() => {
		const interval = window.setInterval(() => { cooldown = Math.max(0, Math.ceil((retryAt - Date.now()) / 1000)); }, 1000);
		void loadSession();
		return () => window.clearInterval(interval);
	});

	async function loadSession() {
		loading = true;
		sessionCheckFailed = false;
		error = '';
		const params = new URLSearchParams(window.location.search);
		const request = params.get('oauth_request') || '';
		if (/^[A-Za-z0-9_-]{43}$/.test(request)) oauthRequest = request;
		linkCode = normalizeDeviceLinkCode(params.get('link_code'));
		const requestedReturnTo = params.get('return_to');
		if (requestedReturnTo?.startsWith('/') && !requestedReturnTo.startsWith('//')) {
			try {
				const destination = new URL(requestedReturnTo, window.location.origin);
				if (destination.origin === window.location.origin) {
					returnTo = destination.pathname + destination.search + destination.hash;
				}
			} catch { /* Keep the default same-origin destination. */ }
		}
		if (params.get('auth_error')) error = 'GitHub sign-in failed. Please try again.';
		try {
			config = await api<AuthConfig>('/v1/auth/config');
		} catch {
			sessionCheckFailed = true;
			error = 'Could not connect to WAM. Please try again.';
		}
		try {
			session = await api<AuthSession>('/v1/auth/session');
			setCsrfToken(session.csrf_token);
			setBrowserSession(session);
			returnToApproval();
		} catch (cause) {
			if (!(cause instanceof ApiError && cause.status === 401)) {
				sessionCheckFailed = true;
				error = 'Could not check your session. Please try again.';
			}
			session = null;
			setCsrfToken();
			setBrowserSession(null);
		} finally { loading = false; }
	}

	async function sendCode(event?: SubmitEvent) {
		event?.preventDefault();
		if (busy || cooldown > 0) return;
		busy = true;
		sessionCheckFailed = false;
		error = '';
		try {
			const result = await api<{ retry_after: number }>('/v1/auth/email/start', { method: 'POST', body: JSON.stringify({ email }) });
			sentTo = email.trim();
			code = '';
			codeSent = true;
			retryAt = Date.now() + result.retry_after * 1000;
			cooldown = result.retry_after;
		} catch (cause) {
			if (cause instanceof ApiError && cause.status === 429) {
				cooldown = cause.retryAfterSeconds || 60;
				retryAt = Date.now() + cooldown * 1000;
				error = 'Please wait before requesting another code.';
			} else error = 'Could not send your code. Please check your email address and try again.';
		} finally { busy = false; }
	}

	async function verifyCode(event: SubmitEvent) {
		event.preventDefault();
		if (busy) return;
		busy = true;
		sessionCheckFailed = false;
		error = '';
		try {
			session = await api<AuthSession>('/v1/auth/email/verify', { method: 'POST', body: JSON.stringify({ code }) });
			setCsrfToken(session.csrf_token);
			setBrowserSession(session);
			code = '';
			codeSent = false;
			returnToApproval();
		} catch (cause) {
			if (!(cause instanceof ApiError)) {
				error = 'Could not verify your code. Check your connection and try again.';
			} else if (cause.status === 429) {
				error = 'Too many attempts. Request a new code when the timer ends.';
			} else if (cause.status === 400 || cause.status === 401) {
				error = 'That code is invalid or has expired. Check the code or request a new one.';
			} else if (cause.status >= 500) {
				error = 'Sign-in is temporarily unavailable. Please try again.';
			} else {
				error = 'Could not verify your code. Refresh this page and try again.';
			}
		} finally { busy = false; }
	}

	async function logout() {
		busy = true;
		error = '';
		try {
			await api<void>('/v1/auth/logout', { method: 'POST' });
			session = null;
			setCsrfToken();
			setBrowserSession(null);
		} catch { error = 'Could not log out. Please try again.'; }
		finally { busy = false; }
	}
</script>

<svelte:head>
	<title>Log in — Web Agentic Memory</title>
	<meta name="description" content="Sign in with email or GitHub and manage WAM device access." />
	<meta name="referrer" content="no-referrer" />
</svelte:head>

<section class="page-shell">
	<div class="narrow login-wrap">
		<div class="page-heading">
			<span class="eyebrow">ACCOUNT ACCESS</span>
			<h1>Keep useful paths<br />moving forward.</h1>
			<p>Sign in to your WAM account. Your email stays private and never appears on your public profile.</p>
		</div>

		{#if error}
			<div class="notice-box login-error" role="alert">
				{error}
				{#if sessionCheckFailed}
					<button class="button button-light button-small" type="button" onclick={loadSession} disabled={loading || busy}>Retry sign-in check</button>
				{/if}
			</div>
		{/if}

		<div class="login-card soft-card">
			<img class="login-mark" src="/brand/wam-mark.svg" width="55" height="55" alt="" />
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
			{:else}
				{#if config?.email_auth_enabled}
					{#if codeSent}
						<h2>Check your email.</h2>
						<p id="code-help">Enter the 6-digit code sent to {sentTo}. It expires in 10 minutes. Keep using this browser. If it does not arrive, check spam and confirm the address. If you no longer have access to this mailbox, contact your email provider; WAM cannot recover your mailbox or change the account email.</p>
						<form class="email-form" onsubmit={verifyCode}>
							<label for="email-code">Verification code</label>
							<input id="email-code" name="code" bind:value={code} type="text" inputmode="numeric" autocomplete="one-time-code" pattern={'[0-9]{6}'} minlength="6" maxlength="6" required aria-describedby="code-help" disabled={busy} />
							<button class="button button-dark full-width" type="submit" disabled={busy}>{busy ? 'Verifying…' : 'Verify and sign in'}</button>
						</form>
						<div class="login-actions">
							<button class="button button-light full-width" type="button" onclick={() => sendCode()} disabled={busy || cooldown > 0}>{cooldown > 0 ? `Resend code in ${cooldown}s` : 'Resend code'}</button>
							<button class="text-link" type="button" disabled={busy} onclick={() => { codeSent = false; code = ''; error = ''; }}>Use a different email</button>
						</div>
					{:else}
						<h2>Sign in with email</h2>
						<p>We’ll send a one-time code. No password needed.</p>
						<p class="signup-note">New to WAM? Your first verified code creates an account. Returning members use the same email address to sign in.</p>
						<form class="email-form" onsubmit={sendCode}>
							<label for="login-email">Email address</label>
							<input id="login-email" name="email" bind:value={email} type="email" autocomplete="email" maxlength="254" required disabled={busy} />
							<button class="button button-dark full-width" type="submit" disabled={busy || cooldown > 0}>{busy ? 'Sending code…' : cooldown > 0 ? `Try again in ${cooldown}s` : 'Send sign-in code'}</button>
						</form>
					{/if}
				{/if}
				{#if config?.github_oauth_enabled}
					{#if config?.email_auth_enabled}<p class="login-divider">or continue with GitHub</p>{:else}<h2>Sign in with GitHub</h2>{/if}
					<a class="button button-light full-width" href={apiUrl('/v1/auth/github/start' + (oauthRequest ? '?oauth_request=' + encodeURIComponent(oauthRequest) : linkCode ? '?link_code=' + encodeURIComponent(linkCode) : ''))}>Connect GitHub account <span aria-hidden="true">↗</span></a>
				{/if}
				{#if !config?.email_auth_enabled && !config?.github_oauth_enabled}
					<h2>Sign-in is temporarily unavailable.</h2>
					<p>Please try again later. You can still browse public memories.</p>
				{/if}
			{/if}
			<div class="login-footnote">Signing out does not revoke device tokens.</div>
		</div>

		{#if import.meta.env.DEV}
			<div class="soft-card demo-access">
				<span class="section-label">LOCAL DEMO</span>
				<h2>Preview with a demo account</h2>
				<p>For local development, enter the local server's WAM_DEMO_TOKEN in the dashboard. It is not a production credential.</p>
				<a class="button button-light full-width" href="/dashboard">Open demo dashboard <span aria-hidden="true">↗</span></a>
				<a class="text-link" href="/memories">Browse memories without signing in →</a>
			</div>
		{/if}
	</div>
</section>

<style>
	.page-heading { display: block; }
	.page-heading h1 { font-size: clamp(36px, 5.5vw, 52px); max-width: 100%; }
	.page-heading p { margin-top: 18px; max-width: 100%; }
	.login-actions .text-link { border: 0; background: none; padding: 8px; font: inherit; font-size: 12px; text-decoration: underline; cursor: pointer; }
	.email-form { display: grid; gap: 12px; margin-top: 20px; text-align: left; }
	.email-form label { font-size: 13px; font-weight: 600; color: #203b2d; }
	.email-form input { width: 100%; box-sizing: border-box; padding: 13px; border: 1px solid #b7c9bc; border-radius: 9px; font: inherit; background: white; }
	.email-form input:focus-visible { outline: 2px solid #27633e; outline-offset: 2px; }
	.login-divider { margin: 22px 0 14px; font-size: 12px; }
	.signup-note { margin-top: 12px; }
	.login-actions { display: grid; gap: 9px; margin-top: 18px; }
	.login-error { margin-bottom: 14px; }
	.demo-access { display: grid; gap: 12px; margin-top: 14px; padding: 22px; }
	.demo-access h2 { margin: 0; color: #203b2d; font-size: 18px; }
	.demo-access p { margin: 0; color: #52665b; font-size: 12px; line-height: 1.7; }
	.demo-access .text-link { justify-self: center; }
</style>
