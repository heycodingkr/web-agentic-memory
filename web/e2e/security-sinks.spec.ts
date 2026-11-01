import { expect, test } from '@playwright/test';
import { DatabaseSync } from 'node:sqlite';
import { createHash, randomBytes } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, appendFileSync, rmSync, existsSync } from 'node:fs';
import { spawn, execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const root = resolve(process.cwd(), '..');
const origin = 'http://127.0.0.1:3107';
const key = 'wam-privacy-synthetic-only-key';
const sinks = ['client', 'web', 'api', 'database', 'diagnostics', 'cache', 'process', 'gatekeeper'];
const digest = (data: string | Buffer) => createHash('sha256').update(data).digest('hex');

test('SEC-A24: 500 synthetic sentinels, actual transports, eight collected sinks and leak positive control', async ({ page }) => {
	const source = readFileSync(join(root, 'tests/sentinels.json'));
	const manifest = JSON.parse(source.toString());
	expect(manifest.synthetic_only).toBe(true);
	expect(manifest.entries).toHaveLength(500);
	const entries: { id: string; family: string; value: string; synthetic: boolean }[] = manifest.entries;
	expect(entries.every(entry => entry.synthetic)).toBe(true);
	const run = join(root, 'target/e2e-logs/security-sinks', new Date().toISOString().replaceAll(/[:.]/g, '-') + '-' + randomBytes(2).toString('hex'));
	mkdirSync(run, { recursive: true, mode: 0o700 });
	const meta = { suite: 'security-sinks', run_id: run.split('/').pop(), synthetic_only: true,
		commit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root }).toString().trim(),
		dirty: execFileSync('git', ['status', '--porcelain'], { cwd: root }).toString().trim().length > 0,
		started_at: new Date().toISOString(), node: process.version, os: process.platform,
		browser: page.context().browser()!.version(), model: 'fake:scripted', sentinel_sha256: digest(source) };
	writeFileSync(join(run, 'meta.json'), JSON.stringify(meta), { mode: 0o600, flag: 'wx' });
	writeFileSync(join(run, 'trace.jsonl'), JSON.stringify({ type: 'started', ts: meta.started_at }) + '\n', { mode: 0o600, flag: 'wx' });
	const isolated = mkdtempSync(join(tmpdir(), 'wam-privacy-client-'));
	for (const dir of ['home', 'config', 'cache', 'data', 'tmp']) mkdirSync(join(isolated, dir), { mode: 0o700 });
	const inventory = { schema_version: 1, fixture: true, sentinel_count: 500, manifest_sha256: digest(source),
		sinks: sinks.map(name => ({ name, status: 'not-collected', artifacts: [] as { path: string; sha256: string }[] })) };
	function save(name: string, file: string, data: string | Buffer) {
		const path = `${name}-${file}`;
		writeFileSync(join(run, path), data, { mode: 0o600, flag: 'wx' });
		const sink = inventory.sinks.find(row => row.name === name)!;
		sink.status = 'collected'; sink.artifacts.push({ path, sha256: digest(data) });
	}
	const outbound: unknown[] = [];
	const webRequests: unknown[] = [];
	const consoleMessages: unknown[] = [];
	const responses: unknown[] = [];
	const proxyErrors: string[] = [];
	const proxy = createServer(async (request, response) => {
		try {
			const url = new URL(request.url!, origin);
			if (url.origin !== origin) throw new Error('fixture proxy origin');
			const chunks: Buffer[] = [];
			for await (const chunk of request) chunks.push(Buffer.from(chunk));
			const body = Buffer.concat(chunks);
			// Capture the actual outbound wire data without masking. Hostile direct
			// API inputs go to the backend separately and are never written here.
			outbound.push({ path: request.url, headers: request.headers, body: body.toString() });
			const headers = Object.fromEntries(Object.entries(request.headers).filter(([name]) => !['host', 'connection', 'content-length'].includes(name))) as Record<string, string>;
			const upstream = await fetch(url, { method: request.method, headers,
				body: ['GET', 'HEAD'].includes(request.method!) ? undefined : body, signal: AbortSignal.timeout(10_000), redirect: 'error' });
			response.statusCode = upstream.status;
			upstream.headers.forEach((value, name) => {
				if (!['transfer-encoding', 'content-length', 'connection', 'content-encoding'].includes(name)) response.setHeader(name, value);
			});
			response.end(Buffer.from(await upstream.arrayBuffer()));
		} catch {
			proxyErrors.push('fixture-proxy-error'); response.writeHead(502).end();
		}
	});
	await new Promise<void>(resolveReady => proxy.listen(0, '127.0.0.1', resolveReady));
	const port = (proxy.address() as { port: number }).port;
	const env: Record<string, string> = { PATH: process.env.PATH!, HOME: join(isolated, 'home'), TMPDIR: join(isolated, 'tmp'),
		XDG_CONFIG_HOME: join(isolated, 'config'), XDG_DATA_HOME: join(isolated, 'data'), XDG_CACHE_HOME: join(isolated, 'cache'),
		WAM_ENDPOINT: `http://127.0.0.1:${port}`, WAM_ANONYMOUS: '1', WAM_CACHE_PATH: join(isolated, 'cache/client.sqlite3'),
		WAM_TELEMETRY_SPOOL_DIR: join(isolated, 'data/telemetry'), WAM_TELEMETRY: 'on' };
	async function adapter(binary: string, args: string[], input: string, captureProcess = false) {
		return await new Promise<{ code: number | null; stdout: string; stderr: string }>((resolveResult, reject) => {
			const child = spawn(join(root, 'target/debug', binary), args, { env, detached: true });
			let stdout = '', stderr = '', failed = false;
			const stop = () => { try { process.kill(-child.pid!, 'SIGKILL'); } catch {} };
			const timer = setTimeout(() => { failed = true; stop(); }, 45_000);
			child.stdout.on('data', data => { stdout += data; if (stdout.length > 8_000_000) { failed = true; stop(); } });
			child.stderr.on('data', data => { stderr += data; if (stderr.length > 1_000_000) { failed = true; stop(); } });
			child.on('error', () => { clearTimeout(timer); stop(); reject(new Error('fixture adapter spawn')); });
			child.on('close', code => {
				clearTimeout(timer);
				if (failed) reject(new Error('fixture adapter timeout/output limit'));
				else resolveResult({ code, stdout, stderr });
			});
			if (captureProcess) {
				try { save('process', 'owned-mcp.txt', execFileSync('ps', ['eww', '-p', String(child.pid), '-o', 'command='])); }
				catch { failed = true; stop(); }
			}
			child.stdin.end(input);
		});
	}
	let passed = false;
	try {
		page.on('console', message => consoleMessages.push({ type: message.type(), text: message.text() }));
		page.on('pageerror', error => consoleMessages.push({ type: 'pageerror', text: error.message }));
		page.on('request', request => {
			if (new URL(request.url()).pathname.startsWith('/v1/')) webRequests.push({ url: request.url(), body: request.postData() });
		});
		await page.goto('/login');
		await page.getByRole('link', { name: /Connect GitHub account/ }).click();
		await expect(page.getByRole('heading', { name: 'My Page.' })).toBeVisible();
		await page.goto('/contribute');
		await expect(page.getByText('Signed in to WAM.')).toBeVisible();
		for (const entry of entries) {
			await page.getByLabel('Site where you did the work').fill(`https://example.org/editor?private=${encodeURIComponent(entry.value)}#${encodeURIComponent(entry.value)}`);
			await expect(page.locator('.contribution-aside pre')).toContainText('"url": "https://example.org/editor"');
		}
		await page.getByLabel(/I agree to store the normalized URL/).check();
		await page.getByRole('button', { name: /Agree and submit/ }).click();
		await expect(page.getByRole('status').filter({ hasText: 'Status: queued' })).toBeVisible();
		await page.getByLabel('Site where you did the work').fill('https://example.org/editor');
		const curated = await page.request.post(`${origin}/__smoke/curate`, { headers: { 'x-wam-smoke-key': key } });
		expect(curated.status()).toBe(200);
		expect((await curated.json()).status).toBe('published');
		const session = await (await page.request.get('/v1/auth/session')).json();
		const ids = new Set<string>();
		for (const entry of entries) {
			const accepted = await page.request.post(`${origin}/v1/me/contributions`, {
				headers: { 'x-csrf-token': session.csrf_token }, data: { url: `https://example.org/editor?private=${encodeURIComponent(entry.value)}`,
					task_category: 'compose-draft', actions: ['navigate', 'edit', 'save'], consent: true, consent_version: 'public-sharing-v1', nonce: 'privacy-retry' }
			});
			const content = await accepted.text();
			responses.push({ case: entry.id, operation: 'contribution', status: accepted.status(), headers: accepted.headers(), body: content });
			expect(accepted.status()).toBe(201); ids.add(JSON.parse(content).id);
			const rejected = await page.request.post(`${origin}/v1/memories/recall`, { data: { url: 'https://example.org/editor', [entry.value]: 'unexpected' } });
			responses.push({ case: entry.id, operation: 'closed-schema', status: rejected.status(), headers: rejected.headers(), body: await rejected.text() });
			expect([400, 422]).toContain(rejected.status());
		}
		expect(ids.size).toBe(1);
		const rpc = [...entries.map((entry, index) => ({ jsonrpc: '2.0', id: index + 1, method: 'tools/call',
			params: { name: 'web_memory_recall', arguments: { url: `https://example.org/editor?private=${encodeURIComponent(entry.value)}`, task_category: 'compose-draft' } } })),
			...entries.filter((_, index) => index % 50 === 0).map((entry, index) => ({ jsonrpc: '2.0', id: index + 501, method: entry.value }))
		].map(row => JSON.stringify(row)).join('\n') + '\n';
		const mcp = await adapter('wam-mcp', [], rpc, true);
		save('client', 'mcp.json', JSON.stringify(mcp)); expect(mcp.code).toBe(0);
		const replies = mcp.stdout.trim().split('\n').map(line => JSON.parse(line));
		expect(replies).toHaveLength(510);
		expect(replies.slice(0, 500).every((reply, index) => reply.id === index + 1 && reply.result?.isError === false && JSON.parse(reply.result.content[0].text).hits.length > 0)).toBe(true);
		expect(replies.slice(500).every(reply => reply.error?.code === -32601)).toBe(true);
		const cliResults = [];
		for (const entry of entries.filter((_, index) => index % 50 === 0)) {
			const answer = await adapter('wam', ['recall', '--stdin', '--json', '--anonymous'], JSON.stringify({ url: `https://example.org/editor?private=${encodeURIComponent(entry.value)}`, task_category: 'compose-draft' }));
			expect(answer.code).toBe(0); cliResults.push(answer);
			const invalid = await adapter('wam', ['recall', '--stdin', '--json', '--anonymous'], JSON.stringify({ url: entry.value }));
			expect(invalid.code).toBe(2); cliResults.push(invalid);
		}
		save('client', 'cli.json', JSON.stringify(cliResults));
		expect(outbound.length).toBeGreaterThan(0); expect(proxyErrors).toEqual([]);
		save('client', 'outbound.json', JSON.stringify(outbound));
		save('api', 'responses.json', JSON.stringify(responses));
		for (const path of ['/v1/memories', '/v1/me/contributions', '/v1/me/dashboard']) {
			const response = await page.request.get(path); expect(response.status()).toBe(200);
			save('api', `${path.split('/').pop()}.json`, await response.text());
		}
		save('web', 'requests.json', JSON.stringify(webRequests));
		save('web', 'console.json', JSON.stringify(consoleMessages));
		save('web', 'storage.json', JSON.stringify(await page.evaluate(() => ({ local: { ...localStorage }, session: { ...sessionStorage } }))));
		save('web', 'dom.html', await page.content());
		const dataDir = process.env.WAM_PRIVACY_DATA_DIR!;
		save('database', 'application.json', readFileSync(join(dataDir, 'application.json')));
		const diagnostics = new DatabaseSync(join(dataDir, 'diagnostics.sqlite'));
		try {
			diagnostics.exec('PRAGMA busy_timeout=2000');
			const rows = diagnostics.prepare('SELECT document FROM wam_diagnostic_events').all();
			expect(rows.length).toBeGreaterThan(1_000);
			save('diagnostics', 'rows.json', JSON.stringify(rows));
			const snapshot = join(isolated, 'diagnostics.sqlite');
			diagnostics.exec(`VACUUM INTO '${snapshot.replaceAll("'", "''")}'`);
			save('diagnostics', 'sqlite.bin', readFileSync(snapshot));
		} finally { diagnostics.close(); }
		save('diagnostics', 'server.log', readFileSync(process.env.WAM_PRIVACY_SERVER_LOG!));
		const cache = new DatabaseSync(env.WAM_CACHE_PATH);
		try {
			const rows = cache.prepare('SELECT cache_key, origin_hash, payload FROM wam_cache_entries').all();
			expect(rows.length).toBeGreaterThan(0);
			save('cache', 'rows.json', JSON.stringify(rows));
		} finally { cache.close(); }
		save('cache', 'sqlite.bin', readFileSync(env.WAM_CACHE_PATH));
		save('gatekeeper', 'provider.jsonl', readFileSync(join(dataDir, 'gatekeeper.jsonl')));
		expect(JSON.parse(readFileSync(join(dataDir, 'gatekeeper.jsonl'), 'utf8').trim().split('\n')[0]).transcript).toContain('[JOB]');
		// Positive control uses the same collection checker and corpus. It must
		// fail after a correctly hashed sentinel leak is deliberately introduced.
		writeFileSync(join(run, 'inventory.json'), JSON.stringify(inventory), { mode: 0o600 });
		const clean = JSON.parse(execFileSync('python3', ['-B', join(root, 'tests/security/check_sink_inventory.py'), join(run, 'inventory.json'), '--json-out', join(run, 'report.json')], { timeout: 90_000 }).toString());
		expect(clean.passed).toBe(true);
		const control = structuredClone(inventory);
		const controlData = entries.map(entry => JSON.stringify({ message: entry.value })).join('\n');
		writeFileSync(join(run, 'positive-control.jsonl'), controlData, { mode: 0o600 });
		control.sinks.find(row => row.name === 'api')!.artifacts = [{ path: 'positive-control.jsonl', sha256: digest(controlData) }];
		writeFileSync(join(run, 'control-inventory.json'), JSON.stringify(control), { mode: 0o600 });
		let rejected = false;
		try { execFileSync('python3', ['-B', join(root, 'tests/security/check_sink_inventory.py'), join(run, 'control-inventory.json'), '--json-out', join(run, 'control-report.json')], { timeout: 90_000 }); }
		catch (error) { rejected = (error as { status: number }).status === 1; }
		expect(rejected).toBe(true);
		const positive = JSON.parse(readFileSync(join(run, 'control-report.json'), 'utf8'));
		expect(new Set(positive.sinks.flatMap((sink: { hits: { sentinel_id: string }[] }) => sink.hits.map(hit => hit.sentinel_id))).size).toBe(500);
		rmSync(join(run, 'positive-control.jsonl')); // retain safe ID-only result, never make it part of a clean capture
		writeFileSync(join(run, 'coverage.json'), JSON.stringify({ synthetic_only: true, sentinel_count: 500,
			web_previews: 500, api_contributions: 500, api_rejections: 500, stdio_mcp_recalls: 500, mcp_rejections: 10, cli_recalls: 10, cli_rejections: 10,
			quota: 'fixture capacities x100', provider: 'scripted; actual input/output captured',
			database: 'local application JSON and diagnostic SQLite; PostgreSQL/Redis/WAL/staging not covered',
			positive_control_detected: 500, passed: true }), { mode: 0o600 });
		console.info(`Synthetic privacy evidence: ${run}`);
		passed = true;
	} finally {
		await new Promise<void>(done => { proxy.closeAllConnections(); proxy.close(() => done()); });
		rmSync(isolated, { recursive: true, force: true });
		const cleaned = !existsSync(isolated);
		passed &&= cleaned;
		writeFileSync(join(run, 'meta.json'), JSON.stringify({ ...meta, ended_at: new Date().toISOString(), result: passed ? 'done' : 'failed', cleanup_passed: cleaned }));
		writeFileSync(join(run, 'cases.jsonl'), JSON.stringify({ case: 'SEC-A24', verdict: passed ? 'pass' : 'fail',
			expected: '500 inputs; all eight sinks clean; positive control detects 500; owned cleanup', actual: passed }) + '\n', { mode: 0o600, flag: 'wx' });
		appendFileSync(join(run, 'trace.jsonl'), JSON.stringify({ type: passed ? 'done' : 'failed', ts: new Date().toISOString() }) + '\n');
		writeFileSync(join(run, 'summary.md'), `# Synthetic privacy sweep\n\nResult: ${passed ? 'done' : 'failed'}. Source dirty: ${meta.dirty}. Owned client cleanup: ${cleaned}.\n\n500 synthetic inputs; eight local sink categories. Read inventory.json, report.json and coverage.json for actual scope. Fake OAuth/replay/model; no staging or live-account attestation.\n`, { mode: 0o600, flag: 'wx' });
		// An early assertion must leave an incomplete inventory, never a clean report.
		if (!inventory.sinks.every(sink => sink.status === 'collected')) {
			writeFileSync(join(run, 'incomplete-inventory.json'), JSON.stringify(inventory), { mode: 0o600 });
		}
		expect(cleaned).toBe(true);
	}
});
