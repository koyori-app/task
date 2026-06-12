import { defineConfig } from '@playwright/test';

const isCI = !!process.env.CI;

export default defineConfig({
  testDir: './tests',
  use: {
    baseURL: process.env.BASE_URL ?? 'http://localhost:3000',
    trace: 'on-first-retry',
  },
  reporter: isCI ? 'github' : 'list',
  webServer: [
    {
      // CI: pass BACKEND_BIN=./apps/backend/target/release/backend to skip rebuild
      command: process.env.BACKEND_BIN ?? 'cargo run --bin backend',
      cwd: process.env.BACKEND_BIN ? '.' : '../backend',
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
      cwd: '../frontend',
      url: 'http://localhost:3000',
      reuseExistingServer: !isCI,
    },
  ],
});
