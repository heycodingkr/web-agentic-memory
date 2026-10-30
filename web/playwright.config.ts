import { defineConfig, devices } from '@playwright/test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const existingDataDir = process.env.WAM_SMOKE_DATA_DIR;
const dataDir = existingDataDir || mkdtempSync(join(tmpdir(), 'wam-smoke-'));
const smokeKey = 'wam-smoke-fixture-only-3105-key';
process.env.WAM_SMOKE_DATA_DIR = dataDir;
if (!existingDataDir) {
	process.on('exit', () => rmSync(dataDir, { recursive: true, force: true }));
}

export default defineConfig({
	outputDir: './test-results/lifecycle',
	testDir: './e2e',
	testMatch: 'local-smoke.spec.ts',
	globalSetup: './e2e/setup.ts',
	fullyParallel: false,
	workers: 1,
	use: { ...devices['Desktop Chrome'], channel: 'chrome', baseURL: 'http://127.0.0.1:5185' },
	webServer: [
		{
			command: 'cargo run --manifest-path ../Cargo.toml -p wam-server --example smoke_fixture',
			url: 'http://127.0.0.1:3105/healthz',
			reuseExistingServer: false,
			timeout: 180_000,
			env: { WAM_SMOKE_DATA_DIR: dataDir, WAM_SMOKE_KEY: smokeKey, WAM_SMOKE_WEB_ORIGIN: 'http://127.0.0.1:5185' }
		},
		{
			command: 'pnpm exec vite dev --host 127.0.0.1 --port 5185 --strictPort',
			url: 'http://127.0.0.1:5185/v1/auth/config',
			reuseExistingServer: false,
			timeout: 90_000,
			env: { VITE_WAM_API_TARGET: 'http://127.0.0.1:3105' }
		}
	]
});
