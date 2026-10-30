import { defineConfig, devices } from '@playwright/test';

// Reuse a running frontend. These tests mock public APIs and never change an account or host config.
export default defineConfig({
	testDir: './e2e',
	testMatch: 'guide.spec.ts',
	workers: 1,
	use: {
		...devices['Desktop Chrome'],
		channel: 'chrome',
		baseURL: process.env.WAM_WEB_URL || 'http://127.0.0.1:5173'
	}
});
