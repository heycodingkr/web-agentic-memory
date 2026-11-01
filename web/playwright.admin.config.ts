import { defineConfig, devices } from '@playwright/test';
export default defineConfig({
    outputDir: './test-results/admin',
    testDir: './e2e',
    testMatch: 'admin.spec.ts',
    workers: 1,
    use: { baseURL: process.env.WAM_ADMIN_WEB_ORIGIN || 'http://127.0.0.1:5276' },
    projects: [
        { name: 'desktop-chrome', use: { ...devices['Desktop Chrome'], channel: 'chrome' } },
        { name: 'mobile-chrome', use: { ...devices['Pixel 7'], channel: 'chrome' } }
    ]
});
