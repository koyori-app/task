import { test as setup, expect } from '@playwright/test';
import { seedUser, signInViaUi, STORAGE_STATE } from '../global-setup';

setup.setTimeout(60_000);

// Real login happy-path: register + verify a user, sign in through the UI, and
// assert the app redirects away from /signin. Doubles as the storageState
// producer for the "authenticated" project (see playwright.config.ts).
setup('sign in with email and password', async ({ page }) => {
  await seedUser();
  await signInViaUi(page);

  await expect(page).not.toHaveURL(/\/signin/);

  await page.context().storageState({ path: STORAGE_STATE });
});
