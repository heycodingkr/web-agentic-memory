<script lang="ts">
    import { onMount } from 'svelte';
    import { api, setCsrfToken, type AuthConfig, type AuthSession } from '#lib/api';

    interface Preview { client_name: string; redirect_uri: string; resource: string; scopes: string[]; expires_at: number }
    let requestId = $state('');
    let preview = $state<Preview | null>(null);
    let handle = $state('');
    let signedIn = $state(false);
    let authConfig = $state<AuthConfig | null>(null);
    let loading = $state(true);
    let busy = $state(false);
    let error = $state('');
    const labels: Record<string, string> = {
        'read:memories': 'Read public memories',
        'read:dashboard': 'Read your profile, rank, usage, and score history',
        'write:contributions': 'Submit or withdraw your contributions',
        'report:outcome': 'Report outcomes for your receipts'
    };

    onMount(async () => {
        if (window.top !== window.self) {
            error = 'Open this authorization page in its own browser window.';
            loading = false;
            return;
        }
        requestId = new URLSearchParams(window.location.search).get('request') || '';
        if (!/^[A-Za-z0-9_-]{43}$/.test(requestId)) {
            error = 'This connection request is invalid or expired.';
            loading = false;
            return;
        }
        try { authConfig = await api<AuthConfig>('/v1/auth/config'); } catch { /* Session check below still allows existing sessions. */ }
        try {
            const session = await api<AuthSession>('/v1/auth/session');
            setCsrfToken(session.csrf_token);
            signedIn = true;
        } catch {
            setCsrfToken();
            loading = false;
            return;
        }
        try {
            const [profile, request] = await Promise.all([
                api<{ handle: string }>('/v1/me/profile'),
                api<Preview>('/v1/oauth/requests/' + encodeURIComponent(requestId))
            ]);
            handle = profile.handle;
            preview = request;
        } catch {
            error = 'This request could not be loaded. It may have expired or already been used.';
        } finally {
            loading = false;
        }
    });

    async function decide(approve: boolean) {
        if (!preview || busy) return;
        busy = true;
        error = '';
        try {
            const result = await api<{ redirect_uri: string }>(
                '/v1/oauth/requests/' + encodeURIComponent(requestId) + '/decision',
                { method: 'POST', body: JSON.stringify({ approve }) }
            );
            const expected = new URL(preview.redirect_uri);
            const target = new URL(result.redirect_uri);
            if (target.origin !== expected.origin || target.pathname !== expected.pathname) {
                throw new Error('Unexpected return destination');
            }
            window.location.replace(target.href);
        } catch {
            error = 'The decision could not be completed. Return to your app and start the connection again.';
            busy = false;
        }
    }
</script>

<svelte:head>
    <title>Authorize an app — Web Agentic Memory</title>
    <meta name="referrer" content="no-referrer" />
    <meta name="robots" content="noindex, nofollow" />
</svelte:head>

<section class="page-shell">
    <div class="narrow">
        <div class="page-heading">
            <span class="eyebrow">CONNECT AN APP</span>
            <h1>Review access<br />to your account.</h1>
            <p>Approve only a connection you started in your MCP host.</p>
        </div>
        <div class="soft-card approval-card">
            {#if loading}
                <p>Loading the connection request…</p>
            {:else if error}
                <p class="error-message" role="alert">{error}</p>
                <a class="text-link" href="/settings/devices">Manage connected devices</a>
            {:else if !signedIn}
                <h2>Sign in to review this request.</h2>
                <p>You will return here to check the app and permissions before granting access.</p>
                {#if authConfig?.email_auth_enabled}
                    <a class="button button-dark" href={'/login?oauth_request=' + encodeURIComponent(requestId)}>Sign in with email</a>
                {/if}
                {#if authConfig?.github_oauth_enabled}
                    <a class="button button-light" href={'/v1/auth/github/start?oauth_request=' + encodeURIComponent(requestId)}>Sign in with GitHub</a>
                {/if}
                {#if !authConfig?.email_auth_enabled && !authConfig?.github_oauth_enabled}<p>Sign-in is temporarily unavailable. Please try again later.</p>{/if}
            {:else if preview}
                <p class="account-line">Signed in as <strong>@{handle}</strong></p>
                <h2>{preview.client_name}</h2>
                <p class="field-help">This app name is supplied by the requesting client. Check the return destination below.</p>
                <dl>
                    <dt>Return destination</dt><dd><code>{preview.redirect_uri}</code></dd>
                    <dt>WAM service</dt><dd><code>{preview.resource}</code></dd>
                </dl>
                <h3>Requested permissions</h3>
                <ul>{#each preview.scopes as scope}<li>{labels[scope] || scope}</li>{/each}</ul>
                <p>Sharing a contribution still requires your explicit consent. You can revoke this app later in device settings.</p>
                <div class="actions">
                    <button class="button button-dark" type="button" onclick={() => decide(true)} disabled={busy}>{busy ? 'Returning to app…' : 'Allow access'}</button>
                    <button class="button button-light" type="button" onclick={() => decide(false)} disabled={busy}>Deny</button>
                </div>
            {/if}
        </div>
    </div>
</section>

<style>
    .approval-card { display: grid; gap: 18px; padding: 28px; }
    h2 { margin: 0; font-size: 24px; }
    h3 { margin: 8px 0 0; font-size: 14px; }
    p { margin: 0; color: var(--muted, #637266); line-height: 1.7; }
    .account-line { font-size: 13px; }
    dl { display: grid; gap: 8px; margin: 0; }
    dt { font-size: 12px; font-weight: 600; }
    dd { margin: 0 0 10px; overflow-wrap: anywhere; }
    code { font-size: 12px; }
    ul { margin: 0; padding-left: 20px; line-height: 1.9; }
    .actions { display: flex; flex-wrap: wrap; gap: 10px; }
    .error-message { color: #a72e28; }
</style>
