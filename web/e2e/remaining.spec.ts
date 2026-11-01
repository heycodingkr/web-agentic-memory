import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

test.beforeEach(async ({ page }) => {
    await page.route('**/v1/auth/session', route => route.fulfill({
        json: { account_id: 'navigation-qa', csrf_token: 'navigation-qa' }
    }));
    await page.route('**/v1/telemetry/**', route => route.fulfill({ status: 202, json: {} }));
});

for (const path of ['/', '/ko']) {
    test(`${path} signed-in header keeps every label on one line`, async ({ page }) => {
        await page.goto(path);
        await page.evaluate(() => document.fonts.ready);
        await expect(page.locator('.account-menu summary')).toBeVisible();
        for (const width of [1024, 1100, 1200, 1280, 1440]) {
            await page.setViewportSize({ width, height: 1000 });
            const header = page.locator('.site-header');
            const menu = header.locator('.mobile-menu-button');
            if (await menu.isVisible()) {
                await menu.click();
                await expect(page.locator('.mobile-nav a[href="/dashboard"]')).toBeVisible();
                await menu.click();
            } else {
                const wrapped = await header.locator('.desktop-nav a, .nav-actions a, .account-menu summary').evaluateAll(elements =>
                    elements.filter(element => {
                        const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
                        const tops: number[] = [];
                        while (walker.nextNode()) {
                            const text = walker.currentNode;
                            if (!text.textContent?.trim() || text.parentElement?.closest('[aria-hidden="true"]')) continue;
                            const range = document.createRange();
                            range.selectNodeContents(text);
                            tops.push(...[...range.getClientRects()].filter(rect => rect.width && rect.height).map(rect => Math.round(rect.top)));
                        }
                        return new Set(tops).size > 1;
                    }).map(element => element.textContent?.trim())
                );
                expect(wrapped, `wrapped labels at ${width}px`).toEqual([]);
                await expect(header.locator('.desktop-nav a[href="/dashboard"]')).toBeVisible();
            }
            expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
        }
    });

    for (const width of [1440, 390]) {
        test(`${path} landing text meets WCAG AA at ${width}px`, async ({ page }) => {
            await page.setViewportSize({ width, height: 1000 });
            await page.goto(path);
            const result = await new AxeBuilder({ page })
                .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
                .analyze();
            expect(result.violations.map(violation => ({ id: violation.id, targets: violation.nodes.map(node => node.target) }))).toEqual([]);
        });
    }
}
