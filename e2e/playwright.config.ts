import { defineConfig, devices } from '@playwright/test';
import { STORAGE_STATE } from './global-setup';

const isCI = !!process.env.CI;

export default defineConfig({
  testDir: './tests',
  use: {
    baseURL: process.env.BASE_URL ?? 'http://localhost:3000',
    trace: 'on-first-retry',
  },
  reporter: isCI ? 'github' : 'list',
  projects: [
    // Logs in once and saves the session for authenticated specs.
    {
      name: 'setup',
      testMatch: /auth\.setup\.ts/,
    },
    // Unauthenticated specs (e.g. the sign-in page render check).
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
      testIgnore: [/auth\.setup\.ts/, /.*\.authenticated\.spec\.ts/],
    },
    // Authenticated specs reuse the saved session from the setup project.
    {
      name: 'authenticated',
      use: { ...devices['Desktop Chrome'], storageState: STORAGE_STATE },
      testMatch: /.*\.authenticated\.spec\.ts/,
      dependencies: ['setup'],
    },
  ],
  webServer: [
    {
      // CI: pass BACKEND_BIN=<absolute path> to skip rebuild
      command: process.env.BACKEND_BIN ?? 'cargo run --bin backend',
      cwd: process.env.BACKEND_BIN ? '.' : '../apps/backend',
      url: 'http://localhost:3400/v1/auth/me',
      reuseExistingServer: !isCI,
      timeout: 120_000,
      env: {
        DATABASE_URL: process.env.E2E_DATABASE_URL ?? 'postgresql://test:test@localhost:5432/task_e2e',
        REDIS_URL: process.env.E2E_REDIS_URL ?? 'redis://localhost:6379',
        PERSONAL_TOKEN_SECRET: '00000000000000000000000000000000',
        RECOVERY_CODE_SECRET: '00000000000000000000000000000000',
        TOTP_ENCRYPTION_KEY: '01234567890123456789012345678901',
        SMTP_HOST: 'localhost',
        SMTP_PORT: '587',
        SMTP_USERNAME: 'test',
        SMTP_PASSWORD: 'test',
        SMTP_FROM: 'test@example.com',
        EMAIL_VERIFICATION_APP_URL: 'http://localhost:3000',
        ARGON2_TEST_MODE: 'true',
      },
    },
    {
      command: 'pnpm dev',
      cwd: '../apps/frontend',
      url: 'http://localhost:3000',
      reuseExistingServer: !isCI,
      env: {
        APP_URL: 'http://localhost:3000',
      },
    },
  ],
});
