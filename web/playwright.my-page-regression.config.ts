import { defineConfig, devices } from '@playwright/test';

// Read-only browser regression checks against an already detached dev server.
export default defineConfig({
  testDir: './e2e',
  testMatch: ['dashboard-failures.spec.ts', 'accessibility.spec.ts'],
  grep: /dashboard|analytics/,
  outputDir: './test-results/my-page-regression',
  workers: 1,
  use: { baseURL: process.env.WAM_MY_PAGE_WEB_ORIGIN || 'http://127.0.0.1:5197' },
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Chrome'], channel: 'chrome' } },
    { name: 'mobile', use: { ...devices['Pixel 7'], channel: 'chrome' } }
  ]
});
