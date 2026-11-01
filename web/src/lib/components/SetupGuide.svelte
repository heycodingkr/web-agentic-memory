<script lang="ts">
	import { onMount } from 'svelte';
	import { api, ApiError, type AuthConfig } from '#lib/api';
	import { browserBridgeInstallCommand, claudeOnMuseEnv, defaultMcpEndpoint, firstRecall, firstRequest, mcpRecallInstructions, museInstallCommand, museSettingsPath, normalizeMcpEndpoint, normalizeSourceRepository, setupCommands, sourceInstallCommands } from '#lib/install';
	import InstallCode from '#lib/components/InstallCode.svelte';
	import { githubRepositoryUrl, publicMcpEndpoint, publicSiteOrigin } from '#lib/site.js';

	let { korean = false }: { korean?: boolean } = $props();
	const t = (english: string, translated: string) => korean ? translated : english;
	const siteOrigin = String(import.meta.env.VITE_SITE_URL || publicSiteOrigin).replace(/\/$/, '');
	const pageTitle = t('Install & connect — Web Agentic Memory', '설치 및 연결 — Web Agentic Memory');
	const pageDescription = t(
		'Connect Muse Code, Codex, Claude Code, or Arise to WAM. Choose remote MCP or local stdio, verify your tools, and link your account when you want to contribute.',
		'Muse Code·Codex·Claude Code를 WAM에 연결하고 첫 힌트를 받아보세요. 원격 MCP, 로컬 CLI, 연결 확인과 선택형 계정 연결을 안내합니다.'
	);
	const pagePath = korean ? '/ko/guide' : '/guide';
	const ogImage = `${siteOrigin}${korean ? '/og-image-ko.png' : '/og-image.png'}`;

	const hosts = [
		{ id: 'muse', name: 'Muse Code', mark: 'M' },
		{ id: 'codex', name: 'Codex', mark: 'C' },
		{ id: 'claude-code', name: 'Claude Code', mark: 'Cl' },
		{ id: 'arise', name: 'Arise MCP Hub', mark: 'A' }
	] as const;
	const platforms = ['macOS', 'Linux', 'Windows'];
	const sourceRepository = normalizeSourceRepository(import.meta.env.VITE_WAM_SOURCE_URL || githubRepositoryUrl);
	const keywordOptions = ['document', 'blog', 'draft', 'publish', 'search', 'write'];
	let selectedHost = $state<'muse' | 'codex' | 'claude-code' | 'arise'>('muse');
	let transport = $state<'remote' | 'local'>('remote');
	let selectedPlatform = $state('macOS');
	let mcpEndpoint = $state(import.meta.env.VITE_WAM_MCP_ENDPOINT || '');
	let authConfig = $state<AuthConfig | null>(null);
	let authLoading = $state(true);
	let ready = $state(false);
	let example = $state<'cli' | 'http'>('cli');
	let wamBinary = $state('');
	let browserBinary = $state('');
	let selectedKeywords = $state<string[]>([...firstRecall.keywords]);
	let task = $state(firstRecall.task_category);
	let testUrl = $state(firstRecall.url);
	let requestState = $state<'idle' | 'loading' | 'success' | 'error'>('idle');
	let requestMessage = $state('');
	type RecallResult = { hits: Array<{ memory_id: string; version_id: string; hint: string }> };
	let recallResult = $state<RecallResult | null>(null);

	const normalizedEndpoint = $derived(normalizeMcpEndpoint(mcpEndpoint));
	const commands = $derived(setupCommands(selectedHost === 'arise' ? 'codex' : selectedHost, normalizedEndpoint, selectedPlatform));
	const hostName = $derived(hosts.find(host => host.id === selectedHost)!.name);
	const oauthAvailable = $derived(!!normalizedEndpoint && authConfig?.mcp_oauth_enabled === true && authConfig.mcp_resource === normalizedEndpoint);
	const missingEndpoint = $derived(t('Enter a valid MCP endpoint above to generate this command.', '위에 올바른 MCP 주소를 입력하면 명령이 생성됩니다.'));
	const hostProgram = $derived(selectedHost === 'claude-code' ? 'claude' : selectedHost === 'muse' ? 'muse' : 'codex');
	const hostInstallUrl = $derived(selectedHost === 'claude-code' ? 'https://code.claude.com/docs/en/quickstart' : selectedHost === 'muse' ? 'https://dev.meta.ai/docs/muse-code' : 'https://developers.openai.com/codex/cli');
	const loginLabel = $derived(selectedHost === 'codex' ? t('Authorize Codex · terminal', 'Codex 인증 · 터미널') : selectedHost === 'muse' ? t('Authorize Muse Code · terminal', 'Muse Code 인증 · 터미널') : t('Authorize Claude Code · inside the agent', 'Claude Code 인증 · 에이전트 안에서'));
	const loginHelp = $derived(selectedHost === 'codex' ? t("Run this in your terminal and approve the requested WAM permissions in the browser.", "터미널에서 실행한 뒤 브라우저에서 요청된 WAM 권한을 승인하세요.") : selectedHost === 'muse' ? t("Run this in your terminal and approve the requested WAM permissions in the browser. Signing in to WAM from Muse Code has not been verified end to end yet.", "터미널에서 실행한 뒤 브라우저에서 요청된 WAM 권한을 승인하세요. Muse Code에서의 WAM 로그인은 아직 끝까지 검증하지 못했습니다.") : t("Inside Claude Code, open /mcp, select web-memory, and choose Authenticate. Follow the browser consent flow.", "Claude Code에서 /mcp를 열고 web-memory를 선택한 뒤 Authenticate를 누르세요. 이어서 브라우저에서 승인하세요."));
	// The Muse panel shows Claude Code running on Muse Spark, so it needs Claude's registration command too.
	const claudeOnMuse = $derived(setupCommands('claude-code', normalizedEndpoint, selectedPlatform));
	const museEnv = $derived(claudeOnMuseEnv(selectedPlatform));
	const bridgeCommand = $derived(browserBridgeInstallCommand(selectedHost === 'claude-code' ? 'claude-code' : 'codex', normalizedEndpoint, wamBinary, browserBinary || (selectedPlatform === 'macOS' ? '/Applications/Arise.app/Contents/MacOS/agent_browser' : ''), selectedPlatform));
	const recallPreview = $derived.by(() => {
		try {
			const url = new URL(testUrl);
			if (url.protocol !== 'https:' || url.username || url.password) return '';
			return url.origin + url.pathname;
		} catch { return ''; }
	});

	onMount(() => {
		ready = true;
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
			recallResult = await api<RecallResult>('/v1/memories/recall', {
				method: 'POST', credentials: 'omit',
				body: JSON.stringify({ url: recallPreview, task_category: task, keywords: selectedKeywords, limit: 3 })
			});
			requestState = 'success';
		} catch (error) {
			requestState = 'error';
			requestMessage = error instanceof ApiError ? error.message : t('The API could not be reached. Check the service address and connection, then try again.', 'API에 연결할 수 없습니다. 서비스 주소와 연결 상태를 확인한 뒤 다시 시도하세요.');
		}
	}
</script>

<svelte:head>
	<title>{pageTitle}</title>
	<meta name="description" content={pageDescription} />
	<meta property="og:type" content="website" />
	<meta property="og:site_name" content="Web Agentic Memory" />
	<meta property="og:locale" content={korean ? 'ko_KR' : 'en_US'} />
	<meta property="og:url" content={`${siteOrigin}${pagePath}`} />
	<meta property="og:title" content={pageTitle} />
	<meta property="og:description" content={pageDescription} />
	<meta property="og:image" content={ogImage} />
	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:title" content={pageTitle} />
	<meta name="twitter:description" content={pageDescription} />
	<meta name="twitter:image" content={ogImage} />
	<link rel="canonical" href={`${siteOrigin}${pagePath}`} />
	<link rel="alternate" hreflang="en" href={`${siteOrigin}/guide`} />
	<link rel="alternate" hreflang="ko" href={`${siteOrigin}/ko/guide`} />
	<link rel="alternate" hreflang="x-default" href={`${siteOrigin}/guide`} />
	<link rel="alternate" type="text/markdown" href={`${siteOrigin}/llms.txt`} />
</svelte:head>

<section class="page-shell setup-page" lang={korean ? 'ko' : 'en'}>
	<div class="container">
		<div class="page-heading">
			<nav class="guide-languages" aria-label={t("Guide language", "안내 언어")}><a href="/guide" lang="en" aria-current={!korean ? "page" : undefined}>English</a><span aria-hidden="true"> · </span><a href="/ko/guide" lang="ko" aria-current={korean ? "page" : undefined}>한국어</a></nav>
			<span class="eyebrow">{t("Agent setup", "에이전트 연결")}</span>
			<h1>{t("Connect your agent.", "에이전트를 연결하고,")}<br />{t("Start a step ahead.", "첫 힌트를 받아보세요.")}</h1>
			<p>{t("Connect once to recall shared website workflows in the tools you already use. Start with public reads. Link an account when you want to contribute.", "쓰던 에이전트에 한 번 연결하면 사이트와 작업에 맞는 힌트를 조회할 수 있습니다. 공개 조회는 계정 없이, 기여는 원할 때 계정을 연결하세요.")}</p>
			<div class="demo-note">{t("Public reads · no account required", "공개 조회 · WAM 계정 없이 시작")}</div>
			<a class="text-link" href="#try">{t("Try a public hint before installing", "설치 전에 힌트 먼저 보기")} →</a>
		</div>

		<nav class="setup-path" aria-label={t("Setup steps", "연결 단계")}>
			<a href="#connect"><span>01</span><div><strong>{t("Connect your agent", "에이전트 연결")}</strong><small>{t("Choose a host and transport", "호스트와 연결 방식 선택")}</small></div><i aria-hidden="true">↗</i></a>
			<a href="#verify"><span>02</span><div><strong>{t("Check the connection", "연결 확인")}</strong><small>{t("Discover tools, then try a recall", "도구 확인 후 첫 조회")}</small></div><i aria-hidden="true">↗</i></a>
			<a href="#account"><span>03</span><div><strong>{t("Make it yours", "계정 연결")}</strong><small>{t("Optional account linking", "기여할 때만 선택")}</small></div><i aria-hidden="true">↗</i></a>
		</nav>

		<div class="guide-grid">
			<div class="guide-main">
				<section id="browser-tools" class="soft-card guide-panel">
					<div class="section-label">{t('BEFORE YOU CONNECT', '연결 전 준비')}</div>
					<h2>{t('Prepare a browser for your agent.', '에이전트가 쓸 브라우저를 준비하세요.')}</h2>
					<p class="guide-description">{t('Download a browser for opening tabs, clicking and typing. WAM supplies the workflow hints your agent reads along the way.', '탭 열기·클릭·입력에 쓸 브라우저를 내려받으세요. WAM은 에이전트가 작업 중 참고할 힌트를 제공합니다.')}</p>
					<div class="guide-actions">
						<a class="button button-dark button-small" href="https://arisebrowser.com" target="_blank" rel="noopener noreferrer">{t('Download Arise Browser', 'Arise Browser 다운로드')} <span aria-hidden="true">↗</span></a>
						<a class="button button-light button-small" href="https://www.google.com/chrome/" target="_blank" rel="noopener noreferrer">{t('Download Google Chrome', 'Google Chrome 다운로드')} <span aria-hidden="true">↗</span></a>
					</div>
					<p>{t('In Arise, use its built-in agent and MCP Hub, or connect its browser MCP to your coding agent. For Chrome, connect a browser MCP to Muse Code, Claude Code or Codex. Keep your browser open, then add WAM below.', 'Arise에서는 내장 에이전트와 MCP Hub를 쓰거나 브라우저 MCP를 코딩 에이전트에 연결하세요. Chrome은 브라우저 MCP를 Muse Code·Claude Code·Codex에 연결해 사용합니다. 브라우저를 열어 둔 뒤 아래에서 WAM을 추가하세요.')}</p>
				</section>
				<section id="connect" class="soft-card guide-panel">
					<div class="section-label">{t("01 / CONNECT", "01 / 연결")}</div>
					<h2>{t("Where does your agent run?", "어떤 에이전트를 쓰시나요?")}</h2>
					<p class="guide-description">{t("Use your existing agent. Remote MCP needs no WAM download or browser extension; use your host's browser tools for page actions.", "이미 쓰는 에이전트를 선택하세요. 원격 MCP에는 WAM 다운로드나 브라우저 확장 프로그램이 필요 없으며, 페이지 조작에는 에이전트의 브라우저 도구를 그대로 사용합니다.")}</p>
					<div class="host-picker" role="group" aria-label={t("Agent host", "에이전트 호스트")}>
						{#each hosts as host}
							<button type="button" disabled={!ready} class:chosen={selectedHost === host.id} aria-pressed={selectedHost === host.id} onclick={() => { selectedHost = host.id; if (host.id === 'arise' || host.id === 'muse') transport = 'remote'; }}><span class="host-mark" aria-hidden="true">{host.mark}</span>{host.name}<span class="host-check" aria-hidden="true">{selectedHost === host.id ? '✓' : ''}</span></button>
						{/each}
					</div>
					<div class="filter-tabs transport-picker" role="group" aria-label={t("Connection type", "연결 방식")}>
						<button class="filter-tab" class:active={transport === 'remote'} type="button" aria-pressed={transport === 'remote'} disabled={!ready} onclick={() => (transport = 'remote')}>{t("Remote MCP", "원격 MCP")} <span class="recommended">{t("Recommended", "추천")}</span></button>
						<button class="filter-tab" class:active={transport === 'local'} type="button" aria-pressed={transport === 'local'} disabled={!ready || selectedHost === 'arise' || selectedHost === 'muse'} onclick={() => (transport = 'local')}>{t("Local stdio", "로컬 stdio")}</button>
					</div>
					{#if selectedHost !== 'arise'}
						<div class="host-prerequisite">
							<p>{t('Already use this CLI? Check its version and continue. Otherwise, install it and sign in to the host first.', '이미 쓰는 CLI라면 버전을 확인하고 계속하세요. 처음이라면 먼저 설치한 뒤 해당 에이전트에 로그인하세요.')}</p>
							<a class="text-link" href={hostInstallUrl} target="_blank" rel="noreferrer">{selectedHost === 'claude-code' ? t('Install Claude Code', 'Claude Code 설치') : selectedHost === 'muse' ? t('Install Muse Code', 'Muse Code 설치') : t('Install Codex CLI', 'Codex CLI 설치')} <span aria-hidden="true">↗</span></a>
							{#if selectedHost === 'muse'}<InstallCode label={t('Install Muse Code · terminal', 'Muse Code 설치 · 터미널')} code={museInstallCommand(selectedPlatform)} {korean} />{/if}
							<InstallCode label={t('Check host CLI · terminal', '호스트 CLI 확인 · 터미널')} code={hostProgram + ' --version'} {korean} />
						</div>
					{/if}
					<p class="field-help">{transport === 'remote' ? (selectedHost === 'muse' ? t("Add the block below to Muse Code's settings file. Remote MCP needs no WAM download, browser extension, Rust, or WAM account.", "아래 블록을 Muse Code 설정 파일에 추가하세요. 원격 MCP에는 WAM 다운로드·브라우저 확장 프로그램·Rust·WAM 계정이 필요 없습니다.") : t("Run the command below in your terminal. Remote MCP needs no WAM download, browser extension, Rust, or WAM account.", "아래 명령을 터미널에서 실행하세요. 원격 MCP에는 WAM 다운로드·브라우저 확장 프로그램·Rust·WAM 계정이 필요 없습니다.")) : t("Build both local tools below first. A local wam-mcp process calls the WAM service and can use your linked device credential.", "아래 로컬 도구 두 개를 먼저 빌드하세요. 로컬 wam-mcp가 WAM 서버를 호출하며 연결한 기기의 자격증명을 사용할 수 있습니다.")}</p>
					<div class="field endpoint-field">
						<label for="mcp-endpoint">{t("WAM MCP endpoint", "WAM MCP 주소")}</label>
						<input id="mcp-endpoint" type="url" bind:value={mcpEndpoint} autocomplete="url" spellcheck="false" aria-invalid={mcpEndpoint !== '' && !normalizedEndpoint} aria-describedby="endpoint-help" />
						<span id="endpoint-help" class="field-help">{mcpEndpoint && !normalizedEndpoint ? t("Use an HTTPS URL ending in /mcp, without credentials, query, or fragment. HTTP is allowed for localhost.", "인증 정보·쿼리·fragment 없이 /mcp로 끝나는 HTTPS 주소를 사용하세요. localhost는 HTTP도 허용합니다.") : t(`Public service: ${publicMcpEndpoint}. For a self-hosted service, enter its /mcp URL.`, `공개 서비스: ${publicMcpEndpoint}. 직접 운영하는 서버는 해당 /mcp 주소를 입력하세요.`)}</span>
					</div>
					<div class="shell-picker" role="group" aria-label={t("Operating system", "운영체제")}>
						<span>{t("Terminal", "터미널")}</span>{#each platforms as platform}<button type="button" class:chosen={selectedPlatform === platform} disabled={!ready} aria-pressed={selectedPlatform === platform} onclick={() => (selectedPlatform = platform)}>{platform}</button>{/each}
						{#if selectedPlatform === 'Windows'}<small>PowerShell</small>{/if}
					</div>
					<details class="setup-details source-tools" open={transport === 'local'}>
						<summary>{t("Install local tools from source (CLI or stdio)", "소스에서 로컬 도구 설치 (CLI 또는 stdio)")}</summary>
						<p><a href="https://rustup.rs" target="_blank" rel="noreferrer">{t("Install Rust and Cargo ↗", "Rust와 Cargo 설치 ↗")}</a></p>
						{#if sourceRepository}
							<p><a href={sourceRepository} target="_blank" rel="noreferrer">{t("Get the source repository ↗", "소스 저장소 열기 ↗")}</a>{t(", then open its root directory in your terminal.", "를 받아 터미널에서 루트 폴더를 여세요.")}</p>
						{:else}
							<p class="field-help">{t("Use the source checkout provided by your service operator. Open its root directory in your terminal. If you do not have a checkout, use Remote MCP above; it needs no source download.", "운영자가 제공한 소스 체크아웃의 루트 폴더를 터미널에서 여세요. 체크아웃이 없다면 위의 원격 MCP로 연결하세요. 소스 다운로드 없이 사용할 수 있습니다.")}</p>
						{/if}
						<InstallCode {korean} label={t("Build local tools", "로컬 도구 빌드")} code={sourceInstallCommands} />
						<p>{t("Cargo installs", "Cargo는")} <code>wam</code> {t("and", "및")} <code>wam-mcp</code> {t("into its bin directory. Add that directory to PATH, then reopen your terminal. A public signed release is not available from this guide.", "를 bin 폴더에 설치합니다. 해당 폴더를 PATH에 추가하고 터미널을 다시 여세요. 이 가이드에서는 공개 서명 릴리스를 제공하지 않습니다.")}</p>
					</details>
					<InstallCode {korean} label={selectedHost === 'arise' ? t("Arise MCP Hub · server URL", "Arise MCP Hub · 서버 URL") : selectedHost === 'muse' ? t(`Muse Code · add to ${museSettingsPath}`, `Muse Code · ${museSettingsPath}에 추가`) : hostName + (transport === 'remote' ? t(" · add remote MCP", " · 원격 MCP 추가") : t(" · register local stdio", " · 로컬 stdio 등록"))} code={selectedHost === 'arise' ? normalizedEndpoint || missingEndpoint : (transport === 'remote' ? commands?.remote : commands?.local) || missingEndpoint} disabled={!commands} />
					{#if selectedHost === 'arise'}
						<p class="field-help">{t("In Arise MCP Hub, add an external HTTP MCP server named", "Arise MCP Hub에서 외부 HTTP MCP 서버 이름을")} <code>web-memory</code> {t("and paste this URL if your version supports remote MCP. Arise host acceptance is pending; copying the URL does not confirm a connection.", "로 지정하고 원격 MCP를 지원하는 버전에서 이 URL을 붙여 넣으세요. Arise Hub 수용 검증은 남아 있으며, URL 복사만으로 연결이 확인되지는 않습니다.")}</p>
					{:else if selectedHost === 'muse'}
						<p class="field-help">{t("Merge this into the file and keep your other settings. The file must keep schema_version. Then start a new muse session and run /mcp to see web-memory and its tools. If WAM is already in Claude Code or Codex, Meta's docs say /migrate inside Muse Code can import those MCP servers.", "기존 설정은 그대로 두고 이 내용을 합치세요. 파일에 schema_version이 있어야 합니다. 새 muse 세션을 열고 /mcp로 web-memory와 도구가 보이는지 확인하세요. Claude Code나 Codex에 이미 WAM을 추가했다면 Meta 문서상 Muse Code의 /migrate로 그 MCP 서버를 가져올 수 있습니다.")}</p>
					{:else}
						<p class="field-help">{t("Run in your terminal, then open a new", "터미널에서 실행한 뒤 새")} {hostName} {t("session. The server name is", "세션을 여세요. 서버 이름은")} <code>web-memory</code>{t(". An existing entry may need removal before switching transports.", "입니다. 연결 방식을 바꾸려면 기존 등록을 먼저 제거해야 할 수 있습니다.")}</p>
					{/if}
					{#if transport === 'local'}<p class="field-help">{t("Use the same service endpoint and WAM config directory when linking and registering. The install helper registers an absolute executable path.", "계정 연결과 호스트 등록 시 같은 서비스 주소와 WAM 설정 폴더를 사용하세요. 설치 도우미는 실행 파일의 절대 경로를 등록합니다.")}</p>{/if}
				</section>

				<section id="verify" class="soft-card guide-panel">
					<div class="section-label">{t("02 / VERIFY", "02 / 확인")}</div>
					<h2>{t("See the tools. Try one call.", "도구를 확인하고, 첫 힌트를 받아보세요.")}</h2>
					<p class="guide-description">{t("Check the saved configuration, then ask your agent to call WAM in a new session.", "저장된 설정을 확인한 뒤 새 에이전트 세션에서 아래 첫 요청을 붙여 넣으세요.")}</p>
					{#if selectedHost !== 'arise'}<InstallCode {korean} label={selectedHost === 'muse' ? t("Check connection · terminal (one short model call)", "연결 확인 · 터미널 (짧은 모델 호출 1회)") : t("Check saved registration · terminal", "저장된 등록 확인 · 터미널")} code={commands?.verify || missingEndpoint} disabled={!commands} />{:else}<p class="field-help">{t("Open WAM in Arise MCP Hub and check its connection and discovered tools.", "Arise MCP Hub에서 WAM을 열어 연결 상태와 발견된 도구를 확인하세요.")}</p>{/if}
					{#if selectedHost === 'muse'}<p class="field-help">{t("Or start muse and run /mcp: web-memory should be listed with its ten tools.", "또는 muse를 실행하고 /mcp를 입력하세요. web-memory와 도구 10개가 보여야 합니다.")}</p>{/if}
					<InstallCode {korean} label={t("First request · paste into your agent", "첫 요청 · 에이전트에 붙여 넣기")} code={korean ? ['web_memory_status를 호출한 뒤, web_memory_recall을 다음 인수로만, 다른 필드 없이 호출해 주세요:', JSON.stringify(firstRecall), '받은 힌트를 보여 주세요. Example:로 시작하는 항목은 시작용 예시이며 검증된 사이트 지식이 아닙니다.', '상세가 필요하면 받은 memory_id와 version_id로 web_memory_expand(level 2)를 호출하세요. 버전을 사용할 수 없으면 다시 조회하세요.', '읽기 전용 연결 시험입니다. 문서를 열거나 편집하지 마세요.'].join('\n') : firstRequest} />
					<div class="verification-note"><span aria-hidden="true">✓</span><p><strong>{t("What a working connection looks like", "연결이 성공하면")}</strong>{t("The agent discovers WAM tools, status reaches the API, and recall returns a response. The sample uses a public Google Docs route without a document ID. Built-in entries start with", "에이전트에 WAM 도구가 나타나고 status와 recall이 응답합니다. 첫 요청은 문서 ID가 없는 공개 Google Docs 경로를 사용합니다. 이름이")} <code>Example:</code> {t("and are starter examples. An empty", "로 시작하는 항목은 시작용 예시입니다.")} <code>hits</code> {t("list is valid if this deployment has no examples or matching workflows.", "가 비어 있어도 응답을 받았다면 호출은 성공입니다. 이 서버에 예시나 일치하는 워크플로가 없을 수 있습니다.")}</p></div>
					{#if selectedHost !== 'arise'}<details class="setup-details"><summary>{t("Help the agent remember when to use WAM", "앞으로도 WAM을 사용하도록 지시하기")}</summary><p>{t("Copy this block into your project’s", "아래 내용을 프로젝트의")} <code>{selectedHost === 'claude-code' ? 'CLAUDE.md' : 'AGENTS.md'}</code>{t(". It uses the connected MCP tools and needs no local WAM CLI. Review it before saving, then start a new agent session.", "에 검토 후 추가하고 새 에이전트 세션을 여세요. 연결한 MCP 도구를 사용하므로 WAM CLI를 설치할 필요가 없습니다.")}</p><InstallCode label={t("Project instructions · MCP", "프로젝트 지시문 · MCP")} code={mcpRecallInstructions} {korean} /></details>{/if}
				</section>

				<details id="try" class="soft-card guide-panel advanced-panel" open>
					<summary>{t("Try a recall in this website", "웹에서 먼저 힌트 조회하기")}</summary>
					<p>{t("This form calls the API configured for this website. It checks public recall. Check host registration and authorization in your agent above.", "설치 없이 이 웹사이트의 공개 API를 조회합니다. 이 결과는 API 동작을 확인하며, 에이전트의 MCP 등록·인증 여부는 위 단계에서 따로 확인하세요.")}</p>
					<form class="recall-form" onsubmit={runRecall}>
						<div class="field"><label for="recall-url">{t("Public HTTPS URL", "공개 HTTPS URL")}</label><input id="recall-url" type="url" bind:value={testUrl} autocomplete="url" /><span class="field-help">{t("Query and fragment removed before this request:", "요청 전에 쿼리와 fragment를 제거합니다:")} <code>{recallPreview || t("Enter a public HTTPS URL without credentials.", "인증 정보가 없는 공개 HTTPS URL을 입력하세요.")}</code> {t("Use a public, non-sensitive path; the server also validates scope.", "민감 정보가 없는 공개 경로를 사용하세요. 서버도 범위를 검증합니다.")}</span></div>
						<div class="recall-selects"><div class="field"><label for="recall-task">{t("Task category", "작업 유형")}</label><select id="recall-task" bind:value={task}><option value="compose-draft">{t("Draft writing", "초안 작성")}</option><option value="navigate">{t("Navigation", "탐색")}</option><option value="research">{t("Research", "조사")}</option></select></div><fieldset class="keyword-field"><legend>{t("Keywords", "키워드")}</legend><div class="keyword-options">{#each keywordOptions as keyword}<label><input type="checkbox" bind:group={selectedKeywords} value={keyword} />{keyword}</label>{/each}</div></fieldset></div>
						<button class="button button-dark" type="submit" disabled={!ready || !recallPreview || requestState === 'loading'}>{requestState === 'loading' ? t("Searching…", "조회 중…") : t("Send test request", "힌트 조회하기")} <span aria-hidden="true">↗</span></button>
					</form>
					{#if requestState === 'error'}<div class="notice-box recall-status" role="alert"><strong>{t("Recall could not complete.", "힌트를 조회하지 못했습니다.")}</strong>{requestMessage}</div>{/if}
					{#if requestState === 'success' && recallResult}
						<div class="recall-status" role="status">{recallResult.hits.length ? t('The API returned hints. Your host connection is checked separately above.', 'API에서 힌트를 받았습니다. 에이전트 연결 여부는 위 단계에서 별도로 확인하세요.') : t('The API responded, but no hints match this request. Try another public route from the library.', 'API는 응답했지만 일치하는 힌트가 없습니다. 메모리 라이브러리에서 다른 공개 경로를 골라보세요.')}</div>
						{#each recallResult.hits as hit}
							<article class="hint-preview"><p>{hit.hint}</p></article>
						{/each}
						<details class="response-details"><summary>{t('View JSON response', 'JSON 응답 보기')}</summary><div class="code-panel sample-response"><pre><code>{JSON.stringify(recallResult, null, 2)}</code></pre></div></details>
					{/if}
				</details>

				<section id="account" class="soft-card guide-panel">
					<div class="section-label">{t("03 / OPTIONAL ACCOUNT", "03 / 계정 연결 (선택)")}</div>
					<h2>{t("Ready to give a useful path back?", "유용한 방법을 다른 사람에게도 남기고 싶다면")}</h2>
					<p class="guide-description">{t("An account connects contributions, outcomes, and your private rank. Public recall works without one.", "기여·사용 결과·내 순위를 확인할 때 계정을 연결하세요. 공개 힌트 조회에는 계정이 필요하지 않습니다.")}</p>
					<div class="account-link"><div><strong>{t("Your WAM web account", "WAM 웹 계정")}</strong><p>{authConfig?.github_oauth_enabled ? t("Sign in through GitHub, then approve the host or device connection when prompted.", "GitHub로 로그인하고 안내에 따라 호스트 또는 기기 연결을 승인하세요.") : authLoading ? t("Checking this deployment’s sign-in options…", "이 서버의 로그인 옵션을 확인하고 있습니다…") : authConfig ? t("GitHub sign-in is disabled on this deployment. A service operator must configure it before new accounts can link.", "이 서버는 GitHub 로그인이 꺼져 있습니다. 새 계정을 연결하려면 운영자가 먼저 설정해야 합니다.") : t("Sign-in availability could not be checked. Open the login page to retry.", "로그인 가능 여부를 확인하지 못했습니다. 로그인 페이지에서 다시 시도하세요.")}</p></div><a class="button button-light button-small" href="/login">{authConfig?.github_oauth_enabled ? t("Sign in with GitHub", "GitHub로 로그인") : t("Open sign-in", "로그인 페이지 열기")} <span aria-hidden="true">↗</span></a></div>
					{#if selectedHost === 'arise'}
						<p class="field-help">{t("Authorize through Arise if your version supports this server’s OAuth flow. Live Arise OAuth acceptance is pending. Arise browser access and WAM account access have separate authorization.", "사용 중인 Arise가 이 서버의 OAuth를 지원하면 Arise에서 인증하세요. 실제 Arise OAuth 수용 검증은 남아 있습니다. Arise 브라우저 권한과 WAM 계정 권한은 별도로 승인합니다.")}</p>
					{:else if transport === 'remote' && oauthAvailable}
						<div class="status-line"><span class="status-dot"></span>{t("Remote OAuth enabled for this endpoint", "이 주소에서 원격 OAuth 사용 가능")}</div>
						<p class="field-help">{loginHelp}</p>
						<InstallCode {korean} label={loginLabel} code={commands?.login || missingEndpoint} disabled={!commands} />
					{:else}
						{#if transport === 'remote'}<p class="field-help">{authLoading ? t("Checking this deployment’s OAuth support…", "이 서버의 OAuth 지원 여부를 확인하고 있습니다…") : t("OAuth has not been confirmed for this endpoint. Public remote reads remain available. For account features, choose Local stdio above, build the tools, then link your device.", "이 주소의 OAuth 지원을 확인하지 못했습니다. 공개 원격 조회는 사용할 수 있습니다. 계정 기능은 위에서 로컬 stdio를 선택하고 도구를 빌드한 뒤 기기를 연결하세요.")}</p>{/if}
						<p class="field-help">{t("With local tools installed, run the commands below. Approve the displayed code in your signed-in WAM browser, then check the credential and service status.", "로컬 도구를 설치한 뒤 아래 명령을 실행하세요. 로그인한 WAM 브라우저에서 표시된 코드를 승인하고 자격증명과 서비스 상태를 확인하세요.")}</p>
						<InstallCode {korean} label={t("Link this device · terminal", "이 기기 연결 · 터미널")} code={commands ? commands.link + '\n' + commands.doctor : missingEndpoint} disabled={!commands} />
					{/if}
					<p class="field-help">{t("After authorization, ask your agent for", "인증 후 에이전트에")} <code>web_memory_my_rank</code> {t("or", "또는")} <code>web_memory_my_usage</code> {t("to verify private access.", "호출을 요청해 계정 접근을 확인하세요.")} <a href="/settings/devices">{t("Manage connected devices ↗", "연결된 기기 관리 ↗")}</a></p>
				</section>

				<details class="soft-card guide-panel advanced-panel">
					<summary>{t("Use WAM from the CLI or HTTP API", "CLI 또는 HTTP API로 WAM 사용")}</summary>
					<p>{t("The CLI uses the local tools built above. HTTP needs no local WAM binary. Both examples follow the endpoint selected on this page.", "CLI는 위에서 빌드한 로컬 도구를 사용합니다. HTTP는 WAM 바이너리가 필요 없습니다. 두 예시 모두 이 페이지에서 고른 서비스 주소를 사용합니다.")}</p>
					<div class="filter-tabs" role="group" aria-label={t("API example", "API 예시")}><button class="filter-tab" class:active={example === 'cli'} type="button" aria-pressed={example === 'cli'} onclick={() => (example = 'cli')}>WAM CLI</button><button class="filter-tab" class:active={example === 'http'} type="button" aria-pressed={example === 'http'} onclick={() => (example = 'http')}>HTTP API</button></div>
					<InstallCode {korean} label={example === 'cli' ? t("First anonymous CLI recall", "첫 익명 CLI 조회") : t("HTTP API · anonymous recall", "HTTP API · 익명 조회")} code={(example === 'cli' ? commands?.recall : commands?.http) || missingEndpoint} disabled={!commands} />
					<p>{t("For a local service, keep", "로컬 서버는 별도 터미널에서")} <code>cargo run -p wam-server</code> {t("running in a separate terminal and use", "을 계속 실행하고")} <code>http://127.0.0.1:3000/mcp</code>{t(". Public recall needs no demo token. New account linking needs GitHub OAuth configured on the server.", "를 사용하세요. 공개 조회에는 데모 토큰이 필요 없습니다. 새 계정 연결에는 서버의 GitHub OAuth 설정이 필요합니다.")}</p>
				</details>

				{#if selectedHost === 'muse'}
					<details class="soft-card guide-panel advanced-panel">
						<summary>{t("Run Muse Spark inside Claude Code (Meta Model API key)", "Claude Code에서 Muse Spark 쓰기 (Meta Model API 키)")}</summary>
						<p>{t("Claude Code can run on Muse Spark through the Meta Model API, so a Muse-driven session reaches the same WAM memory. Export your Model API key as MODEL_API_KEY, set these variables, then add WAM to Claude Code. Checked 2026-11-01 with Claude Code 2.1.316 and muse-spark-1.3.", "Claude Code는 Meta Model API로 Muse Spark를 모델로 쓸 수 있고, 이 세션도 같은 WAM 메모리를 조회합니다. Model API 키를 MODEL_API_KEY로 내보낸 뒤 아래 변수를 설정하고 Claude Code에 WAM을 추가하세요. 2026-11-01에 Claude Code 2.1.316과 muse-spark-1.3으로 확인했습니다.")}</p>
						<InstallCode {korean} label={t('Claude Code on Muse Spark · environment', 'Muse Spark로 쓰는 Claude Code · 환경변수')} code={museEnv} />
						<InstallCode {korean} label={t('Claude Code · add remote MCP', 'Claude Code · 원격 MCP 추가')} code={claudeOnMuse?.remote || missingEndpoint} disabled={!claudeOnMuse} />
						<p>{t("Keep tool search off: WAM has ten tools, and with it on Muse spent extra turns searching. A short test costs a few cents of API credit. Codex on Muse does not work yet: Meta's Responses API returns tool calls in a form Codex cannot route (checked 2026-11-01).", "tool search는 꺼 두세요. WAM 도구는 10개뿐이라, 켜면 Muse가 도구를 찾느라 턴을 더 씁니다. 짧은 시험은 API 크레딧을 몇 센트 씁니다. Codex에 Muse를 붙이는 구성은 아직 동작하지 않습니다. Meta의 Responses API가 돌려주는 도구 호출 형식을 Codex가 처리하지 못합니다 (2026-11-01 확인).")}</p>
					</details>
				{/if}

				{#if selectedHost === 'codex' || selectedHost === 'claude-code'}
					<details class="soft-card guide-panel advanced-panel">
						<summary>{t("Recall from an Arise browser tab (optional local bridge)", "Arise 브라우저 탭에서 조회 (로컬 브리지, 선택)")}</summary>
						<p>{t("This stdio bridge keeps Arise’s browser MCP connected and adds", "이 stdio 브리지는 Arise 브라우저 MCP 연결을 유지하고")} <code>web_memory_recall_current_tab</code>{t(". Use the browser tools and this recall tool from the same", "도구를 추가합니다. 브라우저 동작과 탭 조회는 같은")} <code>wam-browser</code> {t("entry. The URL is read and normalized locally when your agent requests recall.", "항목의 도구를 사용하세요. 에이전트가 조회를 요청할 때 URL을 로컬에서 읽고 정규화합니다.")}</p>
						<p>{t("Build the local tools first. Enter absolute executable paths from your own installation. On macOS,", "로컬 도구를 먼저 빌드하고 본인 기기에 설치된 실행 파일의 절대 경로를 입력하세요. macOS에서는")} <code>command -v wam</code> {t("shows the CLI path; on Windows, use", "으로 CLI 경로를 확인하고 Windows에서는")} <code>Get-Command wam</code>{t(". Native Arise acceptance has been checked on macOS; other OS runtime acceptance is pending.", "을 사용하세요. macOS의 Arise는 검증했으며 다른 OS의 실행 검증은 남아 있습니다.")}</p>
						<div class="field"><label for="wam-binary">{t("Absolute path to wam", "wam 절대 경로")}</label><input id="wam-binary" type="text" bind:value={wamBinary} placeholder={selectedPlatform === 'Windows' ? 'C:\\Users\\you\\.cargo\\bin\\wam.exe' : '/Users/you/.cargo/bin/wam'} spellcheck="false" /></div>
						<div class="field"><label for="browser-binary">{t("Absolute path to Arise browser MCP", "Arise 브라우저 MCP 절대 경로")}</label><input id="browser-binary" type="text" bind:value={browserBinary} placeholder={selectedPlatform === 'macOS' ? '/Applications/Arise.app/Contents/MacOS/agent_browser' : t("Absolute executable path", "실행 파일의 절대 경로")} spellcheck="false" /></div>
						<InstallCode {korean} label={t("Register browser bridge", "브라우저 브리지 등록")} code={bridgeCommand || t("Enter both absolute executable paths and a valid endpoint to generate the command.", "두 실행 파일의 절대 경로와 올바른 서버 주소를 입력하면 명령이 생성됩니다.")} disabled={!bridgeCommand} />
						<p>{t("Open a new agent session and use its", "새 에이전트 세션에서")} <code>wam-browser</code> {t("tools to open a tab. Then call", "의 브라우저 도구로 탭을 여세요. 다음으로")} <code>web_memory_recall_current_tab</code> {t("with a task category and allowlisted keywords. It uses only that connection’s current owned tab. Missing, expired, changed, or foreign tabs fail without choosing another tab. Your agent must honor requests to skip memory.", "에 작업 유형과 허용 키워드를 전달하세요. 해당 연결이 소유한 현재 탭만 사용하며, 탭이 없거나 만료·변경되면 다른 탭을 선택하지 않고 실패합니다. 사용자가 메모리를 사용하지 말라고 하면 그 지시를 따라야 합니다.")}</p>
					</details>
				{/if}



				<details class="soft-card guide-panel advanced-panel">
					<summary>{t("Disconnect or remove WAM", "WAM 연결 해제·제거")}</summary>
					<p>{t("Remove the host registration when you no longer want this agent to load WAM.", "에이전트에서 WAM을 쓰지 않으려면 호스트 등록을 제거하세요.")}</p>
					{#if selectedHost === 'codex' || selectedHost === 'claude-code'}<InstallCode {korean} label={t("Remove host registration", "호스트 등록 제거")} code={commands?.remove || missingEndpoint} disabled={!commands} />{:else if selectedHost === 'muse'}<p>{t("Delete the web-memory entry from", "다음 파일에서 web-memory 항목을 지우세요:")} <code>{museSettingsPath}</code></p>{:else}<p>{t("Remove", "Arise MCP Hub에서")} <code>web-memory</code> {t("from Arise MCP Hub.", "항목을 제거하세요.")}</p>{/if}
					{#if selectedHost === 'codex' || selectedHost === 'claude-code'}<p>{t("If you added the optional browser bridge, remove its separate entry too.", "선택형 브라우저 브리지를 등록했다면 별도 항목도 제거하세요.")}</p><InstallCode {korean} label={t("Remove optional browser bridge", "선택형 브라우저 브리지 제거")} code={selectedHost === 'codex' ? 'codex mcp remove wam-browser' : 'claude mcp remove --scope user wam-browser'} />{/if}
					<p>{t("For a linked local device, revoke its credential with", "연결된 로컬 기기의 자격증명은")} <code>{commands?.unlink || 'wam unlink'}</code>{t(". For remote OAuth, revoke the connection in", "로 해지하세요. 원격 OAuth는")} <a href="/settings/devices">{t("Connected devices", "연결된 기기")}</a>{t(". Removing a host entry alone does not revoke account access.", "에서 해지하세요. 호스트 등록 제거만으로 계정 접근이 해지되지는 않습니다.")}</p>
					<p>{t("Remove source-installed binaries separately:", "소스로 설치한 바이너리는 별도로 제거하세요:")} <code>cargo uninstall wam-cli</code> {t("and", "및")} <code>cargo uninstall wam-mcp</code>.</p>
				</details>
			</div>

			<aside class="guide-aside">
				<section class="soft-card guide-side-card setup-overview">
					<span class="section-label">{t("WHAT YOU’RE CONNECTING", "연결 구조")}</span>
					<h3>{t("Your agent.", "내 에이전트,")}<br />{t("Shared experience.", "공유된 경험.")}</h3>
					<div class="connection-map" aria-label={t("Your agent uses WAM for workflow recall and browser tools for actions", "에이전트는 WAM으로 힌트를 조회하고 브라우저 도구로 작업합니다.")}><div><strong>{hostName}</strong><small>{t("Your agent", "내 에이전트")}</small></div><span aria-hidden="true">↓</span><div class="map-memory"><strong>WAM MCP</strong><small>{t("Recall workflows · share with consent", "워크플로 조회 · 동의 후 공유")}</small></div><span aria-hidden="true">+</span><div><strong>{t("Your browser tools", "기존 브라우저 도구")}</strong><small>{t("Navigate · edit · save", "이동 · 편집 · 저장")}</small></div></div>
					<p>{t("Keep your browser MCP for page actions. Add WAM for workflow hints your agent can consult along the way.", "브라우저 작업은 기존 MCP를 사용하고, WAM으로 작업 도중 참고할 힌트를 더하세요.")}</p>
					<a class="resource-link" href="/memories">{t("Inspect shared workflows", "공유 워크플로 둘러보기")} <span aria-hidden="true">↗</span></a>
				</section>
				<section class="soft-card guide-side-card">
					<span class="section-label">{t("AVAILABLE TODAY", "지금 사용 가능")}</span>
					<h3>{t("10 tools. One shared service.", "공유 서비스 하나, 도구 10개.")}</h3>
					<dl class="tool-groups"><div><dt>{t("Public", "공개 조회")}</dt><dd>{t("Status, recall, expand, leaderboard", "상태, 힌트 조회, 상세 조회, 리더보드")}</dd></div><div><dt>{t("Linked account", "계정 연결 후")}</dt><dd>{t("My rank, my usage, propose, my contributions, withdraw, outcome", "내 순위, 사용량, 기여, 내 기여, 철회, 결과 보고")}</dd></div></dl>
					<p>{t("Remote HTTP and local stdio use this catalog. Contributions need explicit public-sharing consent. Only versions you actually used can earn outcome points.", "원격 HTTP와 로컬 stdio 모두 같은 도구를 제공합니다. 기여에는 명시적 공개 공유 동의가 필요하며 실제 사용한 버전에만 결과 점수가 적용됩니다.")}</p>
				</section>
				<section class="soft-card guide-side-card support-status">
					<span class="section-label">{t("IMPLEMENTATION & LIMITS", "확인 범위와 한계")}</span>
					<h3>{t("What has been checked?", "어디까지 확인했나요?")}</h3>
					<ul><li><span class="pill pill-green">{t("Implemented", "구현 완료")}</span><p>{t("HTTP + stdio MCP, pinned-version expand, auth/consent status, GET/POST SSE, device linking and OAuth lifecycle.", "HTTP·stdio MCP, 버전 고정 상세 조회, 인증·동의 상태, GET/POST SSE, 기기 연결과 OAuth 수명주기.")}</p></li><li><span class="pill">{t("Locally verified", "로컬 검증")}</span><p>{t("macOS credentials, Codex/Claude registration, synthetic GitHub sign-in, native Codex OAuth, and Arise leased-tab recall through the local bridge.", "macOS 자격증명, Codex·Claude 등록, 합성 GitHub 로그인, Codex OAuth, 로컬 브리지의 Arise 소유 탭 조회.")}</p></li><li><span class="pill">{t("Muse checked", "Muse 확인")}</span><p>{t("Muse Code 1.4.14 and Claude Code 2.1.316 running Muse Spark 1.3 called WAM status and recall on a local server and on the public service (2026-11-01). Not working: Codex on Muse, because of a tool-call format mismatch. Not yet verified: signing in to WAM from Muse Code, local stdio, browser tasks.", "Muse Code 1.4.14와 Muse Spark 1.3을 쓰는 Claude Code 2.1.316이 로컬 서버와 공개 서비스에서 WAM 상태·조회를 호출했습니다(2026-11-01). 동작하지 않음: Codex에 Muse를 붙이는 구성(도구 호출 형식 불일치). 미검증: Muse Code에서 WAM 로그인, 로컬 stdio, 브라우저 작업.")}</p></li><li><span class="pill pill-coral">{t("Acceptance pending", "추가 검증 필요")}</span><p>{t("Live GitHub accounts, Claude/Arise remote OAuth, Windows/Linux runtime, and installation from a real signed release.", "실계정 GitHub 여정, Claude·Arise 원격 OAuth, Windows·Linux 실행, 실제 서명 릴리스 설치.")}</p></li><li><span class="pill">{t("Boundaries", "동작 범위")}</span><p>{t("The local bridge reads a tab when the agent calls its recall tool. Background injection, server-pushed workflow updates, and SSE replay are not provided.", "로컬 브리지는 에이전트가 조회 도구를 호출할 때 탭을 읽습니다. 백그라운드 주입, 서버의 워크플로 푸시, SSE 재생은 제공하지 않습니다.")}</p></li></ul>
				</section>
				<section class="soft-card guide-side-card troubleshooting">
					<span class="section-label">{t("NEED A HAND?", "문제가 생겼다면")}</span>
					<h3>{t("Start with the failing step.", "막힌 단계부터 확인하세요.")}</h3>
					<details><summary>{t("Command not found", "명령을 찾을 수 없음")}</summary><p>{t("Install the selected host’s CLI. For local stdio, build both WAM tools and add Cargo’s bin directory to PATH.", "선택한 호스트의 CLI를 설치하세요. 로컬 stdio는 두 WAM 도구를 빌드하고 Cargo의 bin 폴더를 PATH에 추가하세요.")}</p></details>
					<details><summary>{t("Registered, but no tools", "등록했지만 도구가 안 보임")}</summary><p>{t("Start a new agent session and check the endpoint. A saved entry confirms registration; a successful tools/list confirms discovery.", "새 에이전트 세션을 열고 주소를 확인하세요. 저장된 항목은 등록, 성공한 tools/list는 도구 탐색을 확인합니다.")}</p></details>
					<details><summary>{t("401 or 403 on private tools", "계정 도구에서 401·403")}</summary><p>{t("Authorize this host or link the local device and approve the required scopes. Website sign-in alone does not authorize an MCP host.", "호스트 인증 또는 로컬 기기 연결 후 필요한 권한을 승인하세요. 웹사이트 로그인만으로 MCP 호스트가 인증되지는 않습니다.")}</p></details>
					<details><summary>{t("429 or service unavailable", "429 또는 서비스 연결 불가")}</summary><p>{t("Respect Retry-After before retrying. Check the address and TLS. For local development, keep wam-server running.", "Retry-After가 지나면 다시 시도하세요. 주소와 TLS를 확인하고, 로컬 개발에서는 wam-server를 계속 실행하세요.")}</p></details>
					<div class="official-links"><a href="https://developers.openai.com/codex/mcp" target="_blank" rel="noreferrer">{t("Codex MCP docs ↗", "Codex MCP 공식 문서 ↗")}</a><a href="https://code.claude.com/docs/en/mcp" target="_blank" rel="noreferrer">{t("Claude Code MCP docs ↗", "Claude Code MCP 공식 문서 ↗")}</a><a href="https://dev.meta.ai/docs/muse-code/extending" target="_blank" rel="noreferrer">{t("Muse Code MCP docs ↗", "Muse Code MCP 공식 문서 ↗")}</a></div>
				</section>
			</aside>
		</div>
	</div>
</section>

<style>
	.guide-languages { margin-bottom: 18px; font-size: 12px; }
	.guide-languages a[aria-current='page'] { font-weight: 700; text-decoration: underline; text-underline-offset: 4px; }
	.host-prerequisite { padding: 16px; margin-block: 15px; border: 1px solid #dce5d8; border-radius: 8px; background: #f5f7ed; }
	.host-prerequisite p { margin-top: 0; color: var(--setup-muted); font-size: 12px; line-height: 1.8; }
	.host-prerequisite .text-link, .page-heading .text-link { font-size: 13px; }
	.page-heading .text-link { display: inline-block; margin-top: 14px; }
	.hint-preview { padding: 16px; margin-top: 12px; border: 1px solid #dce5d8; border-radius: 8px; background: #f5f8ef; }
	.hint-preview p { margin: 0; white-space: pre-line; font-size: 13px; line-height: 1.8; color: #34543f; }
	.response-details { margin-top: 16px; }
	.recall-status { margin-top: 18px; font-size: 13px; line-height: 1.8; color: #34543f; }
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
	.guide-actions { display: flex; flex-wrap: wrap; gap: 12px; margin: 18px 0; }
	.guide-description { color: var(--setup-muted); font-size: 13px; line-height: 1.8; }
	.guide-panel .field-help { font-size: 12px; line-height: 1.8; color: var(--setup-muted); }
	.field-help a, .setup-details a { text-decoration: underline; text-underline-offset: 3px; }
	.guide-panel code { overflow-wrap: anywhere; }
	.host-picker { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 9px; margin: 23px 0 20px; }
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
	@media (max-width: 900px) { .host-picker { grid-template-columns: repeat(2, minmax(0, 1fr)); } .guide-aside { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 15px; } .setup-path a { padding: 15px 10px; gap: 9px; } .setup-path i { display: none; } }
	@media (max-width: 600px) { .setup-path { grid-template-columns: 1fr; } .setup-path a + a { border-left: 0; border-top: 1px solid #d7e0d1; } .setup-path a { padding: 13px; } .setup-path i { display: block; } .guide-panel { padding: 22px 18px; } .guide-panel h2 { font-size: 23px; } .host-picker { grid-template-columns: 1fr; } .guide-aside { grid-template-columns: 1fr; } .account-link { flex-direction: column; align-items: flex-start; } .guide-panel :global(.code-panel pre) { font-size: 10px; } }
</style>
