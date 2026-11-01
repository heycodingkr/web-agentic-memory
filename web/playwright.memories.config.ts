import { defineConfig, devices } from '@playwright/test';

// Reuse the detached frontend; APIs are mocked from the bundled example memories and no account or setting changes.
export default defineConfig({
	testDir: './e2e',
	testMatch: ['memories.spec.ts', 'memory-discovery.spec.ts'],
	outputDir: 'test-results-memories',
	workers: 1,
	use: {
		...devices['Desktop Chrome'],
		channel: 'chrome',
		reducedMotion: 'reduce',
		baseURL: process.env.WAM_WEB_URL || 'http://127.0.0.1:5173'
	}
});
