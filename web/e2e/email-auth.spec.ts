import { expect, test, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

async function mockEmailApi(page: Page, options: { failDelivery?: boolean; cooldown?: number; github?: boolean } = {}) {
  let signedIn = false;
  let codeSent = false;
  let sends = 0;
  await page.route('**/v1/**', async route => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    if (path.includes('/telemetry/')) return route.fulfill({ status: 202 });
    if (path === '/v1/auth/config') return route.fulfill({ json: { github_oauth_enabled: options.github ?? true, email_auth_enabled: true, mcp_oauth_enabled: true } });
    const session = { account_id: 'email_fixture', csrf_token: 'fixture-csrf', expires_at: Math.floor(Date.now() / 1000) + 43200 };
    if (path === '/v1/auth/session') return signedIn ? route.fulfill({ json: session }) : route.fulfill({ status: 401, json: { error: 'authentication_required' } });
    if (path === '/v1/auth/email/start') {
      expect(request.postDataJSON()).toEqual({ email: 'person@example.com' });
      sends++;
      if (options.failDelivery) return route.fulfill({ status: 503, json: { error: 'service_unavailable' } });
      codeSent = true;
      return route.fulfill({ status: 202, json: { sent: true, expires_in: 600, retry_after: options.cooldown ?? 60 } });
    }
    if (path === '/v1/auth/email/verify') {
      if (!codeSent || request.postDataJSON().code !== '012345') return route.fulfill({ status: 400, json: { error: 'invalid_authorization' } });
      signedIn = true;
      return route.fulfill({ json: session });
    }
    if (path === '/v1/auth/logout') {
      expect(request.headers()['x-csrf-token']).toBe('fixture-csrf');
      signedIn = false;
      return route.fulfill({ status: 204 });
    }
    if (signedIn && path === '/v1/me/dashboard') return route.fulfill({ json: { schema_version: 1, total_points: 0, overall_rank: null, public_rank: null, published_memories: 0, pending_contributions: 0, action_required_contributions: 0 } });
    if (signedIn && path === '/v1/me/contributions') return route.fulfill({ json: { schema_version: 1, contributions: [], next_cursor: null, total_count: 0 } });
    if (signedIn && path.startsWith('/v1/me/memories/analytics')) return route.fulfill({ json: { schema_version: 1, from_date: '2026-10-01', to_date: '2026-10-31', bucket: 'day', memories: [] } });
    if (signedIn && path === '/v1/me/score-events') return route.fulfill({ json: { schema_version: 1, events: [], next_cursor: null } });
    if (signedIn && path === '/v1/me/usage') return route.fulfill({ json: { schema_version: 1, period_days: 30, limit_per_minute: 60, burst_capacity: 30, remaining_units: 30, resets_in_seconds: 0, daily: [] } });
    return route.fulfill({ status: 401, json: { error: 'authentication_required' } });
  });
  return { sends: () => sends };
}

async function requestCode(page: Page) {
  await page.goto('/login');
  await page.getByLabel('Email address', { exact: true }).fill('person@example.com');
  await page.getByRole('button', { name: 'Send sign-in code', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Check your email.' })).toBeVisible();
}

test('email sign-in handles a wrong code, leading zero, CSRF logout, and responsive accessibility', async ({ page }, testInfo) => {
  const mock = await mockEmailApi(page);
  await requestCode(page);
  await expect(page.getByRole('link', { name: 'Connect GitHub account' })).toBeVisible();
  await expect(page.getByRole('button', { name: /Resend code in/ })).toBeDisabled();
  await page.getByLabel('Verification code', { exact: true }).fill('999999');
  await page.getByRole('button', { name: 'Verify and sign in', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('invalid or has expired');
  const report = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa']).analyze();
  expect(report.violations.map(({ id }) => id)).toEqual([]);
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.evaluate(() => window.scrollTo(0, 0));
  await expect.poll(() => page.evaluate(() => scrollY)).toBe(0);
  await page.screenshot({ path: testInfo.outputPath('email-code.png'), fullPage: true });
  await page.getByLabel('Verification code', { exact: true }).fill('012345');
  await page.getByRole('button', { name: 'Verify and sign in', exact: true }).click();
  await expect(page).toHaveURL(/\/dashboard$/);
  if (testInfo.project.name === 'mobile-chrome') await page.getByRole('button', { name: 'Open menu' }).click();
  else await page.getByLabel('Account menu').click();
  await page.getByRole('button', { name: 'Sign out', exact: true }).click();
  await expect(page.getByLabel('Email address', { exact: true })).toBeVisible();
  expect(mock.sends()).toBe(1);
});

test('resend waits for countdown and changing address clears the code', async ({ page }) => {
  await page.clock.install();
  const mock = await mockEmailApi(page, { cooldown: 2, github: false });
  await requestCode(page);
  await expect(page.getByRole('link', { name: 'Connect GitHub account' })).toHaveCount(0);
  await page.getByLabel('Verification code', { exact: true }).fill('123456');
  await page.clock.fastForward(3000);
  await page.getByRole('button', { name: 'Resend code', exact: true }).click();
  await expect(page.getByLabel('Verification code', { exact: true })).toHaveValue('');
  expect(mock.sends()).toBe(2);
  await page.getByRole('button', { name: 'Use a different email', exact: true }).click();
  await expect(page.getByLabel('Email address', { exact: true })).toBeVisible();
});

test('delivery failure stays on email input with a retryable error', async ({ page }) => {
  await mockEmailApi(page, { failDelivery: true });
  await page.goto('/login');
  await page.getByLabel('Email address', { exact: true }).fill('person@example.com');
  await page.getByRole('button', { name: 'Send sign-in code', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('Could not send your code');
  await expect(page.getByRole('button', { name: 'Send sign-in code', exact: true })).toBeEnabled();
  await expect(page.getByLabel('Verification code', { exact: true })).toHaveCount(0);
});

test('email login explains first-time account creation and mailbox recovery', async ({ page }) => {
  await mockEmailApi(page, { github: false });
  await page.goto('/login');
  await expect(page.getByText('New to WAM? Your first verified code creates an account. Returning members use the same email address to sign in.')).toBeVisible();
  await page.getByLabel('Email address', { exact: true }).fill('person@example.com');
  await page.getByRole('button', { name: 'Send sign-in code', exact: true }).click();
  await expect(page.locator('#code-help')).toContainText('spam');
  await expect(page.locator('#code-help')).toContainText('email provider');
  await expect(page.locator('#code-help')).toContainText('cannot recover your mailbox or change the account email');
});

test('contribution page offers WAM email sign-in and keeps any demo affordance local', async ({ page }) => {
  await mockEmailApi(page, { github: false });
  await page.goto('/contribute');
  await expect(page.getByRole('link', { name: 'Sign in with email' })).toBeVisible();
  await expect(page.getByText(/GitHub session|submit locally/i)).toHaveCount(0);
  await expect(page.locator('details.local-access')).toBeVisible();
  await expect(page.locator('details.local-access summary')).toHaveText('Local development access');
  await expect(page.locator('details.local-access')).not.toHaveAttribute('open', '');
});

test('profile settings offer WAM email sign-in and keep demo access local', async ({ page }) => {
  await mockEmailApi(page, { github: false });
  await page.goto('/settings/profile');
  await expect(page.getByRole('link', { name: 'Sign in with email' })).toBeVisible();
  await expect(page.getByText(/GitHub session|Signed in with GitHub/i)).toHaveCount(0);
  await expect(page.locator('details.local-access')).toBeVisible();
  await expect(page.locator('details.local-access')).not.toHaveAttribute('open', '');
});

test('email-only MCP approval links to sign-in with a bounded return request', async ({ page }) => {
  await mockEmailApi(page, { github: false });
  const request = 'a'.repeat(43);
  await page.goto('/oauth/approve?request=' + request);
  await expect(page.getByRole('link', { name: 'Sign in with email', exact: true })).toHaveAttribute('href', '/login?oauth_request=' + request);
  await page.getByRole('link', { name: 'Sign in with email', exact: true }).click();
  await expect(page.getByLabel('Email address', { exact: true })).toBeVisible();
  await page.getByLabel('Email address', { exact: true }).fill('person@example.com');
  await page.getByRole('button', { name: 'Send sign-in code', exact: true }).click();
  await page.getByLabel('Verification code', { exact: true }).fill('012345');
  await page.getByRole('button', { name: 'Verify and sign in', exact: true }).click();
  await expect(page).toHaveURL(/\/oauth\/approve\?request=a{43}$/);
});

test('email sign-in preserves the CLI device code without approving it', async ({ page }) => {
  await mockEmailApi(page, { github: false });
  let approvalCalls = 0;
  page.on('request', request => {
    if (new URL(request.url()).pathname === '/v1/link/approve') approvalCalls++;
  });

  await page.goto('/link?code=ABCD-EFGH');
  await expect(page.getByRole('heading', { name: 'Sign in before approving.' })).toBeVisible();
  await page.getByRole('link', { name: 'Go to sign in' }).click();
  await expect(page).toHaveURL(/\/login\?link_code=ABCD-EFGH$/);
  await page.getByLabel('Email address', { exact: true }).fill('person@example.com');
  await page.getByRole('button', { name: 'Send sign-in code', exact: true }).click();
  await page.getByLabel('Verification code', { exact: true }).fill('012345');
  await page.getByRole('button', { name: 'Verify and sign in', exact: true }).click();

  await expect(page).toHaveURL(/\/link\?code=ABCD-EFGH$/);
  await expect(page.getByLabel('Device code shown in the CLI')).toHaveValue('ABCD-EFGH');
  await expect(page.getByRole('button', { name: 'Review request', exact: true })).toBeEnabled();
  expect(approvalCalls).toBe(0);
});

test('GitHub sign-in request carries only the validated device code for its internal return', async ({ page }) => {
  await mockEmailApi(page);
  await page.goto('/login?link_code=ABCD-EFGH');
  const href = await page.getByRole('link', { name: 'Connect GitHub account', exact: true }).getAttribute('href');
  const target = new URL(href!, page.url());
  expect(target.pathname).toBe('/v1/auth/github/start');
  expect(target.searchParams.get('link_code')).toBe('ABCD-EFGH');
  expect(target.searchParams.get('oauth_request')).toBeNull();
});

test('malformed device return codes cannot redirect outside the WAM site', async ({ page }) => {
  await mockEmailApi(page, { github: false });
  await page.goto('/login?link_code=https%3A%2F%2Fevil.example');
  await page.getByLabel('Email address', { exact: true }).fill('person@example.com');
  await page.getByRole('button', { name: 'Send sign-in code', exact: true }).click();
  await page.getByLabel('Verification code', { exact: true }).fill('012345');
  await page.getByRole('button', { name: 'Verify and sign in', exact: true }).click();
  await expect(page).toHaveURL(/\/dashboard$/);
  expect(new URL(page.url()).hostname).toBe('127.0.0.1');
});
