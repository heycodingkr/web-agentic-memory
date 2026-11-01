<script lang="ts">
    import { onMount } from 'svelte';
    import { adminApi } from '#lib/admin-api';
    let token = $state('');
    let busy = $state(false);
    let error = $state('');
    let ready = $state(false);
    onMount(() => {
        token = new URLSearchParams(window.location.hash.slice(1)).get('token') || '';
        history.replaceState(null, '', window.location.pathname);
        ready = true;
    });
    async function verify() {
        busy = true; error = '';
        try {
            await adminApi('/v1/admin/auth/verify', { method: 'POST', body: JSON.stringify({ token }) });
            token = '';
            window.location.replace('/admin');
        } catch (cause) {
            error = 'This link has expired or has already been used. Open it in the browser where you requested sign-in, or request a new link.';
        } finally { busy = false; }
    }
</script>
<svelte:head><title>Confirm administrator sign-in · WAM</title><meta name="robots" content="noindex, nofollow" /><meta name="referrer" content="no-referrer" /></svelte:head>
<div class="verify-shell"><a class="wordmark" href="/admin">WAM <span>ADMIN</span></a><section class="verify-card">
    <span class="eyebrow">ADMINISTRATOR ACCESS</span><h1>Confirm administrator sign-in</h1>
    <p>If you requested sign-in, select the button below to open the contribution review console.</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if ready && !token}<p class="error" role="alert">No verification link found. Request a new sign-in email.</p>{/if}
    <button class="button button-dark" type="button" onclick={verify} disabled={!ready || !token || busy}>{busy ? 'Verifying…' : 'Complete administrator sign-in'}</button>
    <a href="/admin">Request a new sign-in link</a>
</section><p class="footnote">This link expires after 10 minutes and can be used once.</p></div>
<style>
.verify-shell{min-height:100vh;display:flex;flex-direction:column;justify-content:center;align-items:center;padding:28px;background:#f4f5ed;color:#203b2d}.wordmark{font-weight:800;font-size:24px;margin-bottom:26px}.wordmark span{font-size:11px;letter-spacing:.14em;margin-left:8px;color:#5a6e5b}.verify-card{width:100%;max-width:460px;border:1px solid #dde3d5;border-radius:18px;background:#fffef9;padding:36px;display:grid;gap:18px}.verify-card h1{margin:0;font-size:27px;letter-spacing:-.04em}.verify-card p{font-size:14px;line-height:1.7;margin:0}.verify-card>a{font-size:13px;text-align:center;text-decoration:underline}.error{color:#973b32}.footnote{font-size:12px;color:#53664f;margin-top:22px}
</style>
