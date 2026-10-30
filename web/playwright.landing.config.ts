import { defineConfig, devices } from '@playwright/test';

// Reuse the detached frontend; APIs are mocked and no account or host settings change.
export default defineConfig({
	testDir: './e2e',
	testMatch: 'landing.spec.ts',
	outputDir: 'test-results-landing',
	workers: 1,
	use: {
		...devices['Desktop Chrome'],
		channel: 'chrome',
		reducedMotion: 'reduce',
		baseURL: process.env.WAM_WEB_URL || 'http://127.0.0.1:5173'
	}
});
