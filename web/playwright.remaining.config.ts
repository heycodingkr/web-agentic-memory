import { defineConfig } from '@playwright/test';
import base from './playwright.landing.config';

export default defineConfig({
    ...base,
    testMatch: 'remaining.spec.ts',
    outputDir: 'test-results-remaining'
});
