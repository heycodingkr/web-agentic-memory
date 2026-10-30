<script lang="ts">
	import { onMount } from 'svelte';
	import { api, ApiError, type AuthConfig } from '#lib/api';
	import { browserBridgeInstallCommand, defaultMcpEndpoint, normalizeMcpEndpoint, setupCommands, sourceInstallCommands } from '#lib/install';
	import InstallCode from '#lib/components/InstallCode.svelte';

	const hosts = [
		{ id: 'codex', name: 'Codex', mark: 'C' },
		{ id: 'claude-code', name: 'Claude Code', mark: 'Cl' },
		{ id: 'arise', name: 'Arise MCP Hub', mark: 'A' }
	] as const;
	const platforms = ['macOS', 'Linux', 'Windows'];
	const keywordOptions = ['blog', 'draft', 'publish', 'search', 'write'];
	let selectedHost = $state<'codex' | 'claude-code' | 'arise'>('codex');
	let transport = $state<'remote' | 'local'>('remote');
	let selectedPlatform = $state('macOS');
	let mcpEndpoint = $state(import.meta.env.VITE_WAM_MCP_ENDPOINT || '');
	let authConfig = $state<AuthConfig | null>(null);
	let authLoading = $state(true);
	let example = $state<'cli' | 'http'>('cli');
	let wamBinary = $state('');
	let browserBinary = $state('');
	let selectedKeywords = $state<string[]>(['blog', 'write']);
	let task = $state('compose-draft');
	let testUrl = $state('https://example.com/guide');
	let requestState = $state<'idle' | 'loading' | 'success' | 'error'>('idle');
	let requestMessage = $state('');
	let recallResult = $state<unknown>(null);

	const normalizedEndpoint = $derived(normalizeMcpEndpoint(mcpEndpoint));
	const commands = $derived(setupCommands(selectedHost === 'arise' ? 'codex' : selectedHost, normalizedEndpoint, selectedPlatform));
	const hostName = $derived(hosts.find(host => host.id === selectedHost)!.name);
	const oauthAvailable = $derived(!!normalizedEndpoint && authConfig?.mcp_oauth_enabled === true && authConfig.mcp_resource === normalizedEndpoint);
	const missingEndpoint = 'Enter a valid MCP endpoint above to generate this command.';
	const bridgeCommand = $derived(browserBridgeInstallCommand(selectedHost === 'arise' ? 'codex' : selectedHost, normalizedEndpoint, wamBinary, browserBinary || (selectedPlatform === 'macOS' ? '/Applications/Arise.app/Contents/MacOS/agent_browser' : ''), selectedPlatform));
	const recallPreview = $derived.by(() => {
		try {
			const url = new URL(testUrl);
			if (url.protocol !== 'https:' || url.username || url.password) return '';
			return url.origin + url.pathname;
		} catch { return ''; }
	});

	onMount(() => {
		mcpEndpoint = defaultMcpEndpoint(window.location.origin, import.meta.env.VITE_WAM_API_URL || '', import.meta.env.VITE_WAM_MCP_ENDPOINT || '');
		const initialEndpoint = mcpEndpoint;
		let disposed = false;
		void api<AuthConfig>('/v1/auth/config').then(config => {
			if (disposed) return;
			authConfig = config;
			if (!import.meta.env.VITE_WAM_MCP_ENDPOINT && mcpEndpoint === initialEndpoint && config.mcp_resource) {
				mcpEndpoint = config.mcp_resource;
			}
		}).catch(() => { /* Commands remain available if deployment status cannot be checked. */ })
			.finally(() => { if (!disposed) authLoading = false; });
		return () => { disposed = true; };
	});

	async function runRecall(event: SubmitEvent) {
		event.preventDefault();
		if (!recallPreview || requestState === 'loading') return;
		requestState = 'loading';
		requestMessage = '';
		recallResult = null;
		try {
			recallResult = await api('/v1/memories/recall', {
				method: 'POST', credentials: 'omit',
				body: JSON.stringify({ url: recallPreview, task_category: task, keywords: selectedKeywords, limit: 3 })
			});
			requestState = 'success';
		} catch (error) {
			requestState = 'error';
			requestMessage = error instanceof ApiError ? error.message : 'The API could not be reached. Check the service address and connection, then try again.';
		}
	}
</script>

<svelte:head>
	<title>Install & connect — Web Agentic Memory</title>
	<meta name="description" content="Connect Codex, Claude Code, or Arise to WAM. Choose remote MCP or local stdio, verify your tools, and link your account when you want to contribute." />
</svelte:head>

<section class="page-shell setup-page">
	<div class="container">
		<div class="page-heading">
			<span class="eyebrow">Agent setup</span>
			<h1>Connect your agent.<br />Start a step ahead.</h1>
			<p>Connect once to recall shared website workflows in the tools you already use. Start with public reads. Link an account when you want to contribute.</p>
			<div class="demo-note">Public reads · no account required</div>
		</div>

		<nav class="setup-path" aria-label="Setup steps">
			<a href="#connect"><span>01</span><div><strong>Connect your agent</strong><small>Choose a host and transport</small></div><i aria-hidden="true">↗</i></a>
			<a href="#verify"><span>02</span><div><strong>Check the connection</strong><small>Discover tools, then try a recall</small></div><i aria-hidden="true">↗</i></a>
			<a href="#account"><span>03</span><div><strong>Make it yours</strong><small>Optional account linking</small></div><i aria-hidden="true">↗</i></a>
		</nav>

		<div class="guide-grid">
			<div class="guide-main">
				<section id="connect" class="soft-card guide-panel">
					<div class="section-label">01 / CONNECT</div>
					<h2>Where does your agent run?</h2>
					<p class="guide-description">Use your existing agent. The remote connection needs no local WAM download.</p>
					<div class="host-picker" role="group" aria-label="Agent host">
						{#each hosts as host}
							<button type="button" class:chosen={selectedHost === host.id} aria-pressed={selectedHost === host.id} onclick={() => { selectedHost = host.id; if (host.id === 'arise') transport = 'remote'; }}><span class="host-mark" aria-hidden="true">{host.mark}</span>{host.name}<span class="host-check" aria-hidden="true">{selectedHost === host.id ? '✓' : ''}</span></button>
						{/each}
					</div>
					<div class="filter-tabs transport-picker" role="group" aria-label="Connection type">
						<button class="filter-tab" class:active={transport === 'remote'} type="button" aria-pressed={transport === 'remote'} onclick={() => (transport = 'remote')}>Remote MCP <span class="recommended">Recommended</span></button>
						<button class="filter-tab" class:active={transport === 'local'} type="button" aria-pressed={transport === 'local'} disabled={selectedHost === 'arise'} onclick={() => (transport = 'local')}>Local stdio</button>
					</div>
					<p class="field-help">{transport === 'remote' ? 'Install the selected host’s CLI first, then run the command below in your terminal.' : 'Build both local tools below first. A local wam-mcp process calls the WAM service and can use your linked device credential.'}</p>
					<div class="field endpoint-field">
						<label for="mcp-endpoint">WAM MCP endpoint</label>
						<input id="mcp-endpoint" type="url" bind:value={mcpEndpoint} autocomplete="url" spellcheck="false" aria-invalid={mcpEndpoint !== '' && !normalizedEndpoint} aria-describedby="endpoint-help" />
						<span id="endpoint-help" class="field-help">{mcpEndpoint && !normalizedEndpoint ? 'Use an HTTPS URL ending in /mcp, without credentials, query, or fragment. HTTP is allowed for localhost.' : 'Use your service’s /mcp address. Local development: http://127.0.0.1:3000/mcp.'}</span>
					</div>
					<div class="shell-picker" role="group" aria-label="Operating system">
						<span>Terminal</span>{#each platforms as platform}<button type="button" class:chosen={selectedPlatform === platform} aria-pressed={selectedPlatform === platform} onclick={() => (selectedPlatform = platform)}>{platform}</button>{/each}
						{#if selectedPlatform === 'Windows'}<small>PowerShell</small>{/if}
					</div>
					<details class="setup-details source-tools" open={transport === 'local'}>
						<summary>Install local tools from source (CLI or stdio)</summary>
						<p>Install Rust and Cargo, <a href="https://github.com/heycodingkr/web-agentic-memory" target="_blank" rel="noreferrer">get this repository ↗</a>, then open its root directory in your terminal. Build both tools:</p>
						<InstallCode label="Build local tools" code={sourceInstallCommands} />
						<p>Cargo installs <code>wam</code> and <code>wam-mcp</code> into its bin directory. Add that directory to PATH. Signed release installation and Windows/Linux host acceptance are still pending.</p>
					</details>
					<InstallCode label={selectedHost === 'arise' ? 'Arise MCP Hub · server URL' : hostName + (transport === 'remote' ? ' · add remote MCP' : ' · register local stdio')} code={selectedHost === 'arise' ? normalizedEndpoint || missingEndpoint : (transport === 'remote' ? commands?.remote : commands?.local) || missingEndpoint} disabled={!commands} />
					{#if selectedHost === 'arise'}
						<p class="field-help">In Arise MCP Hub, add an external HTTP MCP server named <code>web-memory</code> and paste this URL if your version supports remote MCP. Arise host acceptance is pending; copying the URL does not confirm a connection.</p>
					{:else}
						<p class="field-help">Run in your terminal, then open a new {hostName} session. The server name is <code>web-memory</code>. An existing entry may need removal before switching transports.</p>
					{/if}
					{#if transport === 'local'}<p class="field-help">Use the same service endpoint and WAM config directory when linking and registering. The install helper registers an absolute executable path.</p>{/if}
				</section>

				<section id="verify" class="soft-card guide-panel">
					<div class="section-label">02 / VERIFY</div>
					<h2>See the tools. Try one call.</h2>
					<p class="guide-description">Check the saved configuration, then ask your agent to call WAM in a new session.</p>
					{#if selectedHost !== 'arise'}<InstallCode label="Check saved registration · terminal" code={commands?.verify || missingEndpoint} disabled={!commands} />{:else}<p class="field-help">Open WAM in Arise MCP Hub and check its connection and discovered tools.</p>{/if}
					<InstallCode label="First request · paste into your agent" code={'Call web_memory_status, then web_memory_recall with:\n{"url":"https://example.com/guide","task_category":"compose-draft","keywords":["blog","write"],"limit":3}\nIf a hit needs details, call web_memory_expand with that hit’s memory_id and version_id, level 2. If the version has changed, recall again.'} />
					<div class="verification-note"><span aria-hidden="true">✓</span><p><strong>What a working connection looks like</strong>The agent discovers WAM tools, status reaches the API, and recall returns a response. An empty <code>hits</code> list is valid: this site may have no shared workflow yet.</p></div>
					{#if selectedHost !== 'arise'}<details class="setup-details"><summary>Help the agent remember when to use WAM</summary><p>With the CLI installed, preview the project instructions. Add the reviewed block to {selectedHost === 'claude-code' ? 'CLAUDE.md' : 'AGENTS.md'} to request recall when a site or task changes. These instructions use the CLI, so configure <code>WAM_ENDPOINT</code> for the same service.</p><InstallCode label="Preview project instructions" code={commands?.instructions || missingEndpoint} disabled={!commands} /><p>Add <code>--write</code> only when you want the CLI to update that project file.</p></details>{/if}
				</section>

				<section id="account" class="soft-card guide-panel">
					<div class="section-label">03 / OPTIONAL ACCOUNT</div>
					<h2>Ready to give a useful path back?</h2>
					<p class="guide-description">An account connects contributions, outcomes, and your private rank. Public recall works without one.</p>
					<div class="account-link"><div><strong>Your WAM web account</strong><p>{authConfig?.github_oauth_enabled ? 'Sign in through GitHub, then approve the host or device connection when prompted.' : authLoading ? 'Checking this deployment’s sign-in options…' : authConfig ? 'GitHub sign-in is disabled on this deployment. A service operator must configure it before new accounts can link.' : 'Sign-in availability could not be checked. Open the login page to retry.'}</p></div><a class="button button-light button-small" href="/login">{authConfig?.github_oauth_enabled ? 'Sign in with GitHub' : 'Open sign-in'} <span aria-hidden="true">↗</span></a></div>
					{#if selectedHost === 'arise'}
						<p class="field-help">Authorize through Arise if your version supports this server’s OAuth flow. Live Arise OAuth acceptance is pending. Arise browser access and WAM account access have separate authorization.</p>
					{:else if transport === 'remote' && oauthAvailable}
						<div class="status-line"><span class="status-dot"></span>Remote OAuth enabled for this endpoint</div>
						<p class="field-help">{selectedHost === 'codex' ? 'Run this in your terminal and approve the requested WAM permissions in the browser.' : 'Inside Claude Code, open /mcp, select web-memory, and choose Authenticate. Follow the browser consent flow.'}</p>
						<InstallCode label={selectedHost === 'codex' ? 'Authorize Codex · terminal' : 'Authorize Claude Code · inside the agent'} code={commands?.login || missingEndpoint} disabled={!commands} />
					{:else}
						{#if transport === 'remote'}<p class="field-help">{authLoading ? 'Checking this deployment’s OAuth support…' : 'OAuth has not been confirmed for this endpoint. Public remote reads remain available. For account features, choose Local stdio above, build the tools, then link your device.'}</p>{/if}
						<p class="field-help">With local tools installed, run the commands below. Approve the displayed code in your signed-in WAM browser, then check the credential and service status.</p>
						<InstallCode label="Link this device · terminal" code={commands ? commands.link + '\n' + commands.doctor : missingEndpoint} disabled={!commands} />
					{/if}
					<p class="field-help">After authorization, ask your agent for <code>web_memory_my_rank</code> or <code>web_memory_my_usage</code> to verify private access. <a href="/settings/devices">Manage connected devices ↗</a></p>
				</section>

				<details class="soft-card guide-panel advanced-panel">
					<summary>Use WAM from the CLI or HTTP API</summary>
					<p>The CLI uses the local tools built above. HTTP needs no local WAM binary. Both examples follow the endpoint selected on this page.</p>
					<div class="filter-tabs" role="group" aria-label="API example"><button class="filter-tab" class:active={example === 'cli'} type="button" aria-pressed={example === 'cli'} onclick={() => (example = 'cli')}>WAM CLI</button><button class="filter-tab" class:active={example === 'http'} type="button" aria-pressed={example === 'http'} onclick={() => (example = 'http')}>HTTP API</button></div>
					<InstallCode label={example === 'cli' ? 'First anonymous CLI recall' : 'HTTP API · anonymous recall'} code={(example === 'cli' ? commands?.recall : commands?.http) || missingEndpoint} disabled={!commands} />
					<p>For a local service, keep <code>cargo run -p wam-server</code> running in a separate terminal and use <code>http://127.0.0.1:3000/mcp</code>. Public recall needs no demo token. New account linking needs GitHub OAuth configured on the server.</p>
				</details>

				{#if selectedHost !== 'arise'}
					<details class="soft-card guide-panel advanced-panel">
						<summary>Recall from an Arise browser tab (optional local bridge)</summary>
						<p>This stdio bridge keeps Arise’s browser MCP connected and adds <code>web_memory_recall_current_tab</code>. Use the browser tools and this recall tool from the same <code>wam-browser</code> entry. The URL is read and normalized locally when your agent requests recall.</p>
						<p>Build the local tools first. Enter absolute executable paths from your own installation. On macOS, <code>command -v wam</code> shows the CLI path; on Windows, use <code>Get-Command wam</code>. Native Arise acceptance has been checked on macOS; other OS runtime acceptance is pending.</p>
						<div class="field"><label for="wam-binary">Absolute path to wam</label><input id="wam-binary" type="text" bind:value={wamBinary} placeholder={selectedPlatform === 'Windows' ? 'C:\\Users\\you\\.cargo\\bin\\wam.exe' : '/Users/you/.cargo/bin/wam'} spellcheck="false" /></div>
						<div class="field"><label for="browser-binary">Absolute path to Arise browser MCP</label><input id="browser-binary" type="text" bind:value={browserBinary} placeholder={selectedPlatform === 'macOS' ? '/Applications/Arise.app/Contents/MacOS/agent_browser' : 'Absolute executable path'} spellcheck="false" /></div>
						<InstallCode label="Register browser bridge" code={bridgeCommand || 'Enter both absolute executable paths and a valid endpoint to generate the command.'} disabled={!bridgeCommand} />
						<p>Open a new agent session and use its <code>wam-browser</code> tools to open a tab. Then call <code>web_memory_recall_current_tab</code> with a task category and allowlisted keywords. It uses only that connection’s current owned tab. Missing, expired, changed, or foreign tabs fail without choosing another tab. Your agent must honor requests to skip memory.</p>
					</details>
				{/if}

				<details class="soft-card guide-panel advanced-panel">
					<summary>Try a recall in this website</summary>
					<p>This form calls the API configured for this website. It checks public recall. Check host registration and authorization in your agent above.</p>
					<form class="recall-form" onsubmit={runRecall}>
						<div class="field"><label for="recall-url">Public HTTPS URL</label><input id="recall-url" type="url" bind:value={testUrl} autocomplete="url" /><span class="field-help">Query and fragment removed before this request: <code>{recallPreview || 'Enter a public HTTPS URL without credentials.'}</code> Use a public, non-sensitive path; the server also validates scope.</span></div>
						<div class="recall-selects"><div class="field"><label for="recall-task">Task category</label><select id="recall-task" bind:value={task}><option value="compose-draft">Draft writing</option><option value="navigate">Navigation</option><option value="research">Research</option></select></div><fieldset class="keyword-field"><legend>Keywords</legend><div class="keyword-options">{#each keywordOptions as keyword}<label><input type="checkbox" bind:group={selectedKeywords} value={keyword} />{keyword}</label>{/each}</div></fieldset></div>
						<button class="button button-dark" type="submit" disabled={!recallPreview || requestState === 'loading'}>{requestState === 'loading' ? 'Searching…' : 'Send test request'} <span aria-hidden="true">↗</span></button>
					</form>
					{#if requestState === 'error'}<div class="notice-box recall-status" role="alert"><strong>Recall could not complete.</strong>{requestMessage}</div>{/if}
					{#if requestState === 'success'}<div class="code-panel sample-response"><div class="code-panel-head"><span>LIVE API RESPONSE · THIS SITE</span></div><pre><code>{JSON.stringify(recallResult, null, 2)}</code></pre></div>{/if}
				</details>

				<details class="soft-card guide-panel advanced-panel">
					<summary>Disconnect or remove WAM</summary>
					<p>Remove the host registration when you no longer want this agent to load WAM.</p>
					{#if selectedHost !== 'arise'}<InstallCode label="Remove host registration" code={commands?.remove || missingEndpoint} disabled={!commands} />{:else}<p>Remove <code>web-memory</code> from Arise MCP Hub.</p>{/if}
					{#if selectedHost !== 'arise'}<p>If you added the optional browser bridge, remove its separate entry too.</p><InstallCode label="Remove optional browser bridge" code={selectedHost === 'codex' ? 'codex mcp remove wam-browser' : 'claude mcp remove --scope user wam-browser'} />{/if}
					<p>For a linked local device, revoke its credential with <code>{commands?.unlink || 'wam unlink'}</code>. For remote OAuth, revoke the connection in <a href="/settings/devices">Connected devices</a>. Removing a host entry alone does not revoke account access.</p>
					<p>Remove source-installed binaries separately: <code>cargo uninstall wam-cli</code> and <code>cargo uninstall wam-mcp</code>.</p>
				</details>
			</div>

			<aside class="guide-aside">
				<section class="soft-card guide-side-card setup-overview">
					<span class="section-label">WHAT YOU’RE CONNECTING</span>
					<h3>Your agent.<br />Shared experience.</h3>
					<div class="connection-map" aria-label="Your agent uses WAM for workflow recall and browser tools for actions"><div><strong>{hostName}</strong><small>Your agent</small></div><span aria-hidden="true">↓</span><div class="map-memory"><strong>WAM MCP</strong><small>Recall workflows · share with consent</small></div><span aria-hidden="true">+</span><div><strong>Your browser tools</strong><small>Navigate · edit · save</small></div></div>
					<p>Keep your browser MCP for page actions. Add WAM for workflow hints your agent can consult along the way.</p>
					<a class="resource-link" href="/memories">Inspect shared workflows <span aria-hidden="true">↗</span></a>
				</section>
				<section class="soft-card guide-side-card">
					<span class="section-label">AVAILABLE TODAY</span>
					<h3>10 tools. One shared service.</h3>
					<dl class="tool-groups"><div><dt>Public</dt><dd>Status, recall, expand, leaderboard</dd></div><div><dt>Linked account</dt><dd>My rank, my usage, propose, my contributions, withdraw, outcome</dd></div></dl>
					<p>Remote HTTP and local stdio use this catalog. Contributions need explicit public-sharing consent. Only versions you actually used can earn outcome points.</p>
				</section>
				<section class="soft-card guide-side-card support-status">
					<span class="section-label">IMPLEMENTATION & LIMITS</span>
					<h3>What has been checked?</h3>
					<ul><li><span class="pill pill-green">Implemented</span><p>HTTP + stdio MCP, pinned-version expand, auth/consent status, GET/POST SSE, device linking and OAuth lifecycle.</p></li><li><span class="pill">Locally verified</span><p>macOS credentials, Codex/Claude registration, synthetic GitHub sign-in, native Codex OAuth, and Arise leased-tab recall through the local bridge.</p></li><li><span class="pill pill-coral">Acceptance pending</span><p>Live GitHub accounts, Claude/Arise remote OAuth, Windows/Linux runtime, and installation from a real signed release.</p></li><li><span class="pill">Boundaries</span><p>The local bridge reads a tab when the agent calls its recall tool. Background injection, server-pushed workflow updates, and SSE replay are not provided.</p></li></ul>
				</section>
				<section class="soft-card guide-side-card troubleshooting">
					<span class="section-label">NEED A HAND?</span>
					<h3>Start with the failing step.</h3>
					<details><summary>Command not found</summary><p>Install the selected host’s CLI. For local stdio, build both WAM tools and add Cargo’s bin directory to PATH.</p></details>
					<details><summary>Registered, but no tools</summary><p>Start a new agent session and check the endpoint. A saved entry confirms registration; a successful tools/list confirms discovery.</p></details>
					<details><summary>401 or 403 on private tools</summary><p>Authorize this host or link the local device and approve the required scopes. Website sign-in alone does not authorize an MCP host.</p></details>
					<details><summary>429 or service unavailable</summary><p>Respect Retry-After before retrying. Check the address and TLS. For local development, keep wam-server running.</p></details>
					<div class="official-links"><a href="https://developers.openai.com/codex/mcp" target="_blank" rel="noreferrer">Codex MCP docs ↗</a><a href="https://code.claude.com/docs/en/mcp" target="_blank" rel="noreferrer">Claude Code MCP docs ↗</a></div>
				</section>
			</aside>
		</div>
	</div>
</section>

<style>
	.setup-page { --setup-muted: #526754; padding-top: 54px; }
	.setup-page .page-heading { margin-bottom: 30px; }
	.setup-page .page-heading h1 { font-size: clamp(36px, 4.1vw, 54px); }
	.setup-path { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); margin-bottom: 28px; padding: 8px; border: 1px solid #dce5d8; border-radius: 13px; background: #edf2e6; }
	.setup-path a { display: flex; align-items: center; gap: 13px; padding: 20px 17px; }
	.setup-path a + a { border-left: 1px solid #d7e0d1; }
	.setup-path a > span { color: #526744; font: 12px 'DM Mono', monospace; }
	.setup-path strong, .setup-path small { display: block; }
	.setup-path strong { color: #294d3b; font-size: 13px; }
	.setup-path small { margin-top: 4px; color: #52674f; font-size: 11px; }
	.setup-path i { margin-left: auto; color: #82966e; font-style: normal; }
	.guide-main, .guide-aside { min-width: 0; }
	.guide-panel { scroll-margin-top: 105px; padding: 30px; }
	.guide-panel h2 { font-size: 25px; }
	.guide-panel :global(.code-panel) { margin-top: 14px; }
	.guide-panel :global(.code-panel pre) { font-size: 11px; line-height: 1.85; overflow-wrap: anywhere; }
	.guide-panel :global(.code-panel-head) { font-size: 10px; }
	.guide-panel :global(.copy-button) { min-height: 30px; padding: 4px 8px; }
	.guide-description { color: var(--setup-muted); font-size: 13px; line-height: 1.8; }
	.guide-panel .field-help { font-size: 12px; line-height: 1.8; color: var(--setup-muted); }
	.field-help a, .setup-details a { text-decoration: underline; text-underline-offset: 3px; }
	.guide-panel code { overflow-wrap: anywhere; }
	.host-picker { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 9px; margin: 23px 0 20px; }
	.host-picker button { display: flex; flex-wrap: wrap; align-items: center; gap: 9px; min-height: 67px; padding: 13px; border: 1px solid #dce4d8; border-radius: 9px; color: #59715d; background: #fffefa; font: 600 12px 'DM Sans', sans-serif; text-align: left; cursor: pointer; }
	.host-picker button.chosen { border-color: #688354; color: #254b3c; background: #f0f5e7; box-shadow: inset 0 0 0 1px #688354; }
	.host-mark { display: grid; place-items: center; width: 29px; height: 29px; border-radius: 8px; color: #405a46; background: #e9eee2; font: 600 12px Manrope, sans-serif; }
	.host-check { margin-left: auto; color: #55773d; }
	.transport-picker { margin-bottom: 6px; }
	.transport-picker button { min-height: 40px; }
	.recommended { margin-left: 7px; opacity: .8; font-size: 9px; }
	.endpoint-field { margin-top: 19px; }
	.endpoint-field input { font: 12px 'DM Mono', monospace; }
	.endpoint-field input[aria-invalid='true'] { border-color: #aa5a43; }
	.shell-picker { display: flex; align-items: center; flex-wrap: wrap; gap: 7px; margin: 19px 0 12px; font-size: 11px; color: var(--setup-muted); }
	.shell-picker > span { margin-right: 6px; }
	.shell-picker button { padding: 7px 10px; border: 1px solid transparent; border-radius: 5px; color: #53684f; background: transparent; font: inherit; cursor: pointer; }
	.shell-picker button.chosen { border-color: #dce5d7; color: #254b3c; background: #eef3e7; }
	.shell-picker small { font-size: 10px; }
	.verification-note { display: flex; align-items: flex-start; gap: 12px; margin-top: 21px; padding: 17px; border-radius: 8px; background: #eef4e7; }
	.verification-note > span { color: #6f8d43; }
	.verification-note p { margin: 0; color: #5c745f; font-size: 12px; line-height: 1.8; }
	.verification-note strong { display: block; margin-bottom: 5px; color: #34543f; }
	.setup-details { margin-top: 20px; padding-top: 17px; border-top: 1px solid #e1e8dc; }
	.setup-details summary, .advanced-panel > summary { color: #3b5943; font-size: 13px; font-weight: 600; cursor: pointer; }
	.setup-details > p, .advanced-panel > p { color: var(--setup-muted); font-size: 12px; line-height: 1.8; }
	.account-link { display: flex; align-items: center; gap: 18px; margin: 20px 0; padding-bottom: 19px; border-bottom: 1px solid #e5eade; }
	.account-link strong { color: #38513f; font-size: 13px; }
	.account-link p { margin: 6px 0 0; color: var(--setup-muted); font-size: 12px; line-height: 1.8; }
	.account-link .button { flex-shrink: 0; }
	.status-line { display: flex; align-items: center; gap: 8px; color: #486b3b; font-size: 12px; }
	.status-dot { width: 7px; height: 7px; border-radius: 50%; background: #739350; }
	.advanced-panel { padding-block: 23px; }
	.guide-side-card h3 { font-size: 22px; }
	.guide-side-card > p { color: var(--setup-muted); font-size: 12px; line-height: 1.8; }
	.setup-overview { background: #edf2e5; }
	.connection-map { display: grid; justify-items: center; gap: 7px; margin-block: 20px; }
	.connection-map > div { width: 100%; padding: 14px; border: 1px solid #d6e0ce; border-radius: 7px; background: #fafcf6; text-align: center; }
	.connection-map strong, .connection-map small { display: block; }
	.connection-map strong { color: #3d5941; font-size: 12px; }
	.connection-map small { margin-top: 4px; color: #526548; font-size: 10px; }
	.connection-map > span { color: #9baa88; font-size: 15px; }
	.connection-map .map-memory { border-color: #254b3c; background: #254b3c; }
	.map-memory strong { color: #eff4df; }
	.map-memory small { color: #bdd09e; }
	.resource-link { display: flex; justify-content: space-between; gap: 10px; margin-top: 20px; padding-top: 17px; border-top: 1px solid #d7e1cf; color: #3e5b3e; font-size: 12px; font-weight: 600; }
	.tool-groups { margin: 0; }
	.tool-groups > div { padding-block: 12px; border-top: 1px solid #e6eadd; }
	.tool-groups dt { color: #436047; font-size: 12px; font-weight: 600; }
	.tool-groups dd { margin: 6px 0 0; color: #52664c; font-size: 12px; line-height: 1.8; }
	.support-status ul { padding: 0; margin: 0; list-style: none; }
	.support-status li + li { padding-top: 14px; border-top: 1px solid #e8ecdf; }
	.support-status li p { margin: 9px 0 14px; color: var(--setup-muted); font-size: 12px; line-height: 1.8; }
	.troubleshooting details { padding: 13px 0; border-top: 1px solid #e6eadd; }
	.troubleshooting summary { color: #48604b; font-size: 12px; cursor: pointer; }
	.troubleshooting details p { margin: 10px 0 0; color: var(--setup-muted); font-size: 12px; line-height: 1.8; }
	.official-links { display: grid; gap: 10px; margin-top: 17px; font-size: 11px; color: #5e7454; }
	@media (max-width: 1100px) { .host-picker { grid-template-columns: 1fr; } .host-picker button { min-height: 49px; } .recommended { display: none; } }
	@media (max-width: 900px) { .host-picker { grid-template-columns: repeat(3, minmax(0, 1fr)); } .guide-aside { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 15px; } .setup-path a { padding: 15px 10px; gap: 9px; } .setup-path i { display: none; } }
	@media (max-width: 600px) { .setup-path { grid-template-columns: 1fr; } .setup-path a + a { border-left: 0; border-top: 1px solid #d7e0d1; } .setup-path a { padding: 13px; } .setup-path i { display: block; } .guide-panel { padding: 22px 18px; } .guide-panel h2 { font-size: 23px; } .host-picker { grid-template-columns: 1fr; } .guide-aside { grid-template-columns: 1fr; } .account-link { flex-direction: column; align-items: flex-start; } .guide-panel :global(.code-panel pre) { font-size: 10px; } }
</style>
