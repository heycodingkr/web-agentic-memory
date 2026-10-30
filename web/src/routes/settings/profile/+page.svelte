<script lang="ts">
	import RichBioEditor from '#lib/components/RichBioEditor.svelte';
	import BioView from '#lib/components/BioView.svelte';
	import { onMount } from 'svelte';
	import { api, safeExternalUrl, setCsrfToken, type AuthSession, type ContributionConsent, type BioBlock, type Dashboard, type Profile, type ServiceLink } from '#lib/api';

	let token = $state('');
	let profile = $state<Profile | null>(null);
	let dashboard = $state<Dashboard | null>(null);
	let handle = $state('');
	let bio = $state<BioBlock[]>([]);
	let links = $state<ServiceLink[]>([]);
	let publicOptIn = $state(false);
	let serviceLabel = $state('');
	let serviceUrl = $state('');
	let error = $state('');
	let notice = $state('');
	let busy = $state(false);
	let signedIn = $state(false);
	let checkingSession = $state(true);
	let sharing = $state<ContributionConsent | null>(null);
	let privacyBusy = $state(false);
	let privacyError = $state('');
	let privacyNotice = $state('');
	let deletionText = $state('');
	let deletionAccepted = $state(false);
	let handleError = $derived(handle.length < 3 || handle.length > 24 || !/^[A-Za-z0-9][A-Za-z0-9_-]*[A-Za-z0-9]$/.test(handle));

	async function loadProfile() {
		error = ''; notice = ''; busy = true;
		try {
			const [saved, rank, consent] = await Promise.all([
				api<Profile>('/v1/me/profile', {}, token || undefined),
				api<Dashboard>('/v1/me/dashboard', {}, token || undefined),
				api<ContributionConsent>('/v1/me/consent', {}, token || undefined)
			]);
			profile = saved;
			sharing = consent;
			handle = saved.handle;
			bio = saved.bio;
			links = saved.service_links;
			publicOptIn = saved.public_opt_in;
			dashboard = rank;
		} catch (cause) { error = cause instanceof Error ? cause.message : 'Could not load account details.'; }
		finally { busy = false; }
	}

	function addServiceLink() {
		const label = serviceLabel.trim();
		const url = serviceUrl.trim();
		if (!label || !url || links.length >= 5) return;
		links = [...links, { label, url }];
		serviceLabel = '';
		serviceUrl = '';
	}

	async function saveProfile(event: SubmitEvent) {
		event.preventDefault();
		error = ''; notice = ''; busy = true;
		try {
			profile = await api<Profile>('/v1/me/profile', {
				method: 'PUT',
				body: JSON.stringify({ handle, bio, service_links: links, public_opt_in: publicOptIn })
			}, token || undefined);
			handle = profile.handle;
			notice = 'Profile saved.';
		} catch (cause) { error = cause instanceof Error ? cause.message : 'Could not save the profile.'; }
		finally { busy = false; }
	}

	async function changeSharing(enabled: boolean) {
		privacyBusy = true; privacyError = ''; privacyNotice = '';
		try {
			sharing = await api<ContributionConsent>('/v1/me/consent', {
				method: 'PUT',
				body: JSON.stringify({ enabled, consent_version: sharing?.consent_version ?? 'public-sharing-v1' })
			}, token || undefined);
			privacyNotice = enabled ? 'Contributions enabled. Each submission still requires your consent.' : 'Contribution consent withdrawn. Pending payloads and your attribution have been removed.';
			if (!enabled) dashboard = await api<Dashboard>('/v1/me/dashboard', {}, token || undefined);
		} catch (cause) { privacyError = cause instanceof Error ? cause.message : 'Could not update contribution consent.'; }
		finally { privacyBusy = false; }
	}

	async function deleteAccount(event: SubmitEvent) {
		event.preventDefault();
		if (deletionText !== 'DELETE' || !deletionAccepted) return;
		privacyBusy = true; privacyError = ''; privacyNotice = '';
		try {
			await api<{ deleted: boolean }>('/v1/me/account', {
				method: 'DELETE', body: JSON.stringify({ confirmation: deletionText })
			}, token || undefined);
			setCsrfToken();
			token = ''; profile = null; dashboard = null; sharing = null; signedIn = false;
			handle = ''; bio = []; links = [];
			privacyNotice = 'Your account has been deleted and its sessions and connected devices have been signed out.';
		} catch (cause) { privacyError = cause instanceof Error ? cause.message : 'Could not delete your account.'; }
		finally { privacyBusy = false; }
	}

	onMount(async () => {
		try {
			const session = await api<AuthSession>('/v1/auth/session');
			setCsrfToken(session.csrf_token);
			signedIn = true;
			await loadProfile();
		} catch (cause) {
			setCsrfToken();
			if (cause instanceof Error && 'status' in cause && cause.status !== 401) error = cause.message;
		} finally {
			checkingSession = false;
		}
	});
</script>

<svelte:head>
	<title>Profile settings — Web Agentic Memory</title>
	<meta name="description" content="Manage your handle, rich-text bio, service links, and public visibility." />
</svelte:head>

<section class="page-shell">
	<div class="container">
		<div class="page-heading">
			<span class="eyebrow">PROFILE SETTINGS</span>
			<h1>Leave a path in your name.</h1>
			<p>Set a handle, write a bio, add service links, and choose whether to appear on the leaderboard.</p>
			<div class="demo-note">Use a GitHub session or local demo token. Public profile visibility is optional.</div>
		</div>

		<nav class="dashboard-nav" aria-label="Account navigation"><a href="/dashboard">My rank</a><a class="active" href="/settings/profile">Profile settings</a><a href="/settings/devices">Devices</a></nav>

		{#if checkingSession}
			<section class="soft-card auth-panel" aria-label="Profile authentication"><p>Checking your sign-in…</p></section>
		{:else if signedIn}
			<section class="soft-card auth-panel" aria-label="Profile authentication"><p>Signed in with GitHub. Load your profile and public settings.</p><button class="button button-dark button-small" type="button" onclick={loadProfile} disabled={busy}>{busy ? 'Loading…' : 'Refresh profile'}</button><a class="text-link" href="/login">Account settings →</a></section>
		{:else}
		<section class="soft-card auth-panel" aria-label="Profile authentication">
			<label for="demo-token">Local demo token</label>
			<div class="auth-row">
				<input id="demo-token" type="password" bind:value={token} autocomplete="off" placeholder="Value of WAM_DEMO_TOKEN" />
				<button class="button button-dark button-small" type="button" onclick={loadProfile} disabled={!token || busy}>{busy ? 'Loading…' : 'Load my profile'}</button>
			</div>
			<p>The token is sent in the request header and is not stored in the browser.</p>
		</section>

			<a class="text-link" href="/login">Sign in with GitHub →</a>
		{/if}
		{#if dashboard}
			<section class="rank-strip" aria-label="My rank">
				<div><small>Overall rank</small><strong>#{dashboard.overall_rank ?? '—'}</strong></div>
				<div><small>Public rank</small><strong>{dashboard.public_rank ? `#${dashboard.public_rank}` : 'Private'}</strong></div>
				<div><small>Total points</small><strong>{dashboard.total_points.toLocaleString('en-US')} pts</strong></div>
			</section>
		{/if}

		{#if profile || token || signedIn}
			<div class="profile-edit-grid">
				<form class="soft-card form-panel" onsubmit={saveProfile}>
					<div class="field">
						<label for="profile-handle">Public handle</label>
						<div class="handle-input"><span>@</span><input id="profile-handle" bind:value={handle} maxlength="24" autocomplete="off" /></div>
						<span class="field-help">3–24 characters. Start and end with a letter or number; underscores and hyphens are allowed.</span>
						{#if handle && handleError}<span class="field-error" role="status">Check the handle format. Reserved or duplicate handles are rejected by the server.</span>{/if}
					</div>

					<RichBioEditor value={bio} onchange={(next) => (bio = next)} />

					<div class="field">
						<span class="field-label">Service links</span>
						<p class="field-help">Up to five links. They open in a new tab on your public profile. HTTPS only; WAM does not fetch URLs for previews.</p>
						{#each links as link, index}
							<div class="service-row"><span><strong>{link.label}</strong><small>{link.url}</small></span><button class="icon-button" type="button" aria-label={`Remove ${link.label} link`} onclick={() => (links = links.filter((_, i) => i !== index))}>Remove</button></div>
						{/each}
						<div class="add-link">
							<label><span>Link label</span><input bind:value={serviceLabel} maxlength="48" placeholder="e.g. My website" /></label>
							<label><span>HTTPS URL</span><input bind:value={serviceUrl} type="url" inputmode="url" placeholder="https://service.example.com" /></label>
							<button class="button button-light button-small" type="button" onclick={addServiceLink} disabled={links.length >= 5}>Add link</button>
						</div>
					</div>

					<label class="consent"><input type="checkbox" bind:checked={publicOptIn} /><span>Show my handle, total points, and public profile on the leaderboard. Turning this off removes them from both public views immediately.</span></label>
					<div class="form-submit-row"><button class="button button-dark" disabled={handleError || (!token && !signedIn) || busy} type="submit">{busy ? 'Saving…' : 'Save profile'}</button>{#if profile}<a class="text-link" href={'/u/' + encodeURIComponent(profile.handle)}>View public profile ↗</a>{/if}</div>
					{#if notice}<div class="success-box" role="status">{notice}</div>{/if}
					{#if error}<div class="field-error" role="alert">{error}</div>{/if}
				</form>

				<aside class="soft-card public-profile-preview">
					<span class="section-label">LIVE PREVIEW</span>
					<div class="avatar profile-avatar">{(handle || 'u').slice(0, 1).toUpperCase()}</div>
					<h2>@{handle || 'your-handle'}</h2>
					<BioView blocks={bio} />
					{#if links.length}
						<div class="profile-link-preview">
							{#each links as link}
								{#if safeExternalUrl(link.url)}<a href={link.url} target="_blank" rel="noopener noreferrer">{link.label} ↗</a>{:else}<span>{link.label} · Check the HTTPS URL</span>{/if}
							{/each}
						</div>
					{/if}
					{#if publicOptIn}<span class="pill pill-green">Public on leaderboard</span>{:else}<span class="pill">Private</span>{/if}
				</aside>
			</div>
		{/if}
        {#if profile && sharing}
            <section class="soft-card form-panel privacy-panel" aria-labelledby="privacy-heading">
                <h2 id="privacy-heading">Sharing and account privacy</h2>
                <p class="field-help">Nothing is shared until you consent to an individual submission. Contribution payloads are removed after {sharing.retention_days} days. Withdrawing removes your submitted payload and attribution immediately. Shared workflows with independent support can remain public.</p>
                <div class="form-submit-row">
                    <span class="pill">{sharing.enabled ? 'Contributions enabled' : 'Contribution consent withdrawn'}</span>
                    <button class="button button-light button-small" type="button" disabled={busy || privacyBusy} onclick={() => changeSharing(!sharing?.enabled)}>
                        {sharing.enabled ? 'Withdraw contribution consent' : 'Enable contributions'}
                    </button>
                </div>
                <details>
                    <summary>Delete my account</summary>
                    <form class="deletion-form" onsubmit={deleteAccount}>
                        <p class="field-help">Delete your profile and sign out all sessions and connected devices. Your contributions lose your attribution. Workflows without independent support are hidden. Minimal anonymous score and audit facts remain.</p>
                        <label class="consent"><input type="checkbox" bind:checked={deletionAccepted} /><span>I understand that deleting this account cannot be undone.</span></label>
                        <div class="field"><label for="deletion-confirmation">Type DELETE to confirm</label><input id="deletion-confirmation" bind:value={deletionText} autocomplete="off" /></div>
                        <button class="button button-dark" type="submit" disabled={busy || privacyBusy || deletionText !== 'DELETE' || !deletionAccepted}>{privacyBusy ? 'Processing…' : 'Delete account'}</button>
                    </form>
                </details>
            </section>
        {/if}
        {#if privacyNotice}<div class="success-box privacy-message" role="status">{privacyNotice}</div>{/if}
        {#if privacyError}<div class="field-error privacy-message" role="alert">{privacyError}</div>{/if}
	</div>
</section>

<style>
	.privacy-panel { margin-top: 24px; max-width: 760px; }
	.privacy-panel h2 { margin: 0; color: #203b2d; font-size: 20px; }
	.privacy-panel summary { color: #9a4f3f; cursor: pointer; }
	.deletion-form { display: grid; gap: 16px; margin-top: 16px; }
	.privacy-message { margin-top: 16px; max-width: 760px; }
	.auth-panel { max-width: 760px; padding: 18px 20px; }
	.auth-panel label, .field > label, .add-link label { display: grid; gap: 7px; color: #586a5e; font-size: 11px; font-weight: 700; }
	.auth-row { display: flex; gap: 10px; margin-top: 8px; }
	.auth-row input, .field input, .add-link input { width: 100%; min-width: 0; min-height: 42px; padding: 10px 12px; border: 1px solid #dce2d9; border-radius: 6px; color: #18312a; background: #fffef9; font: inherit; }
	.auth-row input { flex: 1; }
	.auth-panel p, .field-help { color: #849087; font-size: 10px; line-height: 1.65; margin: 8px 0 0; }
	.rank-strip { display: grid; grid-template-columns: repeat(3, 1fr); margin: 16px 0; border: 1px solid #dce2d9; border-radius: 9px; background: #fffef9; }
	.rank-strip div { display: grid; gap: 4px; padding: 15px 19px; border-right: 1px solid #e5e9e1; }
	.rank-strip div:last-child { border: 0; }
	.rank-strip small { color: #7b8980; font-size: 10px; }
	.rank-strip strong { color: #315847; font: 600 17px Manrope, sans-serif; }
	.profile-edit-grid { display: grid; grid-template-columns: minmax(0, 1.15fr) minmax(260px, .85fr); gap: 20px; align-items: start; margin-top: 18px; }
	.form-panel { padding: 25px; display: grid; gap: 22px; }
	.field { display: grid; gap: 8px; }
	.handle-input { display: flex; align-items: center; gap: 8px; border: 1px solid #dce2d9; border-radius: 6px; padding-left: 12px; background: #fffef9; color: #718078; }
	.handle-input input { border: 0; background: transparent; }
	.field-label { color: #586a5e; font-size: 11px; font-weight: 700; }
	.service-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 10px 0; border-bottom: 1px solid #edf0eb; }
	.service-row span { display: grid; gap: 4px; min-width: 0; }
	.service-row strong { color: #4f6256; font-size: 12px; }
	.service-row small { color: #839087; font-size: 10px; overflow-wrap: anywhere; }
	.icon-button { border: 0; background: transparent; color: #995a47; cursor: pointer; font-size: 11px; }
	.add-link { display: grid; grid-template-columns: 1fr 1.4fr auto; align-items: end; gap: 8px; margin-top: 13px; }
	.add-link label span { color: #77847c; font-size: 9px; }
	.consent { display: flex; align-items: start; gap: 10px; color: #607067; font-size: 11px; line-height: 1.7; }
	.consent input { accent-color: #4c7455; margin-top: 3px; }
	.form-submit-row { display: flex; align-items: center; gap: 16px; flex-wrap: wrap; }
	.success-box { padding: 12px; border-radius: 5px; color: #315847; background: #eef5e6; font-size: 12px; }
	.field-error { color: #9a4f3f; font-size: 11px; }
	.public-profile-preview { padding: 25px; }
	.profile-avatar { display: grid; place-items: center; width: 54px; height: 54px; margin: 21px 0 12px; border-radius: 16px; color: #315847; background: #e5eedc; font: 700 20px Manrope, sans-serif; }
	.public-profile-preview h2 { margin-bottom: 14px; font: 600 19px Manrope, sans-serif; }
	.profile-link-preview { display: grid; gap: 8px; margin: 20px 0; padding-top: 14px; border-top: 1px solid #e5e9e1; }
	.profile-link-preview a { color: #315d4a; font-size: 12px; }
	.profile-link-preview span { color: #9a4f3f; font-size: 11px; }
	@media (max-width: 760px) { .profile-edit-grid { grid-template-columns: 1fr; } .add-link { grid-template-columns: 1fr; } .rank-strip div { padding: 12px; } }
</style>
