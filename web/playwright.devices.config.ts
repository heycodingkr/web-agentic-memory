import { defineConfig, devices } from '@playwright/test';

// Reuse a running frontend; every account/device API response is test-owned.
export default defineConfig({
  testDir: './e2e',
  testMatch: 'devices.spec.ts',
  outputDir: './test-results/devices',
  workers: 1,
  use: { baseURL: process.env.WAM_WEB_URL || 'http://127.0.0.1:5173' },
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Chrome'], channel: 'chrome' } },
    { name: 'mobile', use: { ...devices['Pixel 7'], channel: 'chrome' } }
  ]
});
