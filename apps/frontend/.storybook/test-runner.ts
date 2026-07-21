import { argosScreenshot } from '@argos-ci/playwright';
import type { TestRunnerConfig } from '@storybook/test-runner';

const VIEWPORT = { width: 1440, height: 900 };

const config: TestRunnerConfig = {
  async postVisit(page, context) {
    if (process.env.ARGOS_ENABLED !== 'true') return;

    await page.setViewportSize(VIEWPORT);
    await argosScreenshot(page, context.id, {
      fullPage: true,
    });
  },
};

export default config;
