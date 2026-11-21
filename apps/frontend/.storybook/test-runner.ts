import { argosScreenshot } from '@argos-ci/playwright';
import { getStoryContext, type TestRunnerConfig } from '@storybook/test-runner';
import { MINIMAL_VIEWPORTS } from 'storybook/viewport';

const VIEWPORT = { width: 1440, height: 900 };

type ViewportOptions = Record<string, { styles?: { width?: string; height?: string } }>;

/**
 * ストーリーが viewport を指定していれば、その幅と高さ（px）。
 * test-runner は iframe.html を直接開くので、`globals.viewport` はそのままでは画面幅に効かない。
 * 狭い幅の media query を前提にした play（TaskTable の TableNarrowHitArea など）のために、ここで反映する。
 */
function storyViewport(story: {
  storyGlobals?: { viewport?: { value?: string } };
  parameters?: { viewport?: { defaultViewport?: string; options?: ViewportOptions } };
}) {
  const name = story.storyGlobals?.viewport?.value ?? story.parameters?.viewport?.defaultViewport;
  if (!name) return undefined;
  const options: ViewportOptions = { ...MINIMAL_VIEWPORTS, ...story.parameters?.viewport?.options };
  const width = Number.parseInt(options[name]?.styles?.width ?? '', 10);
  const height = Number.parseInt(options[name]?.styles?.height ?? '', 10);
  return Number.isFinite(width) && Number.isFinite(height) ? { width, height } : undefined;
}

const config: TestRunnerConfig = {
  async preVisit(page, context) {
    const viewport = storyViewport(await getStoryContext(page, context));
    if (viewport) {
      await page.setViewportSize(viewport);
    } else if (process.env.ARGOS_ENABLED === 'true') {
      await page.setViewportSize(VIEWPORT);
    }
  },
  async postVisit(page, context) {
    if (process.env.ARGOS_ENABLED !== 'true') return;

    await argosScreenshot(page, context.id, {
      fullPage: true,
    });
  },
};

export default config;
