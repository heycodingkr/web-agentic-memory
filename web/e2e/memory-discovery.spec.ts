import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { readFileSync } from 'node:fs';

type Seed = { id: string; payload: { l0: { site_host: string; task_category: string; url_patterns: string[] }; l1: { title: string; summary: string } } };
const seed: Seed[] = ['example_memories.json', 'design_memories.json'].flatMap(file => JSON.parse(readFileSync(new URL('../../crates/wam-server/seed/' + file, import.meta.url), 'utf8')));
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

test('discovery waits for the client before accepting a search', async ({ page }) => {
	let releaseScripts!: () => void;
	const scripts = new Promise<void>(resolve => { releaseScripts = resolve; });
	await page.route('**/*', async route => {
		if (route.request().resourceType() === 'script') await scripts;
		await route.fallback();
	});
	try {
		await page.goto('/ko', { waitUntil: 'commit' });
		const discovery = page.locator('#discover');
		await expect(discovery.getByRole('searchbox')).toBeDisabled();
		await expect(discovery.getByRole('button', { name: '메모리 찾기' })).toBeDisabled();
		releaseScripts();
		await expect(discovery.getByRole('searchbox')).toBeEnabled();
		await discovery.getByRole('searchbox').fill('docs.google.com/spreadsheets');
		await discovery.getByRole('button', { name: '메모리 찾기' }).click();
		await expect(page.getByRole('searchbox')).toHaveValue('docs.google.com/spreadsheets');
		await expect(page.locator('.memory-card')).toHaveCount(2);
	} finally {
		releaseScripts();
	}
});

test('site shortcuts and English landing support discovery without signing in', async ({ page }) => {
	await page.goto('/');
	const discovery = page.locator('#discover');
	await expect(discovery.getByRole('button', { name: 'Find workflows' })).toBeVisible();
	await discovery.getByRole('link', { name: 'Google Docs', exact: true }).click();
	await expect(page.locator('.memory-card')).toHaveCount(memories.filter(memory => memory.canonical_url_patterns.some(pattern => pattern.includes('/document/'))).length);
	await expect(page.locator('.memory-card .card-product')).not.toContainText(['Google Sheets']);
});

for (const width of [1440, 390]) {
	test(`workflow categories at ${width}px keep URL search and narrow products`, async ({ page }, testInfo) => {
		const figmaCount = memories.filter(memory => memory.domain === 'www.figma.com').length;
		const stitchCount = memories.filter(memory => memory.domain === 'stitch.withgoogle.com').length;
		await page.setViewportSize({ width, height: 1000 });
		await page.goto('/memories');
		const categories = page.getByRole('group', { name: 'Explore by workflow' });
		await categories.getByRole('button', { name: /Design & media/ }).click();
		await expect(page.locator('.memory-card').filter({ hasText: 'responsive web and mobile frames' })).toBeVisible();
		await expect(page.locator('.memory-card').filter({ hasText: 'mobile app screen' })).toBeVisible();
		const products = page.getByRole('group', { name: 'Filter by product' });
		await expect(products.getByRole('button', { name: /Google Docs/ })).toHaveCount(0);
		await products.getByRole('button', { name: /Figma/ }).click();
		await expect(page.locator('.memory-card')).toHaveCount(figmaCount);

		await page.getByRole('searchbox').fill('https://stitch.withgoogle.com/projects/private-project?token=private');
		await categories.getByRole('button', { name: /Design & media/ }).click();
		await expect(page.getByRole('searchbox')).toHaveValue(/stitch.withgoogle.com/);
		await expect(page.locator('.memory-card')).toHaveCount(stitchCount);
		await expect(page.locator('.memory-card').filter({ hasText: 'mobile app screen' })).toBeVisible();
		await categories.getByRole('button', { name: /Documents & writing/ }).click();
		await expect(page.getByRole('heading', { name: 'No matching memories' })).toBeVisible();
		await page.getByRole('button', { name: 'Clear search and filters' }).click();
		await expect(page.locator('.memory-card')).toHaveCount(memories.length);
		await expect(categories.getByRole('button', { name: /All workflows/ })).toHaveAttribute('aria-pressed', 'true');
		await categories.scrollIntoViewIfNeeded();
		await page.screenshot({ path: testInfo.outputPath(`workflow-categories-${width}.png`), fullPage: true });
		expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
		const audit = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
		expect(audit.violations.map(issue => issue.id)).toEqual([]);
	});
}

test('design shortcuts expose Figma and Stitch workflows without sending private file URLs', async ({ page }) => {
 const requests:string[]=[];
 page.on('request',request=>requests.push(request.url()));
 await page.goto('/ko');
 await page.locator('#discover').getByRole('link',{name:'Google Stitch',exact:true}).click();
 await expect(page.locator('.memory-card').filter({hasText:'mobile app screen'})).toBeVisible();
 await page.goto('/memories#q='+encodeURIComponent('https://www.figma.com/design/private-file-sentinel'));
 await expect(page.locator('.memory-card').filter({hasText:'responsive web and mobile frames'})).toBeVisible();
 expect(requests.some(url=>url.includes('private-file-sentinel'))).toBe(false);
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
