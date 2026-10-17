// build した物が Chromium で install の条件を満たすかを機械で確かめる（TASK-297）。
// 手順は docs/frontend/pwa.md の「確かめ方」。
//
//   node scripts/pwa-installability.mjs http://127.0.0.1:<port>/
//
// 見る物（DevTools > Application > Manifest と同じ物を CDP で読む）:
//   - Page.getInstallabilityErrors が空
//   - beforeinstallprompt が発火する
//   - service worker が /sw.js・scope / で activated になる
//   - Cache Storage が空（service worker は何も cache しない）
// どれかが外れたら exit 1。
//
// 既定の headless shell と incognito の文脈では install の判定が働かないので、
// 完全な Chromium（channel 'chromium'）を永続の profile で起こす。
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { chromium } from 'playwright';

const url = process.argv[2];
if (!url) {
  console.error('usage: node scripts/pwa-installability.mjs <url>');
  process.exit(2);
}

const profile = mkdtempSync(join(tmpdir(), 'pwa-installability-'));
const ctx = await chromium.launchPersistentContext(profile, {
  headless: true,
  channel: 'chromium',
});
try {
  await ctx.addInitScript(() => {
    window.__beforeInstallPrompt = false;
    window.addEventListener('beforeinstallprompt', () => {
      window.__beforeInstallPrompt = true;
    });
  });
  const page = ctx.pages()[0] ?? (await ctx.newPage());
  const cdp = await ctx.newCDPSession(page);
  await page.goto(url, { waitUntil: 'load' });
  // service worker の install と activate を待つ。
  await page.waitForFunction(() => Boolean(navigator.serviceWorker?.controller), null, {
    timeout: 10_000,
  });
  await page.waitForTimeout(3000);

  const { installabilityErrors } = await cdp.send('Page.getInstallabilityErrors');
  const manifest = await cdp.send('Page.getAppManifest');
  const beforeInstallPrompt = await page.evaluate(() => window.__beforeInstallPrompt);
  const workers = await page.evaluate(async () =>
    (await navigator.serviceWorker.getRegistrations()).map((r) => ({
      scope: r.scope,
      script: r.active?.scriptURL,
      state: r.active?.state,
    })),
  );
  await page.reload({ waitUntil: 'load' });
  const cacheNames = await page.evaluate(async () => caches.keys());

  const result = {
    url,
    manifestUrl: manifest.url,
    manifestErrors: manifest.errors,
    installabilityErrors,
    beforeInstallPrompt,
    workers,
    cacheNames,
  };
  console.log(JSON.stringify(result, null, 2));

  const origin = new URL(url).origin;
  const ok =
    installabilityErrors.length === 0 &&
    manifest.errors.length === 0 &&
    beforeInstallPrompt === true &&
    workers.some(
      (w) => w.scope === `${origin}/` && w.script === `${origin}/sw.js` && w.state === 'activated',
    ) &&
    cacheNames.length === 0;
  console.log(ok ? 'PWA check: OK' : 'PWA check: FAILED');
  process.exitCode = ok ? 0 : 1;
} finally {
  await ctx.close();
  rmSync(profile, { recursive: true, force: true });
}
