// Shared e2e auth fixtures and helpers.
//
// DB schema is applied by scripts/start-backend.sh (migration crate) before webServers start.
// This module only seeds rows and exposes UI/API helpers for tests.
import { expect, type Page } from '@playwright/test';
import { Client } from 'pg';
import path from 'node:path';
import { resolveE2eDatabaseUrl } from './env';

const API_URL = process.env.API_URL ?? 'http://localhost:3400';
export const DB_URL = resolveE2eDatabaseUrl();
const TEST_USER_ID = '00000000-0000-4000-8000-000000000001';
export const TEST_USER_PASSWORD_HASH =
  '$argon2id$v=19$m=8192,t=1,p=1$aNpLkTJIWZxc4xa7NVwxmw$DELLN8xZHSjOXOavtdqze+x5XD86fGvZJ4XThahxSFI';

export const TEST_USER = {
  username: 'e2etestuser',
  email: 'e2e@example.com',
  password: 'E2ePassword1!',
};

export const STORAGE_STATE = path.join(import.meta.dirname, '.auth/user.json');

/** Wait until HydrationSafeForm marks the form hydrated (data-hydrated="true"). */
export async function waitForClientHydration(page: Page) {
  await page.locator('form[data-hydrated="true"]').waitFor({ timeout: 15_000 });
}

/** Type into a Vue/TanStack controlled field (fill() skips model updates in prod SSR). */
export async function typeIntoFormField(page: Page, selector: string, value: string) {
  const field = page.locator(selector);
  await expect(field).toBeEditable({ timeout: 15_000 });
  await field.click();
  // clear() desyncs TanStack form state on prod SSR; signup fields start empty.
  await field.pressSequentially(value, { delay: 10 });
  // Storybook SignUp.stories uses tab after type to trigger blur validators.
  await field.press('Tab');
}

/** Ensure signup is allowed. Migration seeds system_settings with defaults; only flip the flag. */
export async function ensureRegistrationEnabled(dbUrl = DB_URL) {
  const db = new Client({ connectionString: dbUrl });
  await db.connect();

  try {
    await db.query(`
      UPDATE system_settings
      SET user_registration_enabled = true,
          updated_at = now()
      WHERE singleton = true
    `);
  } finally {
    await db.end();
  }
}

/**
 * Register e2e user directly in the database and mark its email verified.
 * Idempotent: repeat runs update the same test row.
 */
export async function seedUser() {
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
      VALUES ($1, $2, '', NULL, $3, true, $4, false, false, NULL, false)
      ON CONFLICT (email) DO UPDATE
      SET username = EXCLUDED.username,
          password_hash = EXCLUDED.password_hash,
          email_verified = true,
          is_suspended = false,
          sessions_revoked_at = NULL,
          totp_enabled = false
      `,
      [TEST_USER_ID, TEST_USER.username, TEST_USER.email, TEST_USER_PASSWORD_HASH],
    );
  } finally {
    await db.end();
  }
}

/** Sign in through the API (storage-state bootstrap only; not a UI flow test). */
export async function signInViaApi(page: Page) {
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

/** Sign in through the sign-in form UI. */
export async function signInViaUi(
  page: Page,
  email: string = TEST_USER.email,
  password: string = TEST_USER.password,
) {
  await expect(async () => {
    await page.goto('/signin');
    await waitForClientHydration(page);
    await typeIntoFormField(page, '#email', email);
    await typeIntoFormField(page, '#password', password);

    // Artifact 29200471250: sign-in hydration could replace only the email
    // input after it was typed, while the later password input survived.
    // Re-enter through the UI when the controlled value was reset.
    const emailField = page.locator('#email');
    if ((await emailField.inputValue()) !== email) {
      await typeIntoFormField(page, '#email', email);
    }

    await expect(emailField).toHaveValue(email);
    await expect(page.locator('#password')).toHaveValue(password);
    await expect(page.getByRole('button', { name: 'サインイン' })).toBeEnabled();

    const loginResponse = page.waitForResponse(
      (response) =>
        response.url().includes('/v1/auth/login') &&
        (response.status() === 204 || response.status() === 200),
    );
    await page.getByRole('button', { name: 'サインイン' }).click();
    await loginResponse;
    await expect(page).not.toHaveURL(/\/signin/);
  }).toPass({ timeout: 30_000 });
}
