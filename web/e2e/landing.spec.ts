import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
	await page.route('**/v1/auth/session', route => route.fulfill({ status: 401, json: { error: 'unauthorized' } }));
	await page.route('**/v1/telemetry/**', route => route.fulfill({ status: 202, json: {} }));
	await page.route('**/v1/auth/config', route => route.fulfill({ json: { github_oauth_enabled: false, mcp_oauth_enabled: false } }));
});

for (const language of ['en', 'ko']) {
	for (const width of [1440, 390, 320]) {
		test(`${language} landing at ${width}px explains first use, shows a hint and answers questions`, async ({ page }, testInfo) => {
			await page.setViewportSize({ width, height: 1000 });
			await page.goto(language === 'ko' ? '/ko' : '/');
			await expect(page.locator('.lp-lede')).toContainText('MCP');
			await expect(page.locator('.lp-lede')).toContainText('Codex');
			await expect(page.locator('.lp-availability')).toContainText(language === 'ko' ? '계정 없이' : 'no account');
			expect(await page.locator('.lp > section[id]').evaluateAll(sections => sections.map(section => section.id)))
				.toEqual(['how', 'example', 'demo', 'productivity-benchmarks', 'results', 'contribute', 'faq']);
			await expect(page.locator('.lp-how > li')).toHaveCount(3);
			await expect(page.locator('#example .lp-hint .pill')).toHaveText(language === 'ko' ? '설명용 예시' : 'Illustrative example');
			await expect(page.locator('#example .lp-context')).toContainText('https://example.com/guide/editor');
			await expect(page.locator('.lp-example-limits')).toContainText(language === 'ko' ? '힌트당 최대 400 추정 토큰' : 'each at most 400 estimated tokens');
			await expect(page.locator('#results .lp-tile').last()).toContainText(language === 'ko' ? '힌트당 추정 토큰' : 'estimated tokens per hint');
			await expect(page.locator('.lp-result-scope')).toContainText(language === 'ko' ? '조건당 1회' : 'one run per condition');
			await expect(page.locator('.lp')).not.toContainText('1,000');

			const benchmarks = page.locator('#productivity-benchmarks');
			await expect(benchmarks).toContainText('R8');
			await expect(benchmarks).toContainText(language === 'ko' ? 'Notion 후보 두 개는 반영하지 않았습니다' : 'Notion candidates were rejected');
			await expect(benchmarks.locator('.lp-productivity-scope')).toContainText('R7');
			await expect(benchmarks.locator('.lp-productivity-card')).toHaveCount(3);
			for (const service of ['google-calendar', 'notion', 'canva'] as const) {
				const link = benchmarks.locator('a[href="/benchmarks/' + service + '.md"]');
				await expect(link).toBeVisible();
				const report = await page.request.get('/benchmarks/' + service + '.md');
				expect(report.status()).toBe(200);
				expect(report.headers()['content-type']).toMatch(/text\/(plain|markdown)/);
				expect(await report.text()).toContain('# ' + ({'google-calendar': 'Google Calendar', notion: 'Notion', canva: 'Canva'}[service]));
			}
			await benchmarks.screenshot({ path: testInfo.outputPath('benchmarks-' + language + '-' + width + '.png') });

			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
			await page.screenshot({ path: testInfo.outputPath(`landing-${language}-${width}-hero.png`) });
			await page.locator('#example').screenshot({ path: testInfo.outputPath(`landing-${language}-${width}-hint.png`) });

			const faq = page.locator('#faq details');
			await expect(faq).toHaveCount(5);
			await faq.nth(0).locator('summary').focus();
			await page.keyboard.press('Enter');
			await expect(faq.nth(0)).toHaveAttribute('open', '');
			await expect(faq.nth(0).locator('p')).toBeVisible();
			await faq.nth(1).locator('summary').click();
			await expect(faq.nth(1).locator('p')).toContainText(language === 'ko' ? '호출할 때' : 'calls a lookup tool');
			await faq.nth(4).locator('summary').click();
			await expect(faq.nth(4).locator('p')).toContainText(language === 'ko' ? 'WAM 요청에는' : 'WAM requests');
			await page.locator('#faq').screenshot({ path: testInfo.outputPath(`landing-${language}-${width}-faq.png`) });
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
			await page.locator('#how a[href="/guide"]').click();
			await expect(page).toHaveURL(/\/guide$/);
		});
	}
}
