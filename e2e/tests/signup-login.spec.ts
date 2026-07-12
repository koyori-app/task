import { expect, test } from '@playwright/test';
import { ensureRegistrationEnabled, signInViaUi } from '../global-setup';
import { setEmailVerified } from '../scripts/verify-email';

const DB_URL = process.env.E2E_DATABASE_URL ?? 'postgresql://test:test@localhost:5432/task_e2e';

test('user can sign up, verify email, and sign in', async ({ page }) => {
  test.setTimeout(90_000);

  const unique = Date.now();
  const username = `e2euser${unique}`;
  const email = `e2e.signup.${unique}@example.com`;
  const password = 'E2ePassword1!';

  await ensureRegistrationEnabled(DB_URL);

  await page.goto('/signup');
  await page.locator('#username').fill(username);
  await page.locator('#username').blur();
  await page.locator('#email').fill(email);
  await page.locator('#email').blur();
  await page.locator('#password').fill(password);
  await page.locator('#password').blur();
  await expect(page.getByRole('button', { name: 'アカウント作成' })).toBeEnabled();

  const registerResponse = page.waitForResponse(
    (response) => response.url().includes('/v1/auth/register') && response.status() === 201,
    { timeout: 30_000 },
  );
  await page.getByRole('button', { name: 'アカウント作成' }).click();
  await registerResponse;

  await expect(page.getByRole('heading', { name: 'メールアドレスを確認してください' })).toBeVisible();
  await expect(page.getByText(email)).toBeVisible();

  const updatedRows = await setEmailVerified(email, DB_URL);
  expect(updatedRows).toBe(1);

  await signInViaUi(page, email, password);
});
