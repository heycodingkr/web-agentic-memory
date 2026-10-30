import { expect, test } from '@playwright/test';
import { impact, mockDashboard } from './support/dashboard';

test('WEB-30: expired analytics authentication clears all private dashboard state', async ({ page }) => {
	let revoked = false;
	await mockDashboard(page, async (path, route) => {
		if (revoked && path === '/v1/me/memories/analytics') {
			await route.fulfill({ status: 401, json: { error: 'unauthorized' } });
			return true;
		}
		return false;
	});
	await page.goto('/dashboard');
	await expect(page.getByRole('heading', { name: 'Private authored workflow' })).toBeVisible();
	await expect(page.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '5');
	revoked = true;
	await page.getByRole('button', { name: 'Update', exact: true }).click();
	await expect(page.getByLabel('Local demo token', { exact: true })).toBeVisible();
	await expect(page.getByRole('alert')).toContainText('Sign in');
	await expect(page.getByLabel('My rank and points')).toHaveCount(0);
	await expect(page.getByRole('heading', { name: 'Your agent activity' })).toHaveCount(0);
	await expect(page.getByRole('heading', { name: 'Private authored workflow' })).toHaveCount(0);
	await expect(page.getByRole('heading', { name: 'My point history' })).toHaveCount(0);
});

test('WEB-31: replacing an in-flight analytics request resets busy and ignores its stale 401', async ({ page }) => {
	let analyticsCalls = 0;
	let release!: () => void;
	let began!: () => void;
	const blocked = new Promise<void>(resolve => { release = resolve; });
	const started = new Promise<void>(resolve => { began = resolve; });
	await mockDashboard(page, async (path, route) => {
		if (path !== '/v1/me/memories/analytics') return false;
		analyticsCalls++;
		if (analyticsCalls === 2) {
			began();
			await blocked;
			await route.fulfill({ status: 401, json: { error: 'unauthorized' } });
		} else {
			await route.fulfill({ json: impact(analyticsCalls >= 3 ? 'Current account workflow' : undefined) });
		}
		return true;
	});
	await page.goto('/dashboard');
	await expect(page.getByRole('heading', { name: 'Private authored workflow' })).toBeVisible();
	await page.getByRole('button', { name: 'Update', exact: true }).click();
	await started;
	await page.getByRole('button', { name: 'Refresh rank', exact: true }).click();
	await expect(page.getByRole('heading', { name: 'Current account workflow' })).toBeVisible();
	const oldResponse = page.waitForResponse(response => response.status() === 401 && response.url().includes('/memories/analytics'));
	release();
	await oldResponse;
	await expect(page.getByRole('button', { name: 'Update', exact: true })).toBeEnabled();
	await expect(page.getByRole('button', { name: 'Refresh rank', exact: true })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Current account workflow' })).toBeVisible();
});

for (const status of [429, 503]) {
	test(`WEB-27/32: dashboard ${status} stays an error and does not retry automatically`, async ({ page }) => {
		await page.clock.install();
		const calls = await mockDashboard(page, async (path, route) => {
			if (path !== '/v1/me/usage') return false;
			await route.fulfill({ status, headers: { 'Retry-After': '12' }, json: { error: status === 429 ? 'rate_limited' : 'unavailable' } });
			return true;
		});
		await page.goto('/dashboard');
		const alert = page.getByRole('alert');
		await expect(alert).toContainText(status === 429 ? 'Try again in 12 seconds' : 'temporarily unavailable');
		await expect(page.getByLabel('My rank and points')).toHaveCount(0);
		await expect(page.getByRole('progressbar')).toHaveCount(0);
		const accountCalls = () => calls.filter(path => path.startsWith('/v1/me/')).length;
		const before = accountCalls();
		await page.clock.runFor(15_000);
		expect(accountCalls()).toBe(before);
		await expect(page.getByRole('button', { name: 'Refresh rank', exact: true })).toBeEnabled();
	});
}
