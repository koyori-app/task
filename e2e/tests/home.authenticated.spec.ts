import { test, expect } from '@playwright/test';

// Reuses the session saved by the auth.setup project. With a valid session the
// app root must not bounce an authenticated user back to /signin.
test('authenticated user is not redirected to signin', async ({ page }) => {
  await page.goto('/');
  await expect(page).not.toHaveURL(/\/signin/);
});
