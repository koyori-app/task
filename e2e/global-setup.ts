// Shared e2e auth fixtures and helpers.
//
// Consumed by tests/auth.setup.ts (the Playwright "setup" project) rather than
// wired as a legacy `globalSetup` file: the webServer entries in
// playwright.config.ts are guaranteed to be running before any project's tests,
// so seeding and the UI login below always have the dev/api servers available.
import { expect, type Page } from '@playwright/test';
import { Client } from 'pg';
import path from 'node:path';

const API_URL = process.env.API_URL ?? 'http://localhost:3400';
const DB_URL = process.env.E2E_DATABASE_URL ?? 'postgresql://test:test@localhost:5432/task_e2e';
const TEST_USER_ID = '00000000-0000-4000-8000-000000000001';
const TEST_USER_PASSWORD_HASH =
  '$argon2id$v=19$m=8192,t=1,p=1$aNpLkTJIWZxc4xa7NVwxmw$DELLN8xZHSjOXOavtdqze+x5XD86fGvZJ4XThahxSFI';

export const TEST_USER = {
  username: 'e2etestuser',
  email: 'e2e@example.com',
  password: 'E2ePassword1!',
};

export const STORAGE_STATE = path.join(import.meta.dirname, '.auth/user.json');

/**
 * Register the e2e user via the public API, then mark its email verified
 * directly in the database so it can sign in without the email-verification
 * step. Idempotent: a repeat register is ignored and the verify update is a
 * no-op on an already-verified row.
 */
export async function seedUser() {
  const db = new Client({ connectionString: DB_URL });
  await db.connect();
  await db.query(`
    INSERT INTO system_settings (
      singleton,
      user_registration_enabled,
      drive_default_quota_mb,
      drive_system_max_quota_mb,
      updated_at
    )
    VALUES (true, true, 10240, 102400, now())
    ON CONFLICT (singleton) DO NOTHING
  `);

  await db.query(
    `INSERT INTO users (
      id,
      username,
      bio,
      avatar_url,
      email,
      email_verified,
      password_hash,
      is_admin,
      is_suspended,
      sessions_revoked_at,
      totp_enabled
    )
    VALUES ($1, $2, '', NULL, $3, true, $4, false, false, NULL, false)
    ON CONFLICT (email) DO UPDATE SET
      username = EXCLUDED.username,
      email_verified = true,
      password_hash = EXCLUDED.password_hash,
      is_suspended = false,
      sessions_revoked_at = NULL,
      totp_enabled = false`,
    [TEST_USER_ID, TEST_USER.username, TEST_USER.email, TEST_USER_PASSWORD_HASH],
  );
  await db.end();
}

/** Sign in through the API and verify the frontend accepts the session. */
export async function signIn(page: Page) {
  await expect(async () => {
    const response = await page.request.post(`${API_URL}/v1/auth/login`, {
      data: {
        email: TEST_USER.email,
        password: TEST_USER.password,
      },
    });
    expect(response.status()).toBe(204);

    await page.goto('/');
    await expect(page).not.toHaveURL(/\/signin/);
  }).toPass({ timeout: 30_000 });
}
