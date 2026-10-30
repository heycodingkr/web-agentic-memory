import { defineConfig, devices } from '@playwright/test';
import { mkdtempSync, realpathSync, rmSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// A test-owned frontend with mocked APIs; no account, database or host settings.
const port = 5192;
// Firefox <=157 on macOS 27 also consults the real app-data home despite -profile.
// Keep that lookup in a disposable home; no user browser data or OS grants.
// https://github.com/microsoft/playwright/issues/42768
const inheritedFirefoxHome = process.env.WAM_UI_FIREFOX_HOME;
const firefoxHome = process.platform === 'darwin'
	? inheritedFirefoxHome || realpathSync(mkdtempSync(join(tmpdir(), 'wam-ui-firefox-')))
	: undefined;
if (firefoxHome && !inheritedFirefoxHome) {
	writeFileSync(join(firefoxHome, '.wam-ui-owned'), 'wam-ui', { mode: 0o600, flag: 'wx' });
	// Precreating these paths avoided the observed macOS cleanup EACCES after
	// Gecko created the same directories. Keep all paths inside this owned home.
	for (const dir of ['Downloads', 'Library/Application Support/Firefox/Profiles', 'Library/Caches/Firefox', 'download-artifacts']) {
		mkdirSync(join(firefoxHome, dir), { recursive: true, mode: 0o700 });
	}
	process.env.WAM_UI_FIREFOX_HOME = firefoxHome;
	process.on('exit', () => {
		try { rmSync(firefoxHome, { recursive: true, force: true }); }
		catch { process.exitCode = 1; console.error('Owned Firefox home cleanup failed.'); }
	});
}
export default defineConfig({
	outputDir: './test-results/ui',
	globalSetup: './e2e/ui-setup.ts',
	testDir: './e2e',
	testMatch: ['guide.spec.ts', 'dashboard-failures.spec.ts', 'accessibility.spec.ts'],
	workers: 1,
	use: { baseURL: `http://127.0.0.1:${port}` },
	projects: [
		{ name: 'chrome', use: { ...devices['Desktop Chrome'], channel: 'chrome' } },
		{ name: 'firefox', use: { ...devices['Desktop Firefox'], launchOptions: firefoxHome ? {
			env: { PATH: process.env.PATH || '', CFFIXED_USER_HOME: firefoxHome, TMPDIR: firefoxHome },
			firefoxUserPrefs: { 'browser.download.folderList': 2, 'browser.download.dir': join(firefoxHome, 'download-artifacts') }
		} : {} } },
		// WebKit engine coverage; actual Safari/VoiceOver acceptance is separate.
		{ name: 'webkit', use: { ...devices['Desktop Safari'] } },
		{ name: 'mobile-chrome', testMatch: 'accessibility.spec.ts', use: { ...devices['Pixel 7'], channel: 'chrome' } }
	],
	webServer: {
		command: `pnpm exec vite dev --host 127.0.0.1 --port ${port} --strictPort`,
		url: `http://127.0.0.1:${port}`,
		reuseExistingServer: false,
		timeout: 90_000
	}
});
