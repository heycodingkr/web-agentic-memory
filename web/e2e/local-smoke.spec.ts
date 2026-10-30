import { expect, test } from '@playwright/test';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { join, resolve } from 'node:path';

const key = 'wam-smoke-fixture-only-3105-key';
const apiOrigin = 'http://127.0.0.1:3105';
const editor = 'https://example.org/editor';

async function runAdapter(binary: string, args: string[], input: string, env: Record<string, string>): Promise<{ code: number | null; stdout: string; stderr: string }> {
	return await new Promise((resolveResult, reject) => {
		const child = spawn(resolve(process.cwd(), '..', 'target', 'debug', binary), args, { env: { ...process.env, ...env } });
		let stdout = '';
		let stderr = '';
		const timeout = setTimeout(() => child.kill(), 8_000);
		child.stdout.setEncoding('utf8').on('data', chunk => { stdout += chunk; });
		child.stderr.setEncoding('utf8').on('data', chunk => { stderr += chunk; });
		child.on('error', reject);
		child.on('close', code => { clearTimeout(timeout); resolveResult({ code, stdout, stderr }); });
		child.stdin.end(input);
	});
}

test('anonymous browse, OAuth, contribution, curation, replacement, and account lifecycle', async ({ page }) => {
	const request = page.request;
	const anonymousSession = await request.get('/v1/auth/session');
	expect(anonymousSession.status()).toBe(401);
	expect(anonymousSession.headers()['x-wam-trace-id']).toBeTruthy();
	const config = await request.get('/v1/auth/config');
	expect(config.ok()).toBeTruthy();
	expect(config.headers()['content-type']).toContain('application/json');
	expect((await config.json()).github_oauth_enabled).toBe(true);

	await page.goto('/memories');
	await expect(page.getByRole('heading', { name: 'No public memories yet.' })).toBeVisible();
	await page.goto('/login');
	await page.getByRole('link', { name: /Connect GitHub account/ }).click();
	await expect(page.getByText('Signed in with GitHub. Your personal rank is private.')).toBeVisible();
	await page.goto('/login');
	await expect(page.getByRole('heading', { name: 'Account connected.' })).toBeVisible();
	const sessionResponse = await request.get('/v1/auth/session');
	expect(sessionResponse.ok()).toBeTruthy();
	const session = await sessionResponse.json() as { csrf_token: string };
	expect(session.csrf_token).toBeTruthy();
	const browserFailure = await page.evaluate(async () => {
		const { api } = await import('/src/lib/api.ts');
		try {
			await api('/v1/me/profile', {}, 'web-private-sentinel');
			return null;
		} catch (cause) {
			const error = cause as { status: number; message: string; traceId?: string };
			return { status: error.status, message: error.message, traceId: error.traceId };
		}
	});
	expect(browserFailure?.status).toBe(401);
	expect(browserFailure?.traceId).toMatch(/^[a-f0-9]{32}$/);
	expect(browserFailure?.message).toContain(`Reference: ${browserFailure?.traceId}`);
	expect(browserFailure?.message).not.toContain('web-private-sentinel');

	const missingCsrf = await request.post('/v1/me/contributions', {
		headers: { 'x-wam-client': 'web' },
		data: { url: `${editor}?private-sentinel=must-not-log`, task_category: 'compose-draft', actions: ['navigate', 'edit', 'save'], consent: true, consent_version: 'public-sharing-v1' }
	});
	expect(missingCsrf.status()).toBe(403);
	expect(missingCsrf.headers()['x-wam-trace-id']).toBeTruthy();
	const noKey = await request.post(`${apiOrigin}/__smoke/curate`);
	expect(noKey.status()).toBe(401);
	const db = new DatabaseSync(join(process.env.WAM_SMOKE_DATA_DIR!, 'diagnostics.sqlite'));
	db.exec('PRAGMA busy_timeout=2000');
	try {
		const trace = missingCsrf.headers()['x-wam-trace-id'];
		await expect.poll(() => db.prepare('SELECT source, status_code, document FROM wam_diagnostic_events WHERE trace_id = ? AND status_code = 403').get(trace))
			.toMatchObject({ source: 'web', status_code: 403 });
		const row = db.prepare('SELECT document FROM wam_diagnostic_events WHERE trace_id = ? AND status_code = 403').get(trace) as { document: string };
		expect(row.document).not.toContain('private-sentinel');
		await expect.poll(() => {
			const rows = db.prepare("SELECT document FROM wam_diagnostic_events WHERE trace_id = ? AND source = 'web' AND error_code = 'http_error' AND status_code = 401").all(browserFailure!.traceId) as { document: string }[];
			return rows.some(row => JSON.parse(row.document).kind === 'client_error');
		}).toBe(true);
		const browserDocuments = db.prepare('SELECT document FROM wam_diagnostic_events WHERE trace_id = ?').all(browserFailure!.traceId) as { document: string }[];
		expect(browserDocuments.map(row => row.document).join('\n')).not.toContain('web-private-sentinel');
		await page.evaluate(() => {
			window.setTimeout(() => { void Promise.reject(new Error('smoke-rejection')); }, 0);
		});
		await expect.poll(() => db.prepare("SELECT document FROM wam_diagnostic_events WHERE source = 'web' AND error_code = 'unhandled_rejection' LIMIT 1").get())
			.toBeTruthy();
	} finally { db.close(); }

	const adapterData = process.env.WAM_SMOKE_DATA_DIR!;
	const spool = join(adapterData, 'telemetry');
	mkdirSync(spool, { mode: 0o700 });
	const adapterEnv = {
		WAM_ENDPOINT: apiOrigin,
		WAM_TOKEN: '',
		WAM_CACHE_PATH: join(adapterData, 'client-cache.sqlite3'),
		WAM_TELEMETRY_SPOOL_DIR: spool,
		WAM_TELEMETRY: 'on'
	};
	const cli = await runAdapter('wam', ['rank'], '', { ...adapterEnv, WAM_TOKEN: 'cli-private-sentinel' });
	expect(cli.code).toBe(3);
	const cliTrace = cli.stderr.match(/trace: ([a-f0-9]{32})/)?.[1];
	expect(cliTrace).toBeTruthy();
	expect(cli.stderr).not.toContain('cli-private-sentinel');
	const mcp = await runAdapter('wam-mcp', [], '{"jsonrpc":"2.0","id":1,"method":"mcp-private-sentinel"\n', adapterEnv);
	expect(mcp.code).toBe(0);
	const mcpResponse = JSON.parse(mcp.stdout.trim()) as { error: { code: number; data: { trace_id: string } } };
	expect(mcpResponse.error.code).toBe(-32700);
	const mcpTrace = mcpResponse.error.data.trace_id;
	expect(mcpTrace).toMatch(/^[a-f0-9]{32}$/);
	expect(mcp.stdout).not.toContain('mcp-private-sentinel');
	const adaptersDb = new DatabaseSync(join(adapterData, 'diagnostics.sqlite'));
	adaptersDb.exec('PRAGMA busy_timeout=2000');
	try {
		await expect.poll(() => adaptersDb.prepare("SELECT source, error_code, status_code FROM wam_diagnostic_events WHERE trace_id = ? AND source = 'cli' AND error_code = 'http_error'").get(cliTrace))
			.toMatchObject({ source: 'cli', error_code: 'http_error', status_code: 401 });
		await expect.poll(() => adaptersDb.prepare("SELECT source, error_code FROM wam_diagnostic_events WHERE trace_id = ? AND source = 'mcp' AND error_code = 'invalid_input'").get(mcpTrace))
			.toMatchObject({ source: 'mcp', error_code: 'invalid_input' });
		const documents = adaptersDb.prepare('SELECT document FROM wam_diagnostic_events').all() as { document: string }[];
		expect(documents.map(row => row.document).join('\n')).not.toMatch(/private-sentinel|must-not-log/);
	} finally { adaptersDb.close(); }
	console.info(`Smoke adapter traces: cli=${cliTrace} mcp=${mcpTrace}`);

	await page.goto('/contribute');
	await expect(page.getByText('Signed in with GitHub.')).toBeVisible();
	await page.getByLabel('Site where you did the work').fill(`${editor}?private=removed#fragment`);
	await page.getByLabel(/I agree to store the normalized URL/).check();
	await page.getByRole('button', { name: /Agree and submit locally/ }).click();
	await expect(page.getByRole('status').filter({ hasText: 'Status: queued' })).toBeVisible();

	const firstCurate = await request.post(`${apiOrigin}/__smoke/curate`, { headers: { 'x-wam-smoke-key': key } });
	expect(firstCurate.ok()).toBeTruthy();
	const first = await firstCurate.json() as { status: string; memory_id: string; version_id: string };
	expect(first.status).toBe('published');
	expect(first.memory_id).toBeTruthy();
	const firstDetail = await request.get(`/v1/memories/${encodeURIComponent(first.memory_id)}?level=2`);
	expect(firstDetail.ok()).toBeTruthy();
	expect((await firstDetail.json()).content).toContain('type');
	const firstRecall = await request.post('/v1/memories/recall', { data: { url: editor, task_category: 'compose-draft' } });
	expect(firstRecall.ok()).toBeTruthy();
	expect((await firstRecall.json()).hits[0].version_id).toBe(first.version_id);
	await page.goto('/memories');
	await expect(page.getByRole('link', { name: /Save an editor draft/ })).toBeVisible();
	await page.getByRole('link', { name: /Save an editor draft/ }).click();
	await expect(page.getByRole('heading', { name: 'Save an editor draft' })).toBeVisible();
	await expect(page.getByText('Open the editor, enter the draft, and save it.', { exact: true })).toBeVisible();

	await page.goto('/contribute');
	await expect(page.getByText('Signed in with GitHub.')).toBeVisible();
	await page.getByLabel('Site where you did the work').fill(editor);
	await page.getByRole('button', { name: 'Move up: Check saved state' }).click();
	await page.getByLabel(/I agree to store the normalized URL/).check();
	const changedRequest = page.waitForRequest(incoming => incoming.method() === 'POST' && incoming.url().endsWith('/v1/me/contributions'));
	await page.getByRole('button', { name: /Agree and submit locally/ }).click();
	const changed = await changedRequest;
	expect(changed.postDataJSON().actions).toEqual(['navigate', 'save', 'edit']);
	await expect(page.getByRole('status').filter({ hasText: 'Status: queued' })).toBeVisible();
	const secondCurate = await request.post(`${apiOrigin}/__smoke/curate`, { headers: { 'x-wam-smoke-key': key } });
	expect(secondCurate.ok()).toBeTruthy();
	const second = await secondCurate.json() as { status: string; memory_id: string; version_id: string };
	expect(second.status).toBe('published');
	expect(second.memory_id).toBe(first.memory_id);
	expect(second.version_id).not.toBe(first.version_id);
	console.info(`Smoke publication: memory=${second.memory_id} first=${first.version_id} second=${second.version_id}`);
	const current = await request.get(`/v1/memories/${encodeURIComponent(first.memory_id)}?level=2`);
	expect((await current.json()).version_id).toBe(second.version_id);
	const secondRecall = await request.post('/v1/memories/recall', { data: { url: editor, task_category: 'compose-draft' } });
	expect((await secondRecall.json()).hits[0].version_id).toBe(second.version_id);
	await page.goto(`/memories/${encodeURIComponent(first.memory_id)}`);
	await expect(page.getByText('Save the draft using the updated editor order.', { exact: true })).toBeVisible();
	await page.getByText('Version and URL scope').click();
	await expect(page.getByText(second.version_id)).toBeVisible();

	const dashboard = await request.get('/v1/me/dashboard');
	expect(dashboard.ok()).toBeTruthy();
	expect((await dashboard.json()).total_points).toBe(40);
	await page.goto('/contribute');
	await expect(page.getByRole('button', { name: 'Withdraw' }).first()).toBeVisible();
	await page.getByRole('button', { name: 'Withdraw' }).first().click();
	await expect(page.getByRole('status').filter({ hasText: 'withdrawn' })).toBeVisible();
	const consent = await request.get('/v1/me/consent');
	expect(consent.ok()).toBeTruthy();
	expect((await consent.json()).enabled).toBe(true);
	const deleteAccount = await request.delete('/v1/me/account', {
		headers: { 'x-csrf-token': session.csrf_token }, data: { confirmation: 'DELETE' }
	});
	expect(deleteAccount.ok()).toBeTruthy();
	expect((await deleteAccount.json()).deleted).toBe(true);
	expect((await request.get('/v1/auth/session')).status()).toBe(401);
});
