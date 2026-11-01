import { defineConfig, devices } from '@playwright/test';

// Local fixtures intercept account/API calls; reuse the detached UI dev server.
export default defineConfig({
 testDir: './e2e',
 testMatch: 'memory-discovery.spec.ts',
 outputDir: './test-results/discovery',
 workers: 1,
 use: { ...devices['Desktop Chrome'], channel: 'chrome', baseURL: 'http://127.0.0.1:5182' },
 webServer: {
  command: 'pnpm exec vite dev --host 127.0.0.1 --port 5182 --strictPort',
  url: 'http://127.0.0.1:5182',
  reuseExistingServer: true,
  timeout: 90_000
 }
});
