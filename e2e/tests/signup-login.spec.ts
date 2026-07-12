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
  await expect(page.getByRole('heading', { name: 'アカウント作成' })).toBeVisible();

  const usernameInput = page.getByLabelText('ユーザー名');
  const emailInput = page.getByLabelText('メールアドレス');
  const passwordInput = page.getByLabelText('パスワード');

  await usernameInput.click();
  await usernameInput.pressSequentially(username, { delay: 20 });
  await emailInput.click();
  await emailInput.pressSequentially(email, { delay: 20 });
  await passwordInput.click();
  await passwordInput.pressSequentially(password, { delay: 20 });
  await passwordInput.blur();

  await expect(usernameInput).toHaveValue(username);
  await expect(emailInput).toHaveValue(email);
  await expect(passwordInput).toHaveValue(password);

  const submitButton = page.getByRole('button', { name: 'アカウント作成' });
  await expect(submitButton).toBeEnabled();

  const registerResponse = page.waitForResponse(
    (response) => response.url().includes('/v1/auth/register') && response.status() === 201,
    { timeout: 30_000 },
  );
  await submitButton.click();
  await registerResponse;

  await expect(page.getByRole('heading', { name: 'メールアドレスを確認してください' })).toBeVisible();
  await expect(page.getByText(email)).toBeVisible();

  const updatedRows = await setEmailVerified(email, DB_URL);
  expect(updatedRows).toBe(1);

  await signInViaUi(page, email, password);
});
