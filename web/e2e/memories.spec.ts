import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';

// The bundled example memories, served the way the API serves them: list, level 1 hint and level 2 JSON.
type Seed = { id: string; payload: { l0: { site_host: string; url_patterns: string[]; task_category: string }; l1: { title: string; summary: string; key_points: string[]; pitfalls: string[] }; l2: unknown } };
const seed: Seed[] = JSON.parse(readFileSync(new URL('../../crates/wam-server/seed/example_memories.json', import.meta.url), 'utf8'));
const contributed = {
	memory_id: 'web:contributed-demo',
	version_id: 'ver-contributed',
	title: 'Save a clean draft',
	summary: 'Enter a title and body, then save a draft without publishing it.',
	site: 'example.com',
	domain: 'example.com',
	task_category: 'compose-draft',
	kind: 'workflow',
	status: 'active',
	verified_date: '1798000000',
	canonical_url_patterns: ['https://example.com/guide/**']
};
const listed = seed.map(({ id, payload: { l0, l1 } }) => ({
	memory_id: id,
	version_id: `ver-${id}`,
	title: l1.title,
	summary: l1.summary,
	site: l0.site_host,
	domain: l0.site_host,
	task_category: l0.task_category,
	kind: 'workflow',
	status: 'active',
	verified_date: '1793318400',
	canonical_url_patterns: l0.url_patterns.map((route) => `https://${l0.site_host}${route.replace(/\/\*$/, '/**')}`)
}));
const catalog = [...listed, contributed];

async function mock(page: Page) {
	await page.route('**/v1/auth/session', (route) => route.fulfill({ status: 401, json: { error: 'unauthorized' } }));
	await page.route('**/v1/telemetry/**', (route) => route.fulfill({ status: 202, json: {} }));
	await page.route('**/v1/auth/config', (route) => route.fulfill({ json: { github_oauth_enabled: false, mcp_oauth_enabled: false } }));
	await page.route('**/v1/memories', (route) => route.fulfill({ json: { schema_version: 1, memories: catalog } }));
	await page.route('**/v1/memories/*?level=*', (route) => {
		const url = new URL(route.request().url());
		const id = decodeURIComponent(url.pathname.split('/').pop() ?? '');
		const level = Number(url.searchParams.get('level'));
		const record = seed.find((item) => item.id === id);
		if (!record) return route.fulfill({ status: 404, json: { error: 'not_found' } });
		const { l1, l2 } = record.payload;
		const hint = [l1.title, l1.summary, ...l1.key_points.map((point) => `- ${point}`), ...(l1.pitfalls.length ? [`pitfalls: ${l1.pitfalls.join(', ')}`] : [])].join('\n');
		return route.fulfill({ json: { schema_version: 1, memory_id: id, version_id: `ver-${id}`, level, content: level === 1 ? hint : JSON.stringify(l2) } });
	});
}

const cards = (page: Page) => page.locator('.memory-card h3');

for (const width of [1440, 390, 320]) {
	test(`memory library at ${width}px: search, filters and a detail page work on the bundled examples`, async ({ page, context }, testInfo) => {
		await context.grantPermissions(['clipboard-read', 'clipboard-write']).catch(() => {});
		await mock(page);
		await page.setViewportSize({ width, height: 1000 });
		await page.goto('/memories');
		await expect(page.locator('h1')).toContainText('Find the workflow');
		await expect(cards(page)).toHaveCount(catalog.length);
		await expect(page.locator('.results-line')).toContainText(`${catalog.length} of ${catalog.length} workflows · ${seed.length} built-in examples`);

		// Everyday tools come first and are labelled as examples.
		await expect(page.locator('.memory-card').first()).toContainText('Google Docs');
		await expect(page.locator('.memory-card').first().locator('.pill-example')).toHaveText('Example');
		const plain = page.locator('.memory-card', { has: page.getByRole('heading', { name: 'Save a clean draft', exact: true }) });
		await expect(plain.locator('.pill-example')).toHaveCount(0);
		await expect(plain).toContainText('Verified 2026-12-23');

		// Words people would use.
		const search = page.getByRole('searchbox');
		await search.fill('work log');
		await expect(cards(page)).toHaveText(['Google Sheets daily work log']);
		await search.fill('mail');
		await expect(cards(page).first()).toHaveText('Gmail compose an email and send it only after confirmation');
		await search.fill('zzzz nothing matches');
		await expect(page.locator('.empty-state h2')).toHaveText('No matching memories');
		await page.getByRole('button', { name: 'Clear search and filters' }).click();
		await expect(cards(page)).toHaveCount(catalog.length);

		// Suggestions, product, task and source filters combine.
		await page.getByRole('button', { name: 'meeting notes' }).click();
		await expect(cards(page).first()).toHaveText('Google Docs meeting notes from a template');
		await search.fill('');
		await page.getByRole('button', { name: /^Google Sheets/ }).click();
		await expect(cards(page)).toHaveCount(2);
		await expect(page.getByRole('button', { name: /^Google Sheets/ })).toHaveAttribute('aria-pressed', 'true');
		await page.getByRole('button', { name: 'Research' }).click();
		await expect(page.locator('.empty-state h2')).toHaveText('No matching memories');
		await page.getByRole('button', { name: 'Clear search and filters' }).click();
		await page.getByRole('button', { name: 'Contributed' }).click();
		await expect(cards(page)).toHaveText(['Save a clean draft']);
		await page.getByRole('button', { name: 'Built-in examples' }).click();
		await expect(cards(page)).toHaveCount(seed.length);
		expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
		await page.screenshot({ path: testInfo.outputPath(`memories-${width}.png`) });
		const library = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
		expect(library.violations.map((violation) => `${violation.id}: ${violation.nodes.map((node) => node.target.join(' ')).join(', ')}`)).toEqual([]);

		// A detail page reads level 1 and level 2 the way the API sends them.
		await search.fill('work log');
		await cards(page).first().click();
		await expect(page).toHaveURL(/\/memories\/web%3Aexample-sheets-work-log$/);
		await expect(page.locator('.detail-main h1')).toHaveText('Google Sheets daily work log');
		await expect(page.locator('.example-note')).toContainText('not verified site knowledge');
		await expect(page.locator('.point-list li')).toHaveCount(5);
		await expect(page.locator('.pitfalls code')).toHaveText(['Formula without equals', 'Dialog over grid', 'Date locale swap']);
		await expect(page.locator('.step-list li')).toHaveCount(5);
		await expect(page.locator('.step-list li').first()).toHaveText('Open a new spreadsheet and dismiss any dialog over the grid.');
		await expect(page.locator('.trail span').first()).toHaveText('Open · New spreadsheet');
		await expect(page.locator('.notes')).toContainText('Observed on 2026-10-30');
		await expect(page.locator('.detail-aside')).toContainText('Written');
		await expect(page.locator('.detail-aside pre')).toHaveText('wam recall https://docs.google.com/spreadsheets/ --task compose-draft --json');
		await page.getByRole('button', { name: 'Copy command' }).click();
		await expect(page.getByRole('button', { name: /Copied|Copy command/ })).toBeVisible();
		expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
		const detail = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
		expect(detail.violations.map((violation) => `${violation.id}: ${violation.nodes.map((node) => node.target.join(' ')).join(', ')}`)).toEqual([]);
		await page.screenshot({ path: testInfo.outputPath(`memory-detail-${width}.png`), fullPage: true });
	});
}

test('an unknown memory and a failing catalog show a clear message instead of a blank page', async ({ page }) => {
	await mock(page);
	await page.goto('/memories/web%3Aexample-does-not-exist');
	await expect(page.locator('.notice-box')).toContainText('Memory unavailable');
	await page.unroute('**/v1/memories');
	await page.route('**/v1/memories', (route) => route.fulfill({ status: 503, json: { error: 'unavailable' } }));
	await page.goto('/memories');
	await expect(page.locator('.notice-box')).toBeVisible();
});
