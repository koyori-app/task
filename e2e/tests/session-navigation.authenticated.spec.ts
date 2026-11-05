import { test, expect } from '@playwright/test';

// WebKit と Firefox は遷移の開始時に通信中の fetch を reject し、旧ページの JS は
// 新しいページがコミットされるまで動き続ける。/v1/auth/me のこの失敗を未ログイン扱いにすると、
// 旧ページの location.assign('/signin') が進行中の遷移を上書きする。

// service worker がページを制御すると page.route が /v1/auth/me を捕まえられない回が出る
test.use({ serviceWorkers: 'block' });

test('/v1/auth/me の通信中に遷移しても /signin へ飛ばされない', async ({ page }) => {
  let meRequested!: () => void;
  const firstMeRequest = new Promise<void>((resolve) => (meRequested = resolve));
  let held = false;
  await page.route('**/api/v1/auth/me', async (route) => {
    if (held) return route.continue();
    // 最初の 1 回だけ応答を返さず、通信中のまま次の遷移を始める
    held = true;
    meRequested();
  });

  const signinRequests: string[] = [];
  page.on('request', (request) => {
    if (request.isNavigationRequest() && new URL(request.url()).pathname === '/signin') {
      signinRequests.push(request.url());
    }
  });

  await page.goto('/', { waitUntil: 'commit' });
  await firstMeRequest;

  await page.goto('/settings/profile');

  await expect(page).toHaveURL(/\/settings\/profile$/);
  await expect(page.locator('main')).toBeVisible();
  expect(signinRequests).toEqual([]);
});
