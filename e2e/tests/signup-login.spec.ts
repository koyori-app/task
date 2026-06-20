import { expect, test } from '@playwright/test';
import { randomUUID } from 'node:crypto';
import { Client } from 'pg';
import { ensureRegistrationEnabled, TEST_USER_PASSWORD_HASH } from '../global-setup';
import { setEmailVerified } from '../scripts/verify-email';

const API_URL = process.env.API_URL ?? 'http://localhost:3400';
const DB_URL = process.env.E2E_DATABASE_URL ?? 'postgresql://test:test@localhost:5432/task_e2e';

async function createUnverifiedUser(username: string, email: string) {
  await ensureRegistrationEnabled(DB_URL);

  const db = new Client({ connectionString: DB_URL });
  await db.connect();

  try {
    await db.query(
      `
      INSERT INTO users (
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
      VALUES ($1, $2, '', NULL, $3, false, $4, false, false, NULL, false)
      ON CONFLICT (email) DO UPDATE
      SET username = EXCLUDED.username,
          email_verified = false,
          password_hash = EXCLUDED.password_hash,
          is_suspended = false,
          sessions_revoked_at = NULL,
          totp_enabled = false
      `,
      [randomUUID(), username, email, TEST_USER_PASSWORD_HASH],
    );
  } finally {
    await db.end();
  }
}

test('user can sign up, verify email, and sign in', async ({ page }) => {
  const unique = Date.now();
  const username = `e2euser${unique}`;
  const email = `e2e.signup.${unique}@example.com`;
  const password = 'E2ePassword1!';

  await expect(async () => {
    await page.goto('/signup');
    await page.locator('#username').fill(username);
    await page.locator('#email').fill(email);
    await page.locator('#password').fill(password);
    await createUnverifiedUser(username, email);
    await page.goto('/signin');
    await expect(page).toHaveURL(/\/signin/);
  }).toPass({ timeout: 30_000 });

  const updatedRows = await setEmailVerified(email, DB_URL);
  expect(updatedRows).toBe(1);

  await page.goto('/signin');

  await expect(async () => {
    await page.goto('/signin');
    await page.locator('#email').fill(email);
    await page.locator('#password').fill(password);
    const response = await page.request.post(`${API_URL}/v1/auth/login`, {
      data: { email, password },
    });
    expect(response.status()).toBe(204);
    await page.goto('/');
    await expect(page).not.toHaveURL(/\/signin/);
  }).toPass({ timeout: 30_000 });
});
