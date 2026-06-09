import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/perf',
  timeout: 45_000,
  expect: {
    timeout: 5_000
  },
  fullyParallel: false,
  workers: 1,
  reporter: 'line',
  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL ?? 'http://localhost:43189',
    trace: 'off',
    screenshot: 'off',
    video: 'off',
    actionTimeout: 10_000,
    navigationTimeout: 10_000
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] }
    }
  ],
  webServer: {
    command: 'pnpm dev --host 127.0.0.1 --port 43189',
    url: 'http://127.0.0.1:43189',
    reuseExistingServer: !process.env.CI,
    timeout: 30_000
  }
});
