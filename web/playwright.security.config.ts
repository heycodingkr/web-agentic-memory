import { defineConfig, devices } from '@playwright/test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const inherited = process.env.WAM_PRIVACY_DATA_DIR;
const dataDir = inherited || mkdtempSync(join(tmpdir(), 'wam-privacy-'));
process.env.WAM_PRIVACY_DATA_DIR = dataDir;
const logPath = `${dataDir}.server.log`;
process.env.WAM_PRIVACY_SERVER_LOG = logPath;
if (!inherited) {
	writeFileSync(logPath, '', { mode: 0o600, flag: 'wx' });
	process.on('exit', () => { rmSync(dataDir, { recursive: true, force: true }); rmSync(logPath, { force: true }); });
}
const shellQuote = (value: string) => "'" + value.replaceAll("'", "'\\''") + "'";

export default defineConfig({
	outputDir: './test-results/security',
	testDir: './e2e', testMatch: 'security-sinks.spec.ts', globalSetup: './e2e/setup.ts',
	workers: 1, timeout: 240_000,
	use: { ...devices['Desktop Chrome'], channel: 'chrome', baseURL: 'http://127.0.0.1:5193',
		trace: 'off', screenshot: 'off', video: 'off' },
	webServer: [
		{ command: `cargo run --locked -p wam-server --manifest-path ../Cargo.toml --example smoke_fixture > ${shellQuote(logPath)} 2>&1`,
			url: 'http://127.0.0.1:3107/healthz', reuseExistingServer: false, timeout: 180_000,
			env: { WAM_SMOKE_DATA_DIR: dataDir, WAM_SMOKE_BIND: '127.0.0.1:3107',
				WAM_SMOKE_KEY: 'wam-privacy-synthetic-only-key', WAM_SMOKE_WEB_ORIGIN: 'http://127.0.0.1:5193',
				WAM_SMOKE_PRIVACY_CAPTURE: '1' } },
		{ command: 'pnpm exec vite dev --host 127.0.0.1 --port 5193 --strictPort',
			url: 'http://127.0.0.1:5193/v1/auth/config', reuseExistingServer: false, timeout: 90_000,
			env: { VITE_WAM_API_TARGET: 'http://127.0.0.1:3107' } }
	]
});
