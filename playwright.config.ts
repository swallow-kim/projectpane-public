import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests/browser',
  use: { baseURL: 'http://127.0.0.1:1420', viewport: { width: 1280, height: 900 }, launchOptions: process.env.PROJECTPANE_CHROMIUM ? { executablePath: process.env.PROJECTPANE_CHROMIUM } : {} },
  webServer: { command: 'npm run dev', url: 'http://127.0.0.1:1420', reuseExistingServer: !process.env.CI },
});
