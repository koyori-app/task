import { defineConfig, devices } from '@playwright/test';
import { resolveE2eDatabaseUrl } from './env';
import { STORAGE_STATE } from './global-setup';

const isCI = !!process.env.CI;

// WebKit catches Safari-only bugs that Chromium never shows. It is not Safari itself,
// so iOS input zoom and tap feel still need a real device.
const targets = [
  { name: 'chromium', device: 'Desktop Chrome' },
  { name: 'webkit', device: 'Desktop Safari' },
  { name: 'mobile-safari', device: 'iPhone 15' },
];

export default defineConfig({
  testDir: './tests',
  use: {
    baseURL: process.env.BASE_URL ?? 'http://localhost:3000',
    trace: isCI ? 'retain-on-failure' : 'on-first-retry',
  },
  reporter: isCI ? [['github'], ['html', { open: 'never' }]] : 'list',
  projects: [
    // Logs in once and saves the session for authenticated specs.
    {
      name: 'setup',
      testMatch: /auth\.setup\.ts/,
    },
    ...targets.flatMap(({ name, device }) => [
      // Unauthenticated specs (e.g. the sign-in page render check).
      {
        name,
        use: { ...devices[device] },
        testIgnore: [/auth\.setup\.ts/, /.*\.authenticated\.spec\.ts/],
      },
      // Authenticated specs reuse the saved session from the setup project.
      {
        name: `${name}-authenticated`,
        use: { ...devices[device], storageState: STORAGE_STATE },
        testMatch: /.*\.authenticated\.spec\.ts/,
        dependencies: ['setup'],
      },
    ]),
  ],
  webServer: [
    {
      // CI: BACKEND_BIN + MIGRATION_BIN skip rebuild; migration runs in start-backend.sh
      command: 'bash scripts/start-backend.sh',
      cwd: '.',
      url: 'http://localhost:3400/v1/auth/me',
      reuseExistingServer: !isCI,
      timeout: 120_000,
      env: {
        DATABASE_URL: resolveE2eDatabaseUrl(),
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
      command: 'pnpm exec vike build && bun ./dist/server/index.mjs',
      cwd: '../apps/frontend',
      url: 'http://localhost:3000',
      reuseExistingServer: !isCI,
      env: {
        APP_URL: 'http://localhost:3000',
      },
    },
  ],
});
