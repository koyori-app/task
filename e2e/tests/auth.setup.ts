import { test as setup, expect } from '@playwright/test';
import { seedUser, signInViaApi, STORAGE_STATE } from '../global-setup';

setup.setTimeout(60_000);

// storageState bootstrap: DB seed + API login (see signInViaApi in global-setup).
// UI login flow is covered by signup-login.spec.ts.
setup('sign in with email and password', async ({ page }) => {
  await seedUser();
  await signInViaApi(page);

  await expect(page).not.toHaveURL(/\/signin/);

  await page.context().storageState({ path: STORAGE_STATE });
});
