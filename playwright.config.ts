import { defineConfig } from '@playwright/test';

// The interface runs against its demo backend (src/lib/mock.ts) in a normal
// browser. Set PW_CHANNEL=msedge to use an installed Edge instead of
// Playwright's own Chromium.
export default defineConfig({
  testDir: 'tests/e2e',
  timeout: 30_000,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: process.env.CI ? [['github'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: 'http://localhost:1420',
    viewport: { width: 1280, height: 800 },
    trace: 'retain-on-failure',
    ...(process.env.PW_CHANNEL ? { channel: process.env.PW_CHANNEL } : {}),
  },
  webServer: {
    command: 'pnpm dev',
    url: 'http://localhost:1420',
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
