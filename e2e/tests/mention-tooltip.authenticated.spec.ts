import { expect, test } from '@playwright/test';
import path from 'node:path';

import { ensureMentionTask } from '../mention-fixture';

const SHOT_DIR =
  process.env.MENTION_SCREENSHOT_DIR ??
  path.join(import.meta.dirname, '../../memo/TASK-412-screenshots');

test.use({ serviceWorkers: 'block' });

async function openMentionOnComment(page: import('@playwright/test').Page) {
  const editor = page.locator('[data-comment-editor-input]').first();
  await editor.click();
  await page.keyboard.type('@e');
  const popup = page.locator('.cm-tooltip-autocomplete');
  await page.waitForTimeout(600);
  return { editor, popup };
}

test.describe('mention tooltip layer (TASK-412)', () => {
  test('課題詳細のコメント欄で候補が見える', async ({ page, request }, testInfo) => {
    test.setTimeout(120_000);
    const fixture = await ensureMentionTask(request);
    await page.goto(fixture.taskHref);
    await page.waitForLoadState('networkidle');

    const { popup } = await openMentionOnComment(page);

    const shotPath = path.join(
      SHOT_DIR,
      `${process.env.MENTION_SHOT_LABEL ?? 'after'}-${testInfo.project.name}-comment-light.png`,
    );
    await page.screenshot({ path: shotPath, fullPage: false });

    await expect(popup).toBeVisible({ timeout: 15_000 });

    const zIndex = await popup.evaluate((el) => getComputedStyle(el).zIndex);
    expect(Number(zIndex)).toBeGreaterThanOrEqual(10);

    const parentChain = await popup.evaluate((el) => {
      const chain: string[] = [];
      let node: Element | null = el;
      while (node) {
        chain.push(
          `${node.tagName.toLowerCase()}${node.className ? `.${String(node.className).slice(0, 40)}` : ''}`,
        );
        node = node.parentElement;
      }
      return chain.join(' < ');
    });
    expect(parentChain).toMatch(/body|dialog/i);
  });

  test('暗い型でも候補が見える', async ({ page, request }) => {
    test.setTimeout(120_000);
    const fixture = await ensureMentionTask(request);
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto(fixture.taskHref);
    await page.waitForLoadState('networkidle');
    await openMentionOnComment(page);
    const shotPath = path.join(
      SHOT_DIR,
      `${process.env.MENTION_SHOT_LABEL ?? 'after'}-chromium-authenticated-comment-dark.png`,
    );
    await page.screenshot({ path: shotPath, fullPage: false });
  });
});
