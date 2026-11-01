import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';

const example = JSON.parse(readFileSync(new URL('../../crates/wam-server/seed/example_memories.json', import.meta.url), 'utf8'))[0];
const path = '/memories/' + encodeURIComponent(example.id);

async function mock(page: Page, author: string | null = 'wam_official') {
	await page.route('**/v1/auth/session', route => route.fulfill({ status: 401, json: { error: 'unauthorized' } }));
	await page.route('**/v1/auth/config', route => route.fulfill({ json: {} }));
	await page.route('**/v1/telemetry/**', route => route.fulfill({ status: 202, json: {} }));
	await page.route('**/v1/memories', route => route.fulfill({ json: { schema_version: 1, memories: [{
		memory_id: example.id, version_id: 'ver-example', title: example.payload.l1.title,
		summary: example.payload.l1.summary, site: 'docs.google.com', domain: 'docs.google.com',
		task_category: 'compose-draft', kind: 'workflow', status: 'active', verified_date: '1793491200',
		canonical_url_patterns: ['https://docs.google.com/document/**'], author_handle: author
	}] } }));
	await page.route('**/v1/memories/*?level=*', route => {
		const level = Number(new URL(route.request().url()).searchParams.get('level'));
		return route.fulfill({ json: { schema_version: 1, memory_id: example.id, version_id: 'ver-example', level,
			content: level === 1 ? example.payload.l1.summary : JSON.stringify(example.payload.l2) } });
	});
	await page.route('**/v1/leaderboard', route => route.fulfill({ json: { schema_version: 1, entries: [{ rank: 1, handle: 'wam_official', total_points: 20 }] } }));
	await page.route('**/v1/profiles/wam_official', route => route.fulfill({ json: {
		schema_version: 1, handle: 'wam_official', public_opt_in: true, service_links: [],
		bio: [{ type: 'paragraph', content: [{ type: 'text', text: 'Official WAM account publishing built-in example workflows.' }] }]
	} }));
}

for (const width of [1440, 390]) {
	test(`memory author links to the same public contributor as the leaderboard at ${width}px`, async ({ page }, testInfo) => {
		await mock(page);
		await page.setViewportSize({ width, height: 1000 });
		await page.goto(path);
		const author = page.locator('.detail-aside').getByRole('link', { name: '@wam_official' });
		await expect(author).toBeVisible();
		await expect(author).toHaveAttribute('href', '/u/wam_official');
		await expect(page.locator('.detail-aside')).toContainText('Created by');
		expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
		await page.screenshot({ path: testInfo.outputPath(`memory-author-${width}.png`), fullPage: true });
		await author.click();
		await expect(page.getByRole('heading', { name: '@wam_official', exact: true })).toBeVisible();
		await expect(page.locator('.public-profile-card')).toContainText('Official WAM account');
		await page.getByRole('link', { name: 'Back to leaderboard' }).click();
		await expect(page.locator('.leaderboard-table tbody tr')).toHaveCount(1);
		await expect(page.locator('.leaderboard-table')).toContainText('@wam_official');
		await expect(page.locator('.score')).toContainText('20');
		await expect(page.getByText('No public contributors yet.')).toHaveCount(0);
		await page.screenshot({ path: testInfo.outputPath(`leaderboard-${width}.png`), fullPage: true });
	});
}

test('a memory without a public author does not invent a creator or profile link', async ({ page }) => {
	await mock(page, null);
	await page.goto(path);
	await expect(page.locator('.detail-aside')).toContainText('Not publicly listed');
	await expect(page.locator('.detail-aside a[href^="/u/"]')).toHaveCount(0);
});
