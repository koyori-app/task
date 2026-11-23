import * as Sentry from '@sentry/vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp, defineComponent, h } from 'vue';
import { sentryBrowserConfig } from '../../sentry.browser.config';

/**
 * SDK は実物を使い、送信直前のイベントを見る（送信先は fetch のスタブで止める）。
 * SDK を上げても、TASK-483 の決め（URL のクエリとフラグメントを送らない・認証の頁で Replay を
 * 撮らない）と、v10 と同じ集め方が崩れないことを縛る。
 */
describe('Sentry へ送る中身', () => {
  let events: Sentry.Event[];

  const init = (path: string) => {
    window.history.replaceState(null, '', path);
    sentryBrowserConfig(createApp(defineComponent({ setup: () => () => h('div') })));
    Sentry.getClient()?.on('beforeSendEvent', (event) => events.push(event));
  };

  beforeEach(() => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('{}')),
    );
    vi.stubEnv('PROD', true);
    vi.stubEnv('PUBLIC_ENV__SENTRY_DSN', 'https://public@sentry.invalid/1');
    vi.stubEnv('PUBLIC_ENV__SENTRY_TRACES_SAMPLE_RATE', '1');
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    events = [];
  });

  afterEach(async () => {
    await Sentry.getClient()?.close();
    Sentry.getCurrentScope().setClient(undefined);
    window.history.replaceState(null, '', '/');
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('エラーの URL からクエリとフラグメントを落とし、IP の推定を頼まない', async () => {
    init('/projects?token=secret-token#frag');
    Sentry.captureException(new Error('boom'));
    await Sentry.flush();

    const [event] = events.filter((sent) => sent.exception);
    expect(event?.request?.url).toBe(`${window.location.origin}/projects`);
    expect(JSON.stringify(event)).not.toContain('secret-token');
    // SDK 11 は dataCollection.userInfo が既定の true だと、送るときに IP の推定（'auto'）を頼む
    expect(event?.sdk?.settings?.infer_ip).not.toBe('auto');
  });

  it('認証の頁で読み込んだら Replay を起こさない', () => {
    init('/auth/reset-password?token=secret-token');

    expect(Sentry.getReplay()).toBeUndefined();
  });

  it('ほかの頁では Replay を起こし、認証の頁への遷移で止める', () => {
    init('/projects');
    const replay = Sentry.getReplay();
    expect(replay).toBeDefined();
    const stop = vi.spyOn(replay!, 'stop');

    Sentry.addBreadcrumb({ category: 'navigation', data: { from: '/projects', to: '/tasks' } });
    expect(stop).not.toHaveBeenCalled();

    Sentry.addBreadcrumb({
      category: 'navigation',
      data: { from: '/projects', to: '/invitations/accept?token=x' },
    });
    expect(stop).toHaveBeenCalledOnce();
  });
});
