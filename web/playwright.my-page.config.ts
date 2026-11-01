import { defineConfig, devices } from '@playwright/test';
export default defineConfig({
  testDir: './e2e', testMatch: 'my-page.spec.ts', outputDir: './test-results/my-page', workers: 1,
  use: { baseURL: process.env.WAM_MY_PAGE_WEB_ORIGIN || 'http://127.0.0.1:5199' },
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Chrome'], channel: 'chrome' } },
    { name: 'mobile', use: { ...devices['Pixel 7'], channel: 'chrome' } }
  ]
});
