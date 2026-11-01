import { expect, test, type Page, type Route } from '@playwright/test';

const devices = ['first', 'second'].map(name => ({
  id: 'device_' + name,
  name: name + ' CLI',
  scopes: ['read:memories'],
  created_at: 1_790_000_000,
  expires_at: 1_800_000_000,
  last_seen_at: null,
  revoked: false
}));
const shownToken = 'fixture-only-first-rotated-token';

async function mockDevices(page: Page, intercept: (path: string, route: Route) => Promise<boolean> = async () => false) {
  await page.route('**/v1/**', async route => {
    const path = new URL(route.request().url()).pathname;
    if (path.includes('/telemetry/')) return route.fulfill({ status: 202 });
    if (await intercept(path, route)) return;
    if (path === '/v1/auth/session') return route.fulfill({ json: {
      account_id: 'device-fixture', csrf_token: 'device-fixture-csrf', expires_at: 1_800_000_000
    } });
    if (path === '/v1/me/devices') return route.fulfill({ json: { devices } });
    if (path.endsWith('/rotate')) return route.fulfill({ json: { access_token: shownToken, device: devices[0] } });
    if (path.endsWith('/revoke')) return route.fulfill({ status: 204 });
    throw new Error('Unexpected device fixture request: ' + path);
  });
  await page.goto('/settings/devices');
  await expect(page.getByText('first CLI', { exact: true })).toBeVisible();
  await expect(page.getByText('second CLI', { exact: true })).toBeVisible();
}

function deviceRow(page: Page, name: string) {
  return page.locator('.device-row').filter({ has: page.getByText(name + ' CLI', { exact: true }) });
}

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>(done => { resolve = done; });
  return { promise, resolve };
}

test('device changes stay serialized through the response and list refresh', async ({ page }) => {
  const rotationStarted = deferred();
  const rotation = deferred();
  const refreshStarted = deferred();
  const refresh = deferred();
  let rotated = false;
  await mockDevices(page, async (path, route) => {
    if (path.endsWith('/rotate')) {
      rotationStarted.resolve();
      await rotation.promise;
      rotated = true;
      await route.fulfill({ json: { access_token: shownToken, device: devices[0] } });
      return true;
    }
    if (path === '/v1/me/devices' && rotated) {
      refreshStarted.resolve();
      await refresh.promise;
      await route.fulfill({ json: { devices } });
      return true;
    }
    return false;
  });
  try {
    await deviceRow(page, 'first').getByRole('button', { name: 'Rotate token' }).click();
    await rotationStarted.promise;
    await expect(deviceRow(page, 'second').getByRole('button', { name: 'Rotate token' })).toBeDisabled();
    await expect(deviceRow(page, 'second').getByRole('button', { name: 'Revoke' })).toBeDisabled();
    rotation.resolve();
    await refreshStarted.promise;
    await expect(page.getByLabel('New device token')).toHaveValue(shownToken);
    await expect(deviceRow(page, 'second').getByRole('button', { name: 'Rotate token' })).toBeDisabled();
    refresh.resolve();
    await expect(deviceRow(page, 'second').getByRole('button', { name: 'Rotate token' })).toBeEnabled();
    await expect(deviceRow(page, 'first').getByRole('button', { name: 'Rotate token' })).toBeEnabled();
  } finally {
    rotation.resolve();
    refresh.resolve();
  }
});

for (const operation of ['list', 'rotate', 'revoke'] as const) {
  test(`${operation} authentication expiry clears devices, the token, and the account menu`, async ({ page }) => {
    let expired = false;
    await mockDevices(page, async (path, route) => {
      if (expired && path.startsWith('/v1/me/devices')) {
        await route.fulfill({ status: 401, json: { error: 'authentication_required' } });
        return true;
      }
      if (operation === 'list' && path.endsWith('/rotate')) {
        expired = true;
        await route.fulfill({ json: { access_token: shownToken, device: devices[0] } });
        return true;
      }
      return false;
    });
    await deviceRow(page, 'first').getByRole('button', { name: 'Rotate token' }).click();
    if (operation !== 'list') {
      await expect(page.getByLabel('New device token')).toHaveValue(shownToken);
      await expect(deviceRow(page, 'second').getByRole('button', { name: 'Rotate token' })).toBeEnabled();
      expired = true;
      if (operation === 'revoke') page.once('dialog', dialog => dialog.accept());
      await deviceRow(page, 'second').getByRole('button', { name: operation === 'rotate' ? 'Rotate token' : 'Revoke' }).click();
    }
    await expect(page.getByRole('heading', { name: 'Sign in to continue.' })).toBeVisible();
    await expect(page.locator('.device-row')).toHaveCount(0);
    await expect(page.getByLabel('New device token')).toHaveCount(0);
    await expect(page.getByLabel('Account menu')).toHaveCount(0);
    await expect(page.getByRole('alert')).toContainText('Sign in');
    await expect(page.getByRole('button', { name: 'Try again' })).toHaveCount(0);
  });
}

for (const revoked of ['first', 'second']) {
  test(`successful revocation of ${revoked} device clears only its own displayed token`, async ({ page }) => {
    await mockDevices(page);
    await deviceRow(page, 'first').getByRole('button', { name: 'Rotate token' }).click();
    await expect(page.getByLabel('New device token')).toHaveValue(shownToken);
    await expect(deviceRow(page, 'second').getByRole('button', { name: 'Rotate token' })).toBeEnabled();
    page.once('dialog', dialog => dialog.accept());
    await deviceRow(page, revoked).getByRole('button', { name: 'Revoke' }).click();
    await expect(page.getByRole('status')).toContainText('Revoked the connection for ' + revoked + ' CLI.');
    if (revoked === 'first') await expect(page.getByLabel('New device token')).toHaveCount(0);
    else await expect(page.getByLabel('New device token')).toHaveValue(shownToken);
  });
}

for (const operation of ['rotate', 'revoke'] as const) {
  test(`failed ${operation} on another device preserves the previously shown token`, async ({ page }, testInfo) => {
    let fail = false;
    await mockDevices(page, async (path, route) => {
      if (fail && path.endsWith('/' + operation)) {
        await route.fulfill({ status: 503, json: { error: 'unavailable' } });
        return true;
      }
      return false;
    });
    await deviceRow(page, 'first').getByRole('button', { name: 'Rotate token' }).click();
    await expect(page.getByLabel('New device token')).toHaveValue(shownToken);
    await expect(deviceRow(page, 'second').getByRole('button', { name: 'Rotate token' })).toBeEnabled();
    fail = true;
    if (operation === 'revoke') page.once('dialog', dialog => dialog.accept());
    await deviceRow(page, 'second').getByRole('button', { name: operation === 'rotate' ? 'Rotate token' : 'Revoke' }).click();
    await expect(page.getByRole('alert')).toContainText('temporarily unavailable');
    await expect(page.getByLabel('New device token')).toHaveValue(shownToken);
    await expect(deviceRow(page, 'second').getByRole('button', { name: 'Rotate token' })).toBeEnabled();
    if (testInfo.project.name === 'mobile') {
      await page.getByRole('button', { name: 'Open menu' }).click();
      await expect(page.getByRole('button', { name: 'Sign out', exact: true })).toBeVisible();
    } else {
      await expect(page.getByLabel('Account menu')).toBeVisible();
    }
  });
}
