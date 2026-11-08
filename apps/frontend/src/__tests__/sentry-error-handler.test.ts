import * as Sentry from '@sentry/vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue';
import { sentryBrowserConfig } from '../../sentry.browser.config';

/** SDK は実物を使い、送信直前のイベントを見る（送信先は fetch のスタブで止める）。 */
describe('Vue の例外の Sentry への送信', () => {
  let app: App;
  let shouldThrow: ReturnType<typeof ref<boolean>>;
  let events: Sentry.Event[];

  const triggerError = async () => {
    shouldThrow.value = true;
    // ハンドラーが無いとき SDK は再 throw し、開発ビルドの Vue はそれを flush の外へ投げる
    await nextTick().catch(() => {});
    // SDK は setTimeout の中で captureException する
    await new Promise((resolve) => setTimeout(resolve, 0));
    return events.filter((event) => event.exception?.values?.[0]?.value === 'boom');
  };

  beforeEach(() => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('{}')),
    );
    vi.stubEnv('PROD', true);
    vi.stubEnv('PUBLIC_ENV__SENTRY_DSN', 'https://public@sentry.invalid/1');
    vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.spyOn(console, 'warn').mockImplementation(() => {});

    events = [];
    shouldThrow = ref(false);
    const Child = defineComponent({
      setup: () => () => {
        if (shouldThrow.value) throw new Error('boom');
        return h('div');
      },
    });
    app = createApp(Child);
    sentryBrowserConfig(app);
    Sentry.getClient()?.on('beforeSendEvent', (event) => events.push(event));
    app.mount(document.createElement('div'));
  });

  afterEach(async () => {
    app.unmount();
    await Sentry.getClient()?.close();
    Sentry.getCurrentScope().setClient(undefined);
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('遷移前の例外を未処理として 1 度だけ送る', async () => {
    const captured = await triggerError();

    expect(captured).toHaveLength(1);
    expect(captured[0]?.exception?.values?.[0]?.mechanism).toMatchObject({
      handled: false,
      type: 'auto.function.vue.error_handler',
    });
  });

  it('vike-vue の changePage が errorHandler を上書きした後の例外も送り、上書きしたハンドラーも呼ぶ', async () => {
    // vike-vue@0.9.14 の changePage() がクライアント遷移のたびに行う代入と同じ形
    const vikeHandler = vi.fn();
    app.config.errorHandler = vikeHandler;
    // 2 回目の遷移でハンドラーが積み重ならないこと
    const latestVikeHandler = vi.fn();
    app.config.errorHandler = latestVikeHandler;

    const captured = await triggerError();

    expect(captured).toHaveLength(1);
    expect(captured[0]?.exception?.values?.[0]?.mechanism).toMatchObject({ handled: true });
    expect(latestVikeHandler).toHaveBeenCalledOnce();
    expect(latestVikeHandler.mock.calls[0]?.[0]).toMatchObject({ message: 'boom' });
    expect(vikeHandler).not.toHaveBeenCalled();
  });
});
