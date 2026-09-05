import { defineConfig, devices } from '@playwright/test';
import { existsSync } from 'fs';
import { resolve } from 'path';

// Persistent Chromium profile directory (gitignored).
const _PROFILE_DIR = resolve(
  __dirname,
  '..',
  'scripts',
  '..',
  'output',
  'playwright',
  'chromium-profile',
);

// Load QA_WALLET_SECRET from the gitignored packages/frontend/.env (T9.4 prep).
// @playwright/test has no dotenv loader — process.loadEnvFile is Node >= 20.12.
const ENV_FILE = resolve(__dirname, '.env');
if (existsSync(ENV_FILE)) {
  process.loadEnvFile(ENV_FILE);
}

export default defineConfig({
  testDir: './qa/wallet',
  testMatch: '**/*.spec.ts',
  timeout: 360_000,
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
