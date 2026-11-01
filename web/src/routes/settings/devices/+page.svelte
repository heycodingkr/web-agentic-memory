<script lang="ts">
	import { onMount } from 'svelte';
	import { api, ApiError, type AuthSession, type ConnectedDevice, type DeviceList, type RotatedDevice } from '#lib/api';
	import { browserSession, loadBrowserSession, setBrowserSession } from '#lib/browser-session';

	let session = $state<AuthSession | null>(null);
	let devices = $state<ConnectedDevice[]>([]);
	let loading = $state(true);
	let busyId = $state('');
	let error = $state('');
	let notice = $state('');
	let rotatedToken = $state('');
	let rotatedDeviceName = $state('');
	let copied = $state(false);
	let rotatedDeviceId = $state('');
	let refreshing = $state(false);
	// A rotated token has one display slot, so device changes must finish in order.
	const deviceBusy = $derived(busyId !== '' || refreshing);
	let active = true;
	let generation = 0;

	function current(requestGeneration: number) {
		return active && requestGeneration === generation && session !== null;
	}

	function clearToken() {
		rotatedToken = '';
		rotatedDeviceName = '';
		rotatedDeviceId = '';
		copied = false;
	}

	function expireAuthentication() {
		++generation;
		session = null;
		devices = [];
		busyId = '';
		refreshing = false;
		notice = '';
		clearToken();
		error = 'Your session expired. Sign in again to manage your devices.';
		setBrowserSession(null);
	}

	function showError(cause: unknown, fallback: string) {
		if (cause instanceof ApiError && cause.status === 401) expireAuthentication();
		else error = cause instanceof Error ? cause.message : fallback;
	}

	onMount(() => {
		const unsubscribe = browserSession.subscribe(value => {
			if (!value && session) expireAuthentication();
		});
		void (async () => {
			try {
				const value = await loadBrowserSession();
				if (!active) return;
				session = value;
				if (session) await loadDevices();
			} catch (cause) {
				if (active) showError(cause, 'Could not check your session.');
			} finally {
				if (active) loading = false;
			}
		})();
		return () => { active = false; ++generation; unsubscribe(); clearToken(); };
	});

	async function loadDevices() {
		if (!session || refreshing) return;
		const requestGeneration = generation;
		refreshing = true;
		error = '';
		try {
			const result = await api<DeviceList>('/v1/me/devices');
			if (current(requestGeneration)) devices = result.devices;
		} catch (cause) {
			if (current(requestGeneration)) showError(cause, 'Could not load the device list.');
		} finally {
			if (current(requestGeneration)) refreshing = false;
		}
	}

	async function rotate(device: ConnectedDevice) {
		if (!session || deviceBusy) return;
		const requestGeneration = generation;
		busyId = device.id;
		error = '';
		notice = '';
		try {
			const path = '/v1/me/devices/' + encodeURIComponent(device.id) + '/rotate';
			const result = await api<RotatedDevice>(path, { method: 'POST' });
			if (!current(requestGeneration)) return;
			rotatedToken = result.access_token;
			rotatedDeviceName = result.device.name;
			rotatedDeviceId = result.device.id;
			copied = false;
			notice = 'Device token rotated. You can view the new value on this screen only.';
			await loadDevices();
		} catch (cause) {
			if (current(requestGeneration)) showError(cause, 'Could not rotate the token.');
		} finally {
			if (current(requestGeneration)) busyId = '';
		}
	}

	async function revoke(device: ConnectedDevice) {
		if (!session || deviceBusy) return;
		if (!window.confirm('Revoke the connection for ' + device.name + '? This token will be rejected on the next request.')) return;
		const requestGeneration = generation;
		busyId = device.id;
		error = '';
		notice = '';
		try {
			const path = '/v1/me/devices/' + encodeURIComponent(device.id) + '/revoke';
			await api<void>(path, { method: 'POST' });
			if (!current(requestGeneration)) return;
			if (rotatedDeviceId === device.id) clearToken();
			notice = 'Revoked the connection for ' + device.name + '.';
			await loadDevices();
		} catch (cause) {
			if (current(requestGeneration)) showError(cause, 'Could not revoke the device.');
		} finally {
			if (current(requestGeneration)) busyId = '';
		}
	}

	async function copyToken() {
		if (!session || !rotatedToken) return;
		const requestGeneration = generation;
		const value = rotatedToken;
		try {
			await navigator.clipboard.writeText(value);
			if (current(requestGeneration) && value === rotatedToken) copied = true;
		} catch {
			if (current(requestGeneration) && value === rotatedToken) error = 'Could not copy to the clipboard. Select the token field and copy it manually.';
		}
	}
</script>

<svelte:head>
	<title>Connected devices — Web Agentic Memory</title>
	<meta name="description" content="Review permissions and tokens for connected WAM devices." />
</svelte:head>

<section class="page-shell">
	<div class="container">
		<div class="page-heading">
			<span class="eyebrow">CONNECTED DEVICES</span>
			<h1>Manage connected devices.</h1>
			<p>WAM stores only a hash of each device token. Copy a rotated token now and update it in the CLI's secure credential storage.</p>
		</div>

		<nav class="dashboard-nav" aria-label="Account navigation"><a href="/dashboard">Dashboard</a><a href="/settings/profile">Profile settings</a><a class="active" href="/settings/devices">Devices</a></nav>

		{#if loading}
			<div class="soft-card device-panel"><p class="status-empty">Checking your account and devices…</p></div>
		{:else if !session}
			<div class="soft-card device-panel">
				<h2>Sign in to continue.</h2>
				<p>Only the account owner can view or revoke connected devices.</p>
				<a class="button button-dark" href="/login?return_to=%2Fsettings%2Fdevices">Go to sign in <span aria-hidden="true">↗</span></a>
			</div>
		{:else}
			<div class="device-page-grid">
				<section class="soft-card device-panel">
					<div class="panel-heading"><div><h2>My devices</h2><p>Review permissions and expiry for approved tokens.</p></div><a class="button button-dark button-small" href="/link">Link a device <span>↗</span></a></div>
					{#if devices.length}
						<div class="device-list">
							{#each devices as device (device.id)}
								<article class:revoked={device.revoked} class="device-row">
									<div class="device-main">
										<div class="device-title"><strong>{device.name}</strong><span class:inactive={device.revoked} class="pill">{device.revoked ? 'Revoked or expired' : 'Connected'}</span></div>
										<p>Permissions: {device.scopes.join(' · ')}</p>
										<small>Expires {new Date(device.expires_at * 1000).toLocaleString('en-US')} · Last used {device.last_seen_at ? new Date(device.last_seen_at * 1000).toLocaleString('en-US') : 'Never'}</small>
									</div>
									{#if !device.revoked}
										<div class="device-actions">
											{#if device.oauth_managed}<span class="field-help">Renewed by your MCP host</span>{:else}<button class="button button-light button-small" type="button" onclick={() => rotate(device)} disabled={deviceBusy}>{busyId === device.id ? 'Working…' : 'Rotate token'}</button>{/if}
											<button class="button button-danger button-small" type="button" onclick={() => revoke(device)} disabled={deviceBusy}>Revoke</button>
										</div>
									{/if}
								</article>
							{/each}
						</div>
					{:else}
						<p class="status-empty">No connected devices. Create a link code in the CLI, then connect a device here.</p>
					{/if}
				</section>
				<aside class="notice-box"><strong>Tokens are shown once.</strong> Do not save them in the browser or logs. Enter them in the CLI's operating system credential store. Signing out does not revoke a device.</aside>
			</div>
		{/if}

		{#if rotatedToken}
			<section class="soft-card rotated-token" aria-live="polite">
				<span class="section-label">NEW DEVICE TOKEN</span>
				<h2>Token rotated for {rotatedDeviceName}</h2>
				<p>Once you close this view, the server cannot show the token again. The old token has already been revoked.</p>
				<div class="token-row"><input aria-label="New device token" readonly value={rotatedToken} /><button class="button button-dark button-small" type="button" onclick={copyToken}>{copied ? 'Copied' : 'Copy'}</button></div>
			</section>
		{/if}
		{#if notice}<div class="notice-box device-notice" role="status">{notice}</div>{/if}
		{#if error}<div class="notice-box device-error" role="alert">{error}{#if session}<button class="button button-light button-small" type="button" onclick={loadDevices} disabled={deviceBusy}>Try again</button>{/if}</div>{/if}
	</div>
</section>

<style>
	.device-panel { padding: 24px; }
	.device-panel h2 { margin: 0; color: #203b2d; font-size: 19px; }
	.device-panel .panel-heading p { margin: 7px 0 0; color: #829087; font-size: 11px; }
	.device-list { display: grid; }
	.device-row { display: grid; grid-template-columns: 1fr auto; align-items: center; gap: 18px; padding: 17px 0; border-top: 1px solid #e8ece6; }
	.device-row.revoked { opacity: .68; }
	.device-main { min-width: 0; }
	.device-title { display: flex; align-items: center; flex-wrap: wrap; gap: 9px; }
	.device-title strong { color: #315847; font-size: 13px; }
	.device-main p { margin: 7px 0; color: #6d7b72; font-size: 11px; overflow-wrap: anywhere; }
	.device-main small { color: #89958d; font-size: 10px; }
	.pill.inactive { color: #77847b; background: #edf0eb; }
	.device-actions { display: flex; flex-wrap: wrap; gap: 7px; }
	.button-danger { min-height: 33px; padding: 7px 11px; border: 1px solid #ead3cb; border-radius: 5px; color: #9a4f3f; background: #fff8f5; font-size: 10px; cursor: pointer; }
	.rotated-token { display: grid; gap: 11px; margin-top: 14px; padding: 23px; }
	.rotated-token h2 { margin: 0; color: #203b2d; font-size: 18px; }
	.rotated-token p { margin: 0; color: #77847b; font-size: 11px; line-height: 1.7; }
	.token-row { display: flex; gap: 8px; }
	.token-row input { flex: 1; min-width: 0; padding: 9px; border: 1px solid #dce2d9; border-radius: 5px; color: #244f3f; font: 12px monospace; }
	.device-notice, .device-error { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-top: 14px; }
	@media (max-width: 700px) { .device-row { grid-template-columns: 1fr; } .device-actions { justify-content: flex-start; } }
</style>
