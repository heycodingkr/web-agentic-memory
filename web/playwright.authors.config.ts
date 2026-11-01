import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
	testDir: './e2e',
	testMatch: 'memory-authors.spec.ts',
	outputDir: 'test-results-authors',
	workers: 1,
	use: {
		...devices['Desktop Chrome'],
		channel: 'chrome',
		reducedMotion: 'reduce',
		baseURL: process.env.WAM_WEB_URL || 'http://127.0.0.1:5173'
	}
});
