// https://vike.dev/onCreateApp
export { onCreateApp };

import type { PageContext } from 'vike/types';
import type { VueHeadClient } from '@unhead/vue';
import type { App } from 'vue';

type PageContextWithUnhead = PageContext & {
  _unhead?: VueHeadClient;
};

async function onCreateApp(pageContext: PageContext) {
  const pageContextWithUnhead = pageContext as PageContextWithUnhead;
  const app = pageContextWithUnhead.app!; //pageContext.app!

  pageContextWithUnhead._unhead ??= await createUnhead();
  app.use(pageContextWithUnhead._unhead);

  await initSentry(app);
}

// SDK の要件で app.mount() より前に init する（hydration 中のエラーと API 呼び出しも拾うため）。
// DSN が無いビルドでは SDK のチャンクごと読み込まない。監視の失敗でアプリを止めないよう、
// 広告ブロッカーなどでチャンクの取得に失敗しても起動は続ける。
async function initSentry(app: App): Promise<void> {
  if (import.meta.env.SSR || !import.meta.env.PUBLIC_ENV__SENTRY_DSN) return;

  try {
    const { sentryBrowserConfig } = await import('@/sentry.browser.config');
    sentryBrowserConfig(app);
  } catch (error) {
    console.error('Failed to initialize Sentry', error);
  }
}

async function createUnhead(): Promise<VueHeadClient> {
  if (import.meta.env.SSR) {
    const { createHead } = await import('@unhead/vue/server');

    // Vike already renders the default charset, viewport, and lang tags.
    return createHead({ disableDefaults: true });
  }

  const { createHead } = await import('@unhead/vue/client');
  return createHead();
}
