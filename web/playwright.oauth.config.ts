import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
	outputDir: './test-results/oauth',
    testDir: './e2e',
    testMatch: 'oauth.spec.ts',
    workers: 1,
    use: {
        ...devices['Desktop Chrome'],
        channel: 'chrome',
        baseURL: process.env.WAM_OAUTH_WEB_ORIGIN || 'http://127.0.0.1:32055'
    }
});
