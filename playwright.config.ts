import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  testMatch: 'phase29_workbench.spec.ts',
  fullyParallel: false,
  workers: 1,
  timeout: 180_000,
  use: {
    ...devices['Desktop Chrome'],
    viewport: { width: 1440, height: 1200 },
  },
});
