// Shared e2e auth fixtures and helpers.
//
// Consumed by tests/auth.setup.ts (the Playwright "setup" project) rather than
// wired as a legacy `globalSetup` file: the webServer entries in
// playwright.config.ts are guaranteed to be running before any project's tests,
// so seeding and the UI login below always have the dev/api servers available.
import { type Page, request as playwrightRequest } from '@playwright/test';
import { Client } from 'pg';
import path from 'node:path';

const API_URL = process.env.API_URL ?? 'http://localhost:3400';
const DB_URL = process.env.E2E_DATABASE_URL ?? 'postgresql://test:test@localhost:5432/task_e2e';

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
  const ctx = await playwrightRequest.newContext({ baseURL: API_URL });
  await ctx.post('/v1/auth/register', {
    data: {
      username: TEST_USER.username,
      email: TEST_USER.email,
      password: TEST_USER.password,
    },
  });
  await ctx.dispose();

  const db = new Client({ connectionString: DB_URL });
  await db.connect();
  await db.query('UPDATE users SET email_verified = true WHERE email = $1', [TEST_USER.email]);
  await db.end();
}

/** Drive the sign-in form and wait for the redirect away from /signin. */
export async function signIn(page: Page) {
  await page.goto('/signin');
  await page.getByTestId('signin-form').and(page.locator('[data-hydrated="true"]')).waitFor();
  await page.fill('#email', TEST_USER.email);
  await page.fill('#password', TEST_USER.password);
  await page.getByRole('button', { name: 'サインイン' }).click();
  await page.waitForURL((url) => !url.pathname.includes('/signin'), { timeout: 10_000 });
}
