import { expect, test } from '@playwright/test';
import { createHash } from 'node:crypto';

const origin = process.env.WAM_OAUTH_WEB_ORIGIN || 'http://127.0.0.1:32055';
const resource = origin + '/mcp';
const apiOrigin = process.env.WAM_OAUTH_API_ORIGIN || 'http://127.0.0.1:35055';
const verifier = 'dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk';
const challenge = createHash('sha256').update(verifier).digest('base64url');

test('authorization cannot be approved inside a frame', async ({ page }) => {
    await page.route('**/frame-host', route => route.fulfill({
        contentType: 'text/html',
        body: '<iframe title="embedded approval" src="/oauth/approve?request=' + 'A'.repeat(43) + '"></iframe>'
    }));
    await page.goto('/frame-host');
    const frame = page.frameLocator('iframe');
    await expect(frame.getByRole('alert')).toHaveText('Open this authorization page in its own browser window.');
    await expect(frame.getByRole('button', { name: 'Allow access' })).toHaveCount(0);
});

test('remote OAuth asks for consent after login, redeems PKCE, and revokes from device settings', async ({ page }) => {
    const request = page.request;
    const appName = 'WAM browser fixture ' + Date.now().toString(36);
    const metadata = await request.get('/.well-known/oauth-authorization-server');
    expect(metadata.ok()).toBeTruthy();
    expect((await metadata.json()).issuer).toBe(origin);
    const registered = await request.post('/oauth/register', { data: {
        client_name: appName,
        redirect_uris: [origin + '/oauth-result'],
        token_endpoint_auth_method: 'none',
        grant_types: ['authorization_code', 'refresh_token']
    } });
    expect(registered.status()).toBe(201);
    const client = await registered.json();
    const authorize = new URL(origin + '/oauth/authorize');
    for (const [key, value] of Object.entries({ client_id: client.client_id, redirect_uri: origin + '/oauth-result',
        response_type: 'code', code_challenge: challenge, code_challenge_method: 'S256',
        resource, scope: 'read:memories read:dashboard', state: 'browser-fixture-state' })) {
        authorize.searchParams.set(key, value);
    }
    await page.goto(authorize.href);
    await expect(page.getByRole('heading', { name: 'Sign in to review this request.' })).toBeVisible();
    await page.getByRole('link', { name: 'Sign in with GitHub' }).click();
    await expect(page.getByRole('heading', { name: appName })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Allow access' })).toBeEnabled();
    await expect(page.getByText('Read your profile, rank, usage, and score history', { exact: true })).toBeVisible();
    // Merely loading the preview must not issue an authorization code.
    expect(new URL(page.url()).searchParams.has('code')).toBe(false);
    await page.route('**/oauth-result?**', route => route.fulfill({ contentType: 'text/html', body: '<p>Returned to fixture host</p>' }));
    await page.getByRole('button', { name: 'Allow access' }).click();
    await expect(page.getByText('Returned to fixture host')).toBeVisible();
    const callback = new URL(page.url());
    expect(callback.searchParams.get('iss')).toBe(origin);
    expect(callback.searchParams.get('state')).toBe('browser-fixture-state');
    const code = callback.searchParams.get('code');
    expect(code).toBeTruthy();
    const token = await request.post('/oauth/token', { form: { grant_type: 'authorization_code',
        client_id: client.client_id, redirect_uri: origin + '/oauth-result', code: code!,
        code_verifier: verifier, resource } });
    expect(token.ok()).toBeTruthy();
    const issued = await token.json();
    // Use a real loopback document; route.fulfill callback documents have no server address space.
    await page.goto('/settings/devices');
    // Run an actual cross-origin browser fetch: APIRequestContext does not enforce CORS.
    const browserResult = await page.evaluate(async ({ apiOrigin, token }) => {
        const call = async (access?: string) => {
            const response = await fetch(apiOrigin + '/mcp', {
                method: 'POST', credentials: 'omit',
                headers: {
                    'content-type': 'application/json', accept: 'application/json, text/event-stream',
                    'mcp-protocol-version': '2025-11-25', ...(access ? { authorization: 'Bearer ' + access } : {})
                },
                body: JSON.stringify({ jsonrpc: '2.0', id: 3, method: 'tools/call',
                    params: { name: 'web_memory_my_usage', arguments: {} } })
            });
            return { status: response.status, challenge: response.headers.get('www-authenticate'), body: await response.json() };
        };
        return { denied: await call(), accepted: await call(token) };
    }, { apiOrigin, token: issued.access_token });
    expect(browserResult.denied.status).toBe(401);
    expect(browserResult.denied.challenge).toContain('resource_metadata=');
    expect(browserResult.accepted.status).toBe(200);
    expect(browserResult.accepted.body.result.isError).toBe(false);
    const rank = await request.post('/mcp', { headers: {
        authorization: 'Bearer ' + issued.access_token, accept: 'application/json, text/event-stream',
        'mcp-protocol-version': '2025-11-25'
    }, data: { jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'web_memory_my_rank', arguments: {} } } });
    expect(rank.ok()).toBeTruthy();
    expect((await rank.json()).result.isError).toBe(false);
    const row = page.locator('.device-row').filter({ hasText: 'Remote MCP: ' + appName });
    await expect(row).toBeVisible();
    await expect(row.getByRole('button', { name: 'Rotate token' })).toHaveCount(0);
    page.once('dialog', dialog => dialog.accept());
    await row.getByRole('button', { name: 'Revoke', exact: true }).click();
    await expect(page.getByText('Revoked the connection for Remote MCP: ' + appName + '.', { exact: true })).toBeVisible();
    const refreshed = await request.post('/oauth/token', { form: { grant_type: 'refresh_token',
        client_id: client.client_id, refresh_token: issued.refresh_token, resource } });
    expect(refreshed.status()).toBe(400);
    expect((await refreshed.json()).error).toBe('invalid_grant');
});
