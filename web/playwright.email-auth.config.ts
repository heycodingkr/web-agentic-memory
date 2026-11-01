import { defineConfig, devices } from '@playwright/test';

// Reuse the detached, test-owned frontend started by the operator.
export default defineConfig({
  outputDir: './test-results/email-auth',
  testDir: './e2e',
  testMatch: 'email-auth.spec.ts',
  workers: 1,
  use: { baseURL: process.env.WAM_EMAIL_WEB_ORIGIN || 'http://127.0.0.1:5197' },
  projects: [
    { name: 'desktop-chrome', use: { ...devices['Desktop Chrome'], channel: 'chrome' } },
    { name: 'mobile-chrome', use: { ...devices['Pixel 7'], channel: 'chrome' } }
  ]
});
