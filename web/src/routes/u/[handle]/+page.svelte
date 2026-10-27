<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import BioView from '#lib/components/BioView.svelte';
	import { api, safeExternalUrl, type Profile } from '#lib/api';

	let profile = $state<Profile | null>(null);
	let error = $state('');
	let busy = $state(true);
	const handle = $derived(page.params.handle ?? '');

	async function loadProfile(value: string) {
		profile = null;
		error = '';
		busy = true;
		try { profile = await api<Profile>(`/v1/profiles/${encodeURIComponent(value)}`); }
		catch (cause) { error = cause instanceof Error ? cause.message : 'Could not load the profile.'; }
		finally { busy = false; }
	}

	onMount(() => { void loadProfile(handle); });
</script>

<svelte:head>
	<title>{profile ? `@${profile.handle} — Web Agentic Memory` : 'Public profile — Web Agentic Memory'}</title>
	<meta name="description" content={profile?.handle ? `@${profile.handle}'s public profile` : 'Public contributor profile'} />
</svelte:head>

<section class="page-shell">
	<div class="narrow public-profile-page">
		<a class="text-link" href="/leaderboard"><span aria-hidden="true">←</span> Back to leaderboard</a>
		{#if profile}
			<div class="soft-card public-profile-card">
				<div class="public-profile-header">
					<div class="avatar profile-avatar">{profile.handle.slice(0, 1).toUpperCase()}</div>
					<div><span class="pill pill-green">Public contributor</span><h1>@{profile.handle}</h1></div>
				</div>
				{#if profile.bio.length}<BioView blocks={profile.bio} />{:else}<p class="bio-empty">This contributor has not added a bio yet.</p>{/if}
				{#if profile.service_links.length}
					<div class="public-links">
						<h2>Services and work</h2>
						{#each profile.service_links as link}
							{#if safeExternalUrl(link.url)}
								<a href={link.url} target="_blank" rel="noopener noreferrer">{link.label}<span aria-hidden="true">↗</span></a>
							{/if}
						{/each}
					</div>
				{/if}
			</div>
		{:else}
			<div class="soft-card public-profile-card">
				<p role={error ? 'alert' : 'status'}>{error || (busy ? 'Loading profile…' : 'Profile not found.')}</p>
			</div>
		{/if}
	</div>
</section>

<style>
	.public-profile-card { margin-top: 24px; padding: clamp(26px, 5vw, 52px); }
	.profile-avatar { width: 62px; height: 62px; display: grid; place-items: center; margin-bottom: 22px; border-radius: 18px; color: #315847; background: #e5eedc; font: 700 24px Manrope, sans-serif; }
	.public-profile-card h1 { margin: 16px 0 20px; font: 600 clamp(34px, 6vw, 58px)/1.05 Manrope, sans-serif; letter-spacing: -.06em; }
	.public-links { margin-top: 28px; padding-top: 18px; border-top: 1px solid #e6ebe4; }
	.public-links h2 { font: 600 13px Manrope, sans-serif; }
	.public-links a { display: flex; justify-content: space-between; max-width: 420px; padding: 12px 0; color: #315d4a; border-bottom: 1px solid #edf0eb; font-size: 13px; }
</style>
