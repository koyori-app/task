import { afterEach, describe, expect, it, vi } from 'vitest';
import type { PageContext } from 'vike/types';
import { sentryBrowserConfig } from '@/sentry.browser.config';
import { onCreateApp } from '../pages/+onCreateApp';

vi.mock('@/sentry.browser.config', () => ({ sentryBrowserConfig: vi.fn() }));

const createPageContext = () => {
  const app = { use: vi.fn() };
  return { app, pageContext: { app, _unhead: {} } as unknown as PageContext };
};

describe('+onCreateApp の Sentry 配線', () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.clearAllMocks();
    vi.restoreAllMocks();
  });

  it('ブラウザでは unhead の後に app を渡して初期化する', async () => {
    vi.stubEnv('SSR', false);
    vi.stubEnv('PUBLIC_ENV__SENTRY_DSN', 'https://public@sentry.invalid/1');
    const { app, pageContext } = createPageContext();

    await onCreateApp(pageContext);

    expect(sentryBrowserConfig).toHaveBeenCalledExactlyOnceWith(app);
    expect(app.use.mock.invocationCallOrder[0]).toBeLessThan(
      vi.mocked(sentryBrowserConfig).mock.invocationCallOrder[0]!,
    );
  });

  it('SSR では初期化しない', async () => {
    vi.stubEnv('SSR', true);
    vi.stubEnv('PUBLIC_ENV__SENTRY_DSN', 'https://public@sentry.invalid/1');
    const { app, pageContext } = createPageContext();

    await onCreateApp(pageContext);

    expect(app.use).toHaveBeenCalledOnce();
    expect(sentryBrowserConfig).not.toHaveBeenCalled();
  });

  it('DSN が無ければ初期化しない', async () => {
    vi.stubEnv('SSR', false);
    vi.stubEnv('PUBLIC_ENV__SENTRY_DSN', '');
    const { pageContext } = createPageContext();

    await onCreateApp(pageContext);

    expect(sentryBrowserConfig).not.toHaveBeenCalled();
  });

  it('初期化に失敗してもアプリの起動を止めない', async () => {
    vi.stubEnv('SSR', false);
    vi.stubEnv('PUBLIC_ENV__SENTRY_DSN', 'https://public@sentry.invalid/1');
    const failure = new Error('chunk load failed');
    vi.mocked(sentryBrowserConfig).mockImplementationOnce(() => {
      throw failure;
    });
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
    const { pageContext } = createPageContext();

    await expect(onCreateApp(pageContext)).resolves.toBeUndefined();
    expect(consoleError).toHaveBeenCalledWith('Failed to initialize Sentry', failure);
  });
});
