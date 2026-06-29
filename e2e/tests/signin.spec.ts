import { test, expect } from '@playwright/test';

test('signin page renders', async ({ page }) => {
  await page.goto('/signin');
  await expect(page.locator('#email')).toBeVisible();
  await expect(page.locator('#password')).toBeVisible();
  await expect(page.getByRole('button', { name: 'サインイン' })).toBeVisible();
});
