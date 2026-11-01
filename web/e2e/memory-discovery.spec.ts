import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { readFileSync } from 'node:fs';

type Seed = { id: string; payload: { l0: { site_host: string; task_category: string; url_patterns: string[] }; l1: { title: string; summary: string } } };
const seed: Seed[] = JSON.parse(readFileSync(new URL('../../crates/wam-server/seed/example_memories.json', import.meta.url), 'utf8'));
const memories = seed.map(({ id, payload: { l0, l1 } }) => ({
	memory_id: id, version_id: 'v1', title: l1.title, summary: l1.summary,
	domain: l0.site_host, site: l0.site_host, task_category: l0.task_category,
	kind: 'workflow', status: 'active', verified_date: '1793491200',
	canonical_url_patterns: l0.url_patterns.map((path: string) => 'https://' + l0.site_host + path)
}));

test.beforeEach(async ({ page }) => {
	await page.route('**/v1/auth/session', route => route.fulfill({ status: 401, json: { error: 'unauthorized' } }));
	await page.route('**/v1/auth/config', route => route.fulfill({ json: {} }));
	await page.route('**/v1/telemetry/**', route => route.fulfill({ status: 202, json: {} }));
	await page.route('**/v1/memories', route => route.fulfill({ json: { schema_version: 1, memories } }));
});

for (const width of [1440, 390, 320]) {
	test(`landing URL search at ${width}px opens only the website's workflows without sending the URL`, async ({ page }, testInfo) => {
		await page.setViewportSize({ width, height: 1000 });
		const outgoing: string[] = [];
		page.on('request', request => outgoing.push(request.url()));
		await page.goto('/ko');
		const discovery = page.locator('#discover');
		await discovery.scrollIntoViewIfNeeded();
		await expect(discovery.getByRole('heading')).toContainText('무엇을 할 수 있을까요?');
		await expect(discovery.getByRole('link', { name: 'Google Sheets', exact: true })).toBeVisible();
		await discovery.screenshot({ path: testInfo.outputPath(`discovery-${width}.png`) });
		const audit = await new AxeBuilder({ page }).include('#discover').withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
		expect(audit.violations.map(issue => issue.id)).toEqual([]);
		expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);

		await discovery.getByRole('searchbox').fill('https://docs.google.com/spreadsheets/d/private-document/edit?token=private-token#gid=1');
		await discovery.getByRole('button', { name: '메모리 찾기' }).click();
		await expect(page.getByRole('searchbox')).toHaveValue(/docs.google.com\/spreadsheets/);
		await expect(page.locator('.memory-card')).toHaveCount(2);
		await expect(page.locator('.memory-card .card-product')).toHaveText(['Google Sheets', 'Google Sheets']);
		expect(outgoing.some(url => /private-document|private-token/.test(url))).toBe(false);
		await expect(page.locator('.template-card')).toHaveCount(0);
		await page.screenshot({ path: testInfo.outputPath(`url-results-${width}.png`), fullPage: true });
		const resultsAudit = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
		expect(resultsAudit.violations.map(issue => issue.id)).toEqual([]);

		await page.getByRole('searchbox').fill('https://unknown.example.com/?google=docs');
		await expect(page.getByRole('heading', { name: 'No matching memories' })).toBeVisible();
		await page.getByRole('button', { name: 'Clear search and filters' }).click();
		await expect(page.locator('.memory-card')).toHaveCount(memories.length);
	});
}

test('site shortcuts and English landing support discovery without signing in', async ({ page }) => {
	await page.goto('/');
	const discovery = page.locator('#discover');
	await expect(discovery.getByRole('button', { name: 'Find workflows' })).toBeVisible();
	await discovery.getByRole('link', { name: 'Google Docs', exact: true }).click();
	await expect(page.locator('.memory-card')).toHaveCount(memories.filter(memory => memory.canonical_url_patterns.some(pattern => pattern.includes('/document/'))).length);
	await expect(page.locator('.memory-card .card-product')).not.toContainText(['Google Sheets']);
});

test('a genuinely empty catalog explains availability and offers a retry', async ({ page }) => {
	let calls = 0;
	await page.route('**/v1/memories', route => {
		calls++;
		return route.fulfill({ json: { schema_version: 1, memories: calls === 1 ? [] : memories } });
	});
	await page.goto('/memories');
	await expect(page.getByRole('heading', { name: 'No public memories yet.' })).toBeVisible();
	await expect(page.getByText('Contribute a workflow and follow its review status.')).toHaveCount(0);
	await page.getByRole('button', { name: 'Try again' }).click();
	await expect(page.locator('.memory-card')).toHaveCount(memories.length);
});
