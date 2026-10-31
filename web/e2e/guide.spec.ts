import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
	await page.route('**/v1/auth/session', route => route.fulfill({ status: 401, json: { error: 'unauthorized' } }));
	await page.route('**/v1/telemetry/**', route => route.fulfill({ status: 202, json: {} }));
	await page.route('**/v1/auth/config', route => route.fulfill({ json: {
		github_oauth_enabled: true,
		mcp_oauth_enabled: true,
		mcp_resource: 'https://wam.example/team/mcp'
	} }));
});

test('setup controls wait until the page can handle clicks', async ({ page }) => {
	let resumeBoot!: () => void;
	const boot = new Promise<void>(resolve => { resumeBoot = resolve; });
	await page.route('**/*', async route => {
		if (route.request().resourceType() === 'script') await boot;
		await route.fallback();
	});
	try {
		await page.goto('/guide', { waitUntil: 'commit' });
		await expect(page.getByRole('button', { name: 'Claude Code', exact: true })).toBeDisabled();
		await expect(page.getByRole('button', { name: 'Copy First request · paste into your agent', exact: true })).toBeDisabled();
	} finally {
		resumeBoot();
	}
	await page.getByRole('button', { name: 'Claude Code', exact: true }).click();
	await expect(page.getByRole('button', { name: 'Claude Code', exact: true })).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByRole('link', { name: 'Install Claude Code', exact: true })).toBeVisible();
});

test('Korean landing keeps setup in Korean', async ({ page }) => {
	await page.goto('/ko');
	await page.locator('.lp-hero a.button-dark').click();
	await expect(page).toHaveURL(/\/ko\/guide$/);
	await expect(page.getByRole('heading', { level: 1 })).toContainText('에이전트를 연결하고');
	await expect(page.getByLabel('WAM MCP 주소', { exact: true })).toHaveValue('https://wam.example/team/mcp');
	await expect(page.getByRole('navigation', { name: '주 메뉴', exact: true }).getByRole('link', { name: '설치 안내', exact: true })).toHaveAttribute('aria-current', 'page');
});

test('remote setup provides host prerequisites and MCP instructions without a WAM download', async ({ page }) => {
	await page.goto('/guide');
	const prerequisite = page.getByRole('link', { name: 'Install Codex CLI', exact: true });
	await expect(prerequisite).toHaveAttribute('href', 'https://developers.openai.com/codex/cli');
	await expect(page.locator('.code-panel').filter({ hasText: 'Check host CLI' })).toContainText('codex --version');
	const firstRequest = page.locator('.code-panel').filter({ hasText: 'First request' });
	await expect(firstRequest).toContainText('https://docs.google.com/document/');
	await expect(firstRequest).toContainText('"document"');
	await page.getByText('Help the agent remember when to use WAM', { exact: true }).click();
	const instructions = page.locator('.code-panel').filter({ hasText: 'Project instructions' });
	await expect(instructions).toContainText('web_memory_recall');
	await expect(instructions).not.toContainText('wam recall');
	await expect(instructions).not.toContainText('wam guide');
	await page.getByRole('button', { name: 'Claude Code', exact: true }).click();
	await expect(page.getByRole('link', { name: 'Install Claude Code', exact: true })).toHaveAttribute('href', 'https://code.claude.com/docs/en/quickstart');
	await expect(page.locator('.code-panel').filter({ hasText: 'Check host CLI' })).toContainText('claude --version');
	await expect(page.locator('details').filter({ hasText: 'Help the agent remember when to use WAM' })).toContainText('CLAUDE.md');
});

test('ranking leads navigation and the catalog remains a secondary resource', async ({ page }) => {
	await page.goto('/guide');
	const navigation = page.getByRole('navigation', { name: 'Main navigation', exact: true });
	await expect(navigation.getByRole('link').first()).toHaveAttribute('href', '/leaderboard');
	await expect(navigation.locator('a[href="/memories"]')).toHaveCount(0);
	await expect(page.locator('footer a[href="/memories"]')).toBeVisible();
});

test('source setup does not send public users to a private repository', async ({ page }) => {
	for (const language of ['en', 'ko']) {
		await page.goto(language === 'ko' ? '/ko/guide' : '/guide');
		const source = page.locator('.source-tools');
		await source.locator('summary').click();
		await expect(source.locator('a[href*="github.com/heycodingkr/web-agentic-memory"]')).toHaveCount(0);
		await expect(source).toContainText(language === 'ko' ? '운영자가 제공한 소스 체크아웃' : 'source checkout provided by your service operator');
		await expect(source.locator('.code-panel')).toContainText('cargo install --path crates/wam-cli --locked');
	}
});

test('source installation does not immediately uninstall the CLI', async ({ page }) => {
	await page.goto('/guide');
	const installCommand = page.locator('.code-panel').filter({ hasText: 'cargo install --path' });
	await expect(installCommand).toHaveCount(1);
	await expect(installCommand).not.toContainText('cargo uninstall');
});

test('the API example follows the chosen service endpoint', async ({ page }) => {
	await page.goto('/guide');
	await expect(page.getByLabel('WAM MCP endpoint', { exact: true })).toHaveValue('https://wam.example/team/mcp');
	await page.getByLabel('WAM MCP endpoint', { exact: true }).fill('https://other.example/wam/mcp');
	await page.getByText('Use WAM from the CLI or HTTP API', { exact: true }).click();
	await page.getByText('HTTP API', { exact: true }).click();
	const example = page.locator('.code-panel').filter({ hasText: '/v1/memories/recall' });
	await expect(example).toContainText('https://other.example/wam/v1/memories/recall');
	await expect(example).not.toContainText('127.0.0.1:3000');
});

test('local stdio builds both binaries and uses the selected host and service', async ({ page }) => {
	await page.goto('/guide');
	await expect(page.getByLabel('WAM MCP endpoint', { exact: true })).toHaveValue('https://wam.example/team/mcp');
	await page.getByRole('button', { name: 'Local stdio', exact: true }).click();
	await page.getByRole('button', { name: 'Claude Code', exact: true }).click();
	await expect(page.locator('.code-panel').filter({ hasText: 'cargo install --path' })).toBeVisible();
	await expect(page.locator('.code-panel').filter({ hasText: 'cargo install --path' })).toContainText('crates/wam-mcp');
	await expect(page.locator('.code-panel').filter({ hasText: 'register local stdio' })).toContainText('wam --endpoint https://wam.example/team install claude-code --local');
	await expect(page.locator('.code-panel').filter({ hasText: 'Link this device' })).toContainText('wam --endpoint https://wam.example/team link');
	await page.getByRole('button', { name: 'Arise MCP Hub', exact: true }).click();
	await expect(page.getByRole('button', { name: 'Local stdio', exact: true })).toBeDisabled();
	await expect(page.getByText('Help the agent remember when to use WAM', { exact: true })).toHaveCount(0);
	await expect(page.locator('.code-panel').filter({ hasText: 'Arise MCP Hub · server URL' })).toContainText('https://wam.example/team/mcp');
});

test('copy contains only the current command and invalid endpoints disable copying', async ({ page }) => {
	await page.addInitScript(() => {
		Object.defineProperty(navigator, 'clipboard', { value: { writeText: async (text: string) => { (window as any).__copied = text; } } });
	});
	await page.goto('/guide');
	await expect(page.getByLabel('WAM MCP endpoint', { exact: true })).toHaveValue('https://wam.example/team/mcp');
	await page.getByRole('button', { name: 'Copy Codex · add remote MCP', exact: true }).click();
	expect(await page.evaluate(() => (window as any).__copied)).toBe('codex mcp add web-memory --url https://wam.example/team/mcp');
	await expect(page.getByRole('button', { name: 'Copy Codex · add remote MCP', exact: true })).toHaveText('COPIED');
	await page.getByRole('button', { name: 'Claude Code', exact: true }).click();
	await expect(page.getByRole('button', { name: 'Copy Claude Code · add remote MCP', exact: true })).toHaveText('COPY');
	await page.getByLabel('WAM MCP endpoint', { exact: true }).fill('https://wam.example/mcp?token=private-sentinel');
	await expect(page.getByRole('button', { name: 'Copy Claude Code · add remote MCP', exact: true })).toBeDisabled();
	await expect(page.getByLabel('WAM MCP endpoint', { exact: true })).toHaveAttribute('aria-invalid', 'true');
	await expect(page.locator('.code-panel').filter({ hasText: 'private-sentinel' })).toHaveCount(0);
});

test('OAuth guidance is bound to the deployment that reported support', async ({ page }) => {
	await page.goto('/guide');
	await expect(page.getByText('Remote OAuth enabled for this endpoint', { exact: true })).toBeVisible();
	await expect(page.locator('.code-panel').filter({ hasText: 'Authorize Codex' })).toContainText('codex mcp login web-memory');
	await page.getByLabel('WAM MCP endpoint', { exact: true }).fill('https://other.example/mcp');
	await expect(page.getByText('Remote OAuth enabled for this endpoint', { exact: true })).toHaveCount(0);
	await expect(page.getByText('OAuth has not been confirmed for this endpoint.', { exact: false })).toBeVisible();
});

test('public recall removes URL query and fragment and displays server failures', async ({ page }) => {
	let sent: any;
	await page.route('**/v1/memories/recall', route => {
		sent = route.request().postDataJSON();
		return route.fulfill({ json: { schema_version: 1, hits: [] } });
	});
	await page.goto('/guide');
	await expect(page.getByLabel('WAM MCP endpoint', { exact: true })).toHaveValue('https://wam.example/team/mcp');
	await expect(page.locator('#try')).toHaveAttribute('open', '');
	await page.getByLabel('Public HTTPS URL', { exact: true }).fill('https://example.com/guide?private-sentinel=secret#fragment');
	await page.getByRole('button', { name: 'Send test request' }).click();
	await expect(page.locator('.sample-response')).toContainText('"hits": []');
	expect(sent).toEqual({ url: 'https://example.com/guide', task_category: 'compose-draft', keywords: ['document'], limit: 3 });
	await page.route('**/v1/memories/recall', route => route.fulfill({ status: 429, headers: { 'Retry-After': '12' }, json: { error: 'rate_limited', message: 'Rate limit reached.' } }));
	await page.getByRole('button', { name: 'Send test request' }).click();
	await expect(page.getByRole('alert')).toContainText('Rate limit reached.');
	await expect(page.getByRole('alert')).not.toContainText('Run cargo');
});

test('Korean chrome, mobile ranking navigation, and guide layout', async ({ page }, testInfo) => {
	await page.setViewportSize({ width: 390, height: 844 });
	await page.goto('/ko');
	await expect(page.getByRole('button', { name: '메뉴 열기' })).toBeVisible();
	await page.getByRole('button', { name: '메뉴 열기' }).click();
	const mobile = page.getByRole('navigation', { name: '모바일 메뉴', exact: true });
	await expect(mobile.getByRole('link').first()).toHaveText('랭킹↗');
	await expect(mobile.getByRole('link').first()).toHaveAttribute('href', '/leaderboard');
	await page.goto('/guide');
	await expect(page.getByLabel('WAM MCP endpoint', { exact: true })).toHaveValue('https://wam.example/team/mcp');
	expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
	await page.screenshot({ path: testInfo.outputPath('guide-mobile.png') });
	await page.setViewportSize({ width: 1440, height: 1100 });
	await page.screenshot({ path: testInfo.outputPath('guide-desktop.png') });
	await page.route('**/v1/memories', route => route.fulfill({ json: { schema_version: 1, memories: [] } }));
	await page.goto('/memories');
	await expect(page.getByRole('heading', { name: /^No (?:public )?memories yet\.$/ })).toBeVisible();
	await expect(page.locator('main').getByRole('link', { name: 'Connect your agent' })).toBeVisible();
	await page.screenshot({ path: testInfo.outputPath('catalog-desktop.png') });
	await page.route('**/v1/leaderboard', route => route.fulfill({ json: { schema_version: 1, entries: [] } }));
	await page.goto('/leaderboard');
	await expect(page.getByRole('heading', { name: 'No public contributors yet.' })).toBeVisible();
	await page.screenshot({ path: testInfo.outputPath('ranking-desktop.png') });
});

test('implemented web sign-in is described accurately', async ({ page }) => {
	await page.goto('/guide');
	await expect(page.getByText('GitHub OAuth is planned', { exact: false })).toHaveCount(0);
	await expect(page.getByRole('link', { name: 'Sign in with GitHub', exact: true })).toBeVisible();
});

test('optional browser bridge follows local paths, host, endpoint and cleanup scope', async ({ page }, testInfo) => {
	await page.goto('/guide');
	await page.getByText('Recall from an Arise browser tab (optional local bridge)', { exact: true }).click();
	const copy = page.getByRole('button', { name: 'Copy Register browser bridge', exact: true });
	await expect(copy).toBeDisabled();
	await page.getByLabel('Absolute path to wam', { exact: true }).fill('/Users/test user/bin/wam');
	await page.getByLabel('Absolute path to Arise browser MCP', { exact: true }).fill('/Applications/Arise.app/Contents/MacOS/agent_browser');
	await expect(copy).toBeEnabled();
	const bridge = page.locator('.code-panel').filter({ hasText: 'Register browser bridge' });
	await expect(bridge).toContainText("codex mcp add wam-browser -- '/Users/test user/bin/wam' --endpoint https://wam.example/team browser-mcp");
	await page.getByRole('button', { name: 'Claude Code', exact: true }).click();
	await expect(bridge).toContainText('claude mcp add --scope user wam-browser');
	await page.getByText('Disconnect or remove WAM', { exact: true }).click();
	await expect(page.locator('.code-panel').filter({ hasText: 'Remove optional browser bridge' })).toContainText('claude mcp remove --scope user wam-browser');
	const bridgePanel = page.locator('details').filter({ has: page.getByText('Recall from an Arise browser tab (optional local bridge)', { exact: true }) });
	await bridgePanel.screenshot({ path: testInfo.outputPath('browser-bridge-desktop.png') });
	await page.setViewportSize({ width: 390, height: 844 });
	expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
	await bridgePanel.screenshot({ path: testInfo.outputPath('browser-bridge-mobile.png') });
	await page.getByLabel('WAM MCP endpoint', { exact: true }).fill('https://other.example/mcp?secret=sentinel');
	await expect(copy).toBeDisabled();
	await expect(bridge).not.toContainText('sentinel');
});

for (const [language, width] of [['en', 1440], ['ko', 390]] as const) {
	test(`live public onboarding from ${language} landing at ${width}px`, async ({ page }, testInfo) => {
		test.skip(process.env.WAM_LIVE_ONBOARDING !== '1', 'Opt in against a running development API with built-in examples.');
		await page.unroute('**/v1/auth/config');
		await page.setViewportSize({ width, height: 1000 });
		await page.goto(language === 'ko' ? '/ko' : '/');
		await page.locator('.lp-hero a.button-dark').click();
		await expect(page).toHaveURL(language === 'ko' ? /\/ko\/guide$/ : /\/guide$/);
		await expect(page.locator('html')).toHaveAttribute('lang', language);
		await expect(page.locator('#mcp-endpoint')).toHaveValue(/\/mcp$/);
		await page.locator('#connect').screenshot({ path: testInfo.outputPath(`setup-${language}-${width}.png`) });
		await page.getByRole('button', { name: language === 'ko' ? '힌트 조회하기' : 'Send test request', exact: true }).click();
		await expect(page.locator('.hint-preview').first()).toContainText('Google Docs');
		await expect(page.locator('#try [role="status"]')).toContainText(language === 'ko' ? 'API에서 힌트를 받았습니다' : 'The API returned hints');
		await expect(page.locator('#try')).toContainText('Example:');
		expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
		await page.locator('#try').screenshot({ path: testInfo.outputPath(`first-hints-${language}-${width}.png`) });
	});
}
