import { expect, test } from '@playwright/test';

test.beforeEach(async ({ page }) => {
	await page.route('**/v1/auth/session', route => route.fulfill({ status: 401, json: { error: 'unauthorized' } }));
	await page.route('**/v1/telemetry/**', route => route.fulfill({ status: 202, json: {} }));
	await page.route('**/v1/auth/config', route => route.fulfill({ json: { github_oauth_enabled: false, mcp_oauth_enabled: false } }));
});

for (const language of ['en', 'ko']) {
	test(`${language} landing pulls the install commands out of the setup guide, Muse Code first`, async ({ page }) => {
		await page.goto(language === 'ko' ? '/ko' : '/');
		const quick = page.locator('.lp-quick');
		await expect(quick).toBeVisible();
		const tabs = quick.getByRole('group').getByRole('button');
		await expect(tabs).toHaveText(['Muse Code', 'Claude Code', 'Codex']);
		await expect(tabs.first()).toHaveAttribute('aria-pressed', 'true');
		await expect(quick.locator('.code-panel').first()).toContainText('curl -fsSL https://dev.meta.ai/install.sh | sh');
		await expect(quick.locator('.code-panel').nth(1)).toContainText('"url": "https://api.webagenticmemory.com/mcp"');
		await quick.getByRole('button', { name: 'Claude Code', exact: true }).click();
		await expect(quick.locator('.code-panel')).toContainText('claude mcp add --transport http --scope user web-memory https://api.webagenticmemory.com/mcp');
		await quick.getByRole('button', { name: 'Codex', exact: true }).click();
		await expect(quick.locator('.code-panel')).toContainText('codex mcp add web-memory --url https://api.webagenticmemory.com/mcp');
		await expect(quick.getByRole('link', { name: language === 'ko' ? /Windows, 계정 로그인/ : /Windows, account sign-in/ })).toHaveAttribute('href', language === 'ko' ? '/ko/guide#connect' : '/guide#connect');
		await expect(page.locator('.lp-hosts')).toContainText('Muse Code');
	});
	test(`${language} navigation fits tablet widths and keeps GitHub reachable`, async ({ page }) => {
		await page.goto(language === 'ko' ? '/ko' : '/');
		for (const width of [761, 800, 1000, 1024]) {
			await page.setViewportSize({ width, height: 1000 });
			expect(await page.evaluate(() => document.documentElement.scrollWidth), `page at ${width}px`).toBeLessThanOrEqual(width);
			const menu = page.locator('.mobile-menu-button');
			const mobile = await menu.isVisible();
			if (mobile) await menu.click();
			const name = mobile ? (language === 'ko' ? '모바일 메뉴' : 'Mobile navigation') : (language === 'ko' ? '주 메뉴' : 'Main navigation');
			const navigation = page.getByRole('navigation', { name, exact: true });
			await expect(navigation.getByRole('link', { name: 'GitHub', exact: true })).toBeVisible();
			if (mobile) await menu.click();
		}
	});
	for (const width of [1440, 390, 320]) {
		test(`${language} landing at ${width}px explains first use, shows a hint and answers questions`, async ({ page }, testInfo) => {
			await page.setViewportSize({ width, height: 1000 });
			await page.goto(language === 'ko' ? '/ko' : '/');
			const repository = 'https://github.com/heycodingkr/web-agentic-memory';
			const githubName = language === 'ko' ? 'GitHub에서 보기' : 'View on GitHub';
			const heroGithub = page.locator('.lp-hero').getByRole('link', { name: githubName, exact: true });
			await expect(heroGithub).toBeVisible();
			await expect(heroGithub).toHaveAttribute('href', repository);
			await expect(heroGithub).toHaveAttribute('target', '_blank');
			await expect(page.locator('footer').getByRole('link', { name: 'GitHub', exact: true })).toHaveAttribute('href', repository);
			if (width <= 760) {
				await page.getByRole('button', { name: language === 'ko' ? '메뉴 열기' : 'Open menu', exact: true }).click();
				const github = page.getByRole('navigation', { name: language === 'ko' ? '모바일 메뉴' : 'Mobile navigation', exact: true }).getByRole('link', { name: 'GitHub', exact: true });
				await expect(github).toBeVisible();
				await expect(github).toHaveAttribute('href', repository);
				await page.getByRole('button', { name: language === 'ko' ? '메뉴 닫기' : 'Close menu', exact: true }).click();
			} else {
				await expect(page.getByRole('navigation', { name: language === 'ko' ? '주 메뉴' : 'Main navigation', exact: true }).getByRole('link', { name: 'GitHub', exact: true })).toHaveAttribute('href', repository);
			}
			await expect(page.locator('.lp-lede')).toContainText('MCP');
			await expect(page.locator('.lp-lede')).toContainText('WAM CLI');
				await expect(page.locator('.lp-hosts')).toContainText('Codex');
			await expect(page.locator('.lp-availability')).toContainText(language === 'ko' ? '계정 없이' : 'no account');
			await page.screenshot({ path: testInfo.outputPath(`landing-${language}-${width}-hero.png`) });
			expect(await page.locator('.lp > section[id]').evaluateAll(sections => sections.map(section => section.id)))
				.toEqual(['discover', 'how', 'workflows', 'example', 'productivity-benchmarks', 'results', 'contribute', 'faq']);
			// One hero clip plus one per workflow card: no clip is shown twice (the old separate demo section repeated Slides and Blog).
			await expect(page.locator('.lp video')).toHaveCount(7);
			await expect(page.locator('.lp-how > li')).toHaveCount(3);
			await expect(page.locator('#example .lp-hint .pill')).toHaveText(language === 'ko' ? '설명용 예시' : 'Illustrative example');
			await expect(page.locator('#example .lp-context')).toContainText('https://example.com/guide/editor');
			await expect(page.locator('.lp-example-limits')).toContainText(language === 'ko' ? '힌트당 최대 400 추정 토큰' : 'each at most 400 estimated tokens');
			await expect(page.locator('#results .lp-tile').last()).toContainText(language === 'ko' ? '힌트당 추정 토큰' : 'estimated tokens per hint');
			await expect(page.locator('.lp-result-scope')).toContainText(language === 'ko' ? '조건당 1회' : 'one run per condition');
			await expect(page.locator('.lp')).not.toContainText('1,000');

			const benchmarks = page.locator('#productivity-benchmarks');
			await expect(benchmarks).toContainText('R8');
			await expect(benchmarks).toContainText(language === 'ko' ? 'Notion은 메모리 재측정 전이라 표에서 제외했습니다' : 'Notion is omitted while its memory is remeasured');
			await expect(benchmarks.locator('[data-service="notion"]')).toHaveCount(0);
			await expect(benchmarks).not.toContainText('375.414');
			await expect(benchmarks).not.toContainText('406.530');
			await expect(benchmarks.locator('[data-service="google-calendar"]')).toContainText('431.108');
			await expect(benchmarks.locator('[data-service="google-calendar"]')).toContainText('237.169');
			await expect(benchmarks.locator('a[href="https://calendar.google.com/"]')).toBeVisible();
			await expect(benchmarks.locator('a[href="https://www.notion.so/"]')).toHaveCount(0);
			await expect(benchmarks.locator('a[href="https://www.canva.com/"]')).toBeVisible();
			await expect(page.locator('a[href="https://docs.google.com/presentation/"]')).toBeVisible();
			await expect(page.locator('#results .lp-row')).toHaveCount(language === 'ko' ? 4 : 3);
			if (language === 'en') {
				await expect(page.locator('.lp')).not.toContainText(/Naver|네이버/i);
			} else {
				await expect(page.locator('#results a[href="https://blog.naver.com/"]')).toBeVisible();
			}
			await expect(benchmarks.locator('.lp-productivity-list > li')).toHaveCount(2);
				await expect(benchmarks.locator('.lp-productivity-grid')).toHaveCount(0);
				await expect(benchmarks.locator('.lp-productivity-comparison')).toHaveCount(2);
				const changes = benchmarks.locator('[data-service="google-calendar"]').locator(width <= 520 ? '.lp-mobile-delta' : '.lp-delta-column .lp-productivity-delta');
				await expect(changes.first()).toBeVisible();
				await expect(changes.first()).toContainText('−45%');
			for (const service of ['google-calendar', 'notion', 'canva'] as const) {
				const link = benchmarks.locator('a[href="/benchmarks/' + service + '.md"]');
				if (service === 'notion') await expect(link).toHaveCount(0);
				else await expect(link).toBeVisible();
				const report = await page.request.get('/benchmarks/' + service + '.md');
				expect(report.status()).toBe(200);
				expect(report.headers()['content-type']).toMatch(/text\/(plain|markdown)/);
				expect(await report.text()).toContain('# ' + ({'google-calendar': 'Google Calendar', notion: 'Notion', canva: 'Canva'}[service]));
			}
			// A single-task observation sits apart from the two comparison rows and never counts as one of them.
			const trip = benchmarks.locator('[data-service="trip-flights"]');
			await expect(trip).toBeVisible();
			await expect(trip).toContainText(language === 'ko' ? '예비 관찰' : 'Preliminary observation');
			await expect(trip.locator('.lp-prelim-runs > li')).toHaveCount(3);
			await expect(trip.locator('[data-run="b1-T"]')).toContainText('2,170,689');
			await expect(trip.locator('[data-run="b2-M"]')).toContainText(language === 'ko' ? '통과' : 'Passed');
			await expect(trip.locator('a[href="https://www.trip.com/flights/"]')).toBeVisible();
			await expect(trip.locator('a[href="/benchmarks/trip-flights.md"]')).toBeVisible();
			const tripReport = await page.request.get('/benchmarks/trip-flights.md');
			expect(tripReport.status()).toBe(200);
			expect(await tripReport.text()).toContain('# Trip.com flight search');
			await benchmarks.screenshot({ path: testInfo.outputPath('benchmarks-' + language + '-' + width + '.png'), style: 'header:not(.lp header), .skip-link { visibility: hidden !important; }' });

			// Workflows: six cards, each with a click-to-play clip in this language; docs and blog can swap to a side-by-side clip.
			const suffix = language === 'ko' ? '-ko' : '';
			const ids = ['slides', 'docs', 'blog', 'sheets', 'email', 'forms'];
			await expect(page.locator('#workflows h2')).toHaveText(language === 'ko' ? 'Web Agentic Memory로 어떤 일을 할 수 있을까요?' : 'What can you do with Web Agentic Memory?');
			await expect(page.locator('#workflows .lp-wf')).toHaveCount(6);
			expect(await page.locator('#workflows .lp-wf').evaluateAll(cards => cards.map(card => (card as HTMLElement).dataset.workflow))).toEqual(ids);
			await expect(page.locator('#workflows .lp-wf[data-workflow="docs"]')).toContainText(language === 'ko' ? '문서' : 'Documents');
			await expect(page.locator('#workflows .lp-wf-hints li')).toHaveCount(18);
			const videos = page.locator('#workflows video');
			for (let i = 0; i < ids.length; i++) {
				// A clip fetches nothing until it is near the screen, and never plays by itself.
				await videos.nth(i).scrollIntoViewIfNeeded();
				await expect(videos.nth(i)).toHaveAttribute('preload', 'metadata');
				expect(await videos.nth(i).evaluate(video => [video.getAttribute('poster'), video.querySelector('source')?.getAttribute('src'), video.hasAttribute('autoplay')])).toEqual([`/media/demo-${ids[i]}${suffix}-poster.jpg`, `/media/demo-${ids[i]}${suffix}.mp4`, false]);
			}
			for (const url of [...ids.flatMap(id => [`/media/demo-${id}${suffix}-poster.jpg`, `/media/demo-${id}${suffix}.mp4`]), `/media/demo-docs-compare${suffix}.mp4`, `/media/demo-compare${suffix}.mp4`]) {
				expect((await page.request.head(url)).ok(), `${url} is served`).toBe(true);
			}
			await expect(page.locator('#workflows .lp-wf-compare')).toHaveCount(2);
			for (const [id, compare] of [['docs', 'demo-docs-compare'], ['blog', 'demo-compare']]) {
				const card = page.locator(`#workflows .lp-wf[data-workflow="${id}"]`);
				await card.locator('.lp-wf-compare').click();
				await expect(card.locator('video source')).toHaveAttribute('src', `/media/${compare}${suffix}.mp4`);
				await expect(card.locator('video')).toHaveJSProperty('autoplay', true);
				await card.locator('.lp-wf-compare').click();
				await expect(card.locator('video source')).toHaveAttribute('src', `/media/demo-${id}${suffix}.mp4`);
			}
			await expect(page.locator('#workflows .lp-chips')).toHaveCount(1);
			await expect(page.locator('#workflows a[href="/memories"]')).toHaveText(language === 'ko' ? '메모리 라이브러리 열기 →' : 'Open the memory library →');
			await page.locator('#workflows').screenshot({ path: testInfo.outputPath(`landing-${language}-${width}-workflows.png`) });
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
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
			await page.locator(`#how a[href="${language === 'ko' ? '/ko/guide' : '/guide'}"]`).click();
			await expect(page).toHaveURL(language === 'ko' ? /\/ko\/guide$/ : /\/guide$/);
		});
	}
}
