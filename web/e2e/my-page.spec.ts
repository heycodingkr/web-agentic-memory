import { expect, test } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

test('MYPAGE-35: analytics failure keeps valid points and contribution data', async ({ page }) => {
  await page.route('**/__preview/v1/me/memories/analytics**', route => route.fulfill({ status: 503, json: { error: 'service_unavailable' } }));
  await page.goto('/dashboard');
  await expect(page.getByLabel('My rank and points')).toContainText('246');
  await expect(page.getByRole('button', { name: /Format a long report in Google Docs/ })).toBeVisible();
  await expect(page.getByRole('alert')).toContainText('temporarily unavailable');
});

test('contributions can be filtered, searched, paged and inspected without inventing a publication', async ({ page }) => {
  await page.goto('/dashboard');
  const filters = page.getByLabel('Filter contributions');
  await filters.getByRole('button', { name: 'Pending 4', exact: true }).click();
  await expect(page.getByRole('button', { name: /Format a long report in Google Docs/ })).toHaveCount(0);
  await expect(page.getByRole('button', { name: /Clean duplicate rows/ })).toBeVisible();
  await page.getByLabel('Search my contributions').fill('Notion');
  await expect(page.getByRole('button', { name: /Clean duplicate rows/ })).toHaveCount(0);
  await page.getByRole('button', { name: 'View contribution sample-notion-03', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('Retry scheduled');
  await expect(dialog).toContainText('This is not an estimated completion time.');
  await expect(dialog.getByRole('link', { name: 'Open memory' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Close contribution details' }).click();
  await page.getByLabel('Search my contributions').fill('');
  await filters.getByRole('button', { name: 'All 9', exact: true }).click();
  await page.getByRole('button', { name: 'Next contributions page' }).click();
  await expect(page.getByRole('button', { name: /Build a monthly expense summary/ })).toBeVisible();
});

test('legacy approved and supported submissions are not displayed as new publications', async ({ page }) => {
  await page.route('**/__preview/v1/me/contributions', route => route.fulfill({ json: { schema_version: 1, contributions: [{ id: 'legacy-record', title: 'Legacy reviewed workflow', url_pattern: 'https://example.org/guide', task_category: 'navigate', actions: ['navigate'], status: 'approved', submitted_at: 1700000000 }] } }));
  await page.goto('/dashboard');
  await expect(page.getByText('Result unconfirmed', { exact: true })).toBeVisible();
  await expect(page.getByText('Publication not yet linked', { exact: true })).toBeVisible();
  await expect(page.getByRole('cell', { name: /^Submitted / })).toBeVisible();
  const filters = page.getByLabel('Filter contributions');
  await expect(filters.getByRole('button', { name: 'Published 0', exact: true })).toBeVisible();
});

test('verified result links to its memory version and explains a safe model-budget wait', async ({ page }) => {
  await page.goto('/dashboard');
  await page.getByRole('button', { name: 'View contribution sample-docs-01', exact: true }).click();
  const published = page.getByRole('dialog');
  await expect(published).toContainText('New version published');
  await expect(published).toContainText('Version v3');
  await expect(published.getByRole('link', { name: 'Open memory' })).toHaveAttribute('href', '/memories/sample-docs-report');
  await page.getByRole('button', { name: 'Close contribution details' }).click();
  await page.getByRole('button', { name: 'View contribution sample-notion-03', exact: true }).click();
  const waiting = page.getByRole('dialog');
  await expect(waiting).toContainText('worker’s shared model budget');
  await expect(waiting).toContainText('This is not an estimated completion time.');
});

test('My Page tabs, periods and point history work with real rendered fixture responses', async ({ page }) => {
  await page.goto('/dashboard');
  await page.getByRole('tab', { name: 'My memories', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'My memories', exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Build a monthly expense summary', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Memory impact', exact: true }).click();
  await page.getByLabel('Impact period').selectOption('7');
  await page.getByLabel('Group by', { exact: true }).selectOption('week');
  await expect(page.getByRole('button', { name: 'Update', exact: true })).toBeEnabled();
  await page.getByText('View chart data', { exact: true }).first().click();
  await expect(page.getByRole('table').first()).toContainText('Anonymous recall');
  await page.getByRole('tab', { name: 'Point history', exact: true }).click();
  await expect(page.getByText('Memory published', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('Credit reversed', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Load older entries', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Load older entries', exact: true })).toHaveCount(0);
  await expect(page.getByLabel('My rank and points')).toContainText('246');
});

test('empty and signed-out states distinguish an unused account from missing authentication', async ({ page }) => {
  await page.goto('/dashboard?preview=empty');
  await expect(page.getByRole('heading', { name: 'Your first contribution starts here.' })).toBeVisible();
  await expect(page.getByText('No rank yet', { exact: true })).toBeVisible();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await page.goto('/dashboard?preview=signed-out');
  await expect(page.getByRole('heading', { name: 'Your contributions belong here.' })).toBeVisible();
  await expect(page.getByLabel('My rank and points')).toHaveCount(0);
});

test('layout remains accessible and usable at desktop and mobile widths', async ({ page }, testInfo) => {
  await page.goto('/dashboard');
  await expect(page.getByLabel('My rank and points')).toContainText('246');
  await expect(page.getByRole('button', { name: /Format a long report in Google Docs/ })).toBeVisible();
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  for (const label of ['My contributions', 'My memories', 'Memory impact', 'Point history']) {
    await page.getByRole('tab', { name: new RegExp('^' + label) }).click();
    const report = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
    await testInfo.attach('axe-' + label.replaceAll(' ', '-'), { body: JSON.stringify(report.violations), contentType: 'application/json' });
    expect(report.violations.map(item => ({ id: item.id, targets: item.nodes.map(node => node.target) }))).toEqual([]);
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
  await page.getByRole('tab', { name: /My contributions/ }).click();
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({ path: testInfo.outputPath('my-page.png'), fullPage: true });
  const first = page.getByRole('tab', { name: /My contributions/ });
  await first.focus(); await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('tab', { name: 'My memories', exact: true })).toBeFocused();
  await expect(page.getByRole('tab', { name: 'My memories', exact: true })).toHaveAttribute('aria-selected', 'true');
});
