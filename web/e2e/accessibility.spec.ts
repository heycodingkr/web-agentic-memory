import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { mockDashboard } from './support/dashboard';

test.beforeEach(async ({ page }) => {
	await page.route('**/v1/**', async route => {
		const path = new URL(route.request().url()).pathname;
		if (path === '/v1/auth/session') return route.fulfill({ status: 401, json: { error: 'unauthorized' } });
		if (path === '/v1/auth/config') return route.fulfill({ json: { github_oauth_enabled: true, mcp_oauth_enabled: true, mcp_resource: 'https://wam.example/mcp' } });
		if (path.includes('/telemetry/')) return route.fulfill({ status: 202 });
		return route.fulfill({ status: 401, json: { error: 'unauthorized' } });
	});
});

for (const path of ['/guide', '/contribute', '/dashboard', '/login']) {
	test(`WEB-33: ${path} responsive landmarks, names and automated WCAG A/AA scan`, async ({ page }, testInfo) => {
		await page.goto(path);
		await expect(page.getByRole('main')).toHaveCount(1);
		await expect(page.getByRole('heading', { level: 1 })).toHaveCount(1);
		await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
		await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
		// Wait for the asynchronous account state before inspecting dynamic UI.
		if (path === '/guide') await expect(page.getByLabel('WAM MCP endpoint', { exact: true })).toHaveValue('https://wam.example/mcp');
		if (path === '/dashboard') await expect(page.getByLabel('Local demo token', { exact: true })).toBeVisible();
		const report = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
		await testInfo.attach('axe-results', { body: JSON.stringify(report), contentType: 'application/json' });
		expect(report.violations.map(item => ({ id: item.id, impact: item.impact, targets: item.nodes.map(node => node.target) }))).toEqual([]);
	});
}

test('WEB-33: signed-in dashboard analytics and point history remain accessible on narrow screens', async ({ page }, testInfo) => {
	await mockDashboard(page);
	await page.goto('/dashboard');
	await expect(page.getByRole('heading', { name: 'Private authored workflow' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'My point history' })).toBeVisible();
	await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
	const report = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
	await testInfo.attach('axe-results-private', { body: JSON.stringify(report), contentType: 'application/json' });
	expect(report.violations.map(item => ({ id: item.id, impact: item.impact, targets: item.nodes.map(node => node.target) }))).toEqual([]);
});

test('WEB-34: keyboard reaches the main landmark and selects a host without a pointer', async ({ page, browserName }) => {
	// Safari's default macOS keyboard mode uses Option+Tab for links/buttons.
	const tab = browserName === 'webkit' && process.platform === 'darwin' ? 'Alt+Tab' : 'Tab';
	await page.goto('/guide');
	await expect(page.getByLabel('WAM MCP endpoint', { exact: true })).toHaveValue('https://wam.example/mcp');
	await page.keyboard.press(tab);
	await expect(page.getByRole('link', { name: 'Skip to main content' })).toBeFocused();
	await page.keyboard.press('Enter');
	await expect(page.getByRole('main')).toBeFocused();
	const target = page.getByRole('button', { name: 'Claude Code', exact: true });
	for (let index = 0; index < 40 && !(await target.evaluate(node => node === document.activeElement)); index++) await page.keyboard.press(tab);
	await expect(target).toBeFocused();
	await page.keyboard.press('Space');
	await expect(target).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByRole('button', { name: 'Copy Claude Code · add remote MCP', exact: true })).toBeEnabled();
});

test('WEB-35: mobile navigation is keyboard operable and restores its closed state', async ({ page, browserName }) => {
	const tab = browserName === 'webkit' && process.platform === 'darwin' ? 'Alt+Tab' : 'Tab';
	await page.setViewportSize({ width: 390, height: 844 });
	await page.goto('/guide');
	const open = page.getByRole('button', { name: 'Open menu', exact: true });
	await expect(open).toBeEnabled();
	for (let index = 0; index < 20 && !(await open.evaluate(node => node === document.activeElement)); index++) await page.keyboard.press(tab);
	await expect(open).toBeFocused();
	await page.keyboard.press('Enter');
	await expect(page.getByRole('button', { name: 'Close menu', exact: true })).toHaveAttribute('aria-expanded', 'true');
	await page.keyboard.press(tab);
	const ranking = page.getByRole('navigation', { name: 'Mobile navigation', exact: true }).getByRole('link', { name: 'Leaderboard' });
	await expect(ranking).toBeFocused();
	await page.keyboard.press('Enter');
	await expect(page).toHaveURL(/\/leaderboard$/);
	await expect(page.getByRole('navigation', { name: 'Mobile navigation', exact: true })).toHaveCount(0);
	await expect(page.getByRole('button', { name: 'Open menu', exact: true })).toHaveAttribute('aria-expanded', 'false');
});
