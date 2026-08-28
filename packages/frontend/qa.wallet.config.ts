import { defineConfig, devices } from '@playwright/test';
import { resolve } from 'path';

// Persistent Chromium profile directory (gitignored).
const _PROFILE_DIR = resolve(__dirname, '..', 'scripts', '..', 'output', 'playwright', 'chromium-profile');

export default defineConfig({
  testDir: './qa/wallet',
  timeout: 120_000,
  expect: { timeout: 10_000 },
  fullyParallel: false, // wallet tests must run serially
  retries: 0,
  reporter: [
    ['list'],
    ['html', { outputFolder: resolve(__dirname, '..', '..', 'output', 'playwright', 'report') }],
  ],
  use: {
    // dAppwright launches Chromium with a real MetaMask extension.
    headless: false, // MetaMask extension requires headed mode
    viewport: { width: 1280, height: 800 },
    screenshot: 'on',
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'chromium-wallet',
      use: {
        ...devices['Desktop Chrome'],
      },
    },
  ],
  outputDir: resolve(__dirname, '..', '..', 'output', 'playwright', 'test-results'),
});
