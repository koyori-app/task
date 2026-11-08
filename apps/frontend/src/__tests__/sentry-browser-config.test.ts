import * as Sentry from '@sentry/vue';
import type { Breadcrumb, ErrorEvent } from '@sentry/vue';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { App } from 'vue';
import {
  buildTracePropagationTargets,
  parseTracesSampleRate,
  scrubUrls,
  sentryBrowserConfig,
  stripUrlQuery,
} from '../../sentry.browser.config';

const sentryHooks = vi.hoisted(() => ({
  beforeAddBreadcrumb: [] as ((breadcrumb: Breadcrumb) => void)[],
  stopReplay: vi.fn(async () => {}),
}));

vi.mock('@sentry/vue', () => ({
  init: vi.fn(),
  attachErrorHandler: vi.fn(),
  isInitialized: vi.fn(() => false),
  browserTracingIntegration: vi.fn(() => ({ name: 'BrowserTracing' })),
  replayIntegration: vi.fn(() => ({ name: 'Replay' })),
  getClient: vi.fn(() => ({
    on: (hook: string, callback: (breadcrumb: Breadcrumb) => void) => {
      if (hook === 'beforeAddBreadcrumb') sentryHooks.beforeAddBreadcrumb.push(callback);
    },
  })),
  getReplay: vi.fn(() => ({ stop: sentryHooks.stopReplay })),
}));

const RESET_URL = 'https://task.example.com/auth/reset-password?token=secret-token#frag';

const app = { config: {} } as App;

const matches = (targets: RegExp[], url: string) => targets.some((target) => target.test(url));

describe('parseTracesSampleRate', () => {
  it.each([
    [undefined, 0.1],
    ['', 0.1],
    ['  ', 0.1],
    ['abc', 0.1],
    ['-0.1', 0.1],
    ['1.5', 0.1],
    ['0', 0],
    ['0.25', 0.25],
    ['1', 1],
  ])('%j -> %d', (raw, expected) => {
    expect(parseTracesSampleRate(raw)).toBe(expected);
  });
});

describe('buildTracePropagationTargets', () => {
  it('matches only the same-origin API proxy path by default', () => {
    const targets = buildTracePropagationTargets(undefined);
    expect(matches(targets, '/api/v1/tasks')).toBe(true);
    expect(matches(targets, '/apix/v1/tasks')).toBe(false);
    expect(matches(targets, '/tasks')).toBe(false);
    expect(matches(targets, 'https://other.example.com/api/v1/tasks')).toBe(false);
  });

  it('adds an absolute API base without matching lookalike hosts', () => {
    const targets = buildTracePropagationTargets('https://api.example.com/');
    expect(matches(targets, 'https://api.example.com/v1/tasks')).toBe(true);
    expect(matches(targets, 'https://api.example.com.evil.test/v1/tasks')).toBe(false);
    expect(matches(targets, 'https://apixexample.com/v1/tasks')).toBe(false);
  });

  it('does not duplicate the default target for the default API base', () => {
    expect(buildTracePropagationTargets('/api')).toHaveLength(1);
  });
});

describe('stripUrlQuery', () => {
  it.each([
    [RESET_URL, 'https://task.example.com/auth/reset-password'],
    ['/invitations/accept?token=abc', '/invitations/accept'],
    ['/verify-email#token=abc', '/verify-email'],
    ['/?token=abc', '/'],
    [
      'GET https://api.example.com/v1/x?token=abc failed',
      'GET https://api.example.com/v1/x failed',
    ],
    ['navigated to /signin?redirect=%2Finvitations%2Faccept%3Ftoken%3Dabc', 'navigated to /signin'],
    ['/projects/ENG/tasks', '/projects/ENG/tasks'],
    ['what? and/or?', 'what? and/or?'],
  ])('%j -> %j', (raw, expected) => {
    expect(stripUrlQuery(raw)).toBe(expected);
  });
});

describe('scrubUrls', () => {
  it('removes the token from errors, breadcrumbs and transactions', () => {
    const event = {
      message: `failed at ${RESET_URL}`,
      request: { url: RESET_URL, query_string: 'token=secret-token', headers: {} },
      breadcrumbs: [
        { category: 'navigation', data: { from: '/signin', to: '/auth/reset-password?token=x' } },
        { category: 'fetch', data: { url: '/api/v1/auth/x?token=secret-token' } },
      ],
      exception: {
        values: [{ value: 'boom', stacktrace: { frames: [{ filename: '/assets/a.js?v=1' }] } }],
      },
      spans: [
        {
          description: 'GET /api/v1/x?token=secret-token',
          data: { 'url.full': RESET_URL, 'http.query': '?token=secret-token', 'url.query': 'x' },
        },
      ],
      urls: [RESET_URL],
    };

    const scrubbed = scrubUrls(event);

    expect(JSON.stringify({ ...scrubbed, exception: undefined })).not.toContain('secret-token');
    expect(scrubbed.request).toEqual({
      url: 'https://task.example.com/auth/reset-password',
      headers: {},
    });
    expect(scrubbed.breadcrumbs[0]?.data.to).toBe('/auth/reset-password');
    expect(scrubbed.spans[0]?.data).toEqual({
      'url.full': 'https://task.example.com/auth/reset-password',
    });
    expect(scrubbed.urls).toEqual(['https://task.example.com/auth/reset-password']);
    // sourcemap の照合に使うスタックは触らない
    expect(scrubbed.exception.values[0]?.stacktrace.frames[0]?.filename).toBe('/assets/a.js?v=1');
  });
});

describe('sentryBrowserConfig', () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.clearAllMocks();
    sentryHooks.beforeAddBreadcrumb.length = 0;
    window.history.replaceState(null, '', '/');
  });

  const initOptions = () => {
    vi.stubEnv('PROD', true);
    sentryBrowserConfig(app);
    return vi.mocked(Sentry.init).mock.calls[0]?.[0];
  };

  it('scrubs the URL query in every send hook', async () => {
    const options = initOptions()!;
    const errorEvent = { request: { url: RESET_URL } } as ErrorEvent;
    type TransactionEvent = Parameters<NonNullable<typeof options.beforeSendTransaction>>[0];
    const transaction = { request: { url: RESET_URL } } as TransactionEvent;

    expect(options.beforeSend?.(errorEvent, {})).toMatchObject({
      request: { url: 'https://task.example.com/auth/reset-password' },
    });
    expect(options.beforeSendTransaction?.(transaction, {})).toMatchObject({
      request: { url: 'https://task.example.com/auth/reset-password' },
    });

    const integrations = options.integrations as { name: string; processEvent?: unknown }[];
    const scrubber = integrations.find((integration) => integration.name === 'ScrubUrls');
    const processEvent = scrubber?.processEvent as (event: unknown) => unknown;
    expect(processEvent({ type: 'replay_event', urls: [RESET_URL] })).toEqual({
      type: 'replay_event',
      urls: ['https://task.example.com/auth/reset-password'],
    });

    const replayOptions = vi.mocked(Sentry.replayIntegration).mock.calls[0]?.[0] ?? {};
    type RecordingEvent = Parameters<NonNullable<typeof replayOptions.beforeAddRecordingEvent>>[0];
    const recording = {
      type: 5,
      timestamp: 0,
      data: { tag: 'performanceSpan', payload: { description: RESET_URL } },
    } as unknown as RecordingEvent;
    expect(JSON.stringify(replayOptions?.beforeAddRecordingEvent?.(recording))).not.toContain(
      'secret-token',
    );
  });

  it('does not start Replay when the page is loaded on an auth page', () => {
    window.history.replaceState(null, '', '/auth/reset-password?token=secret-token');
    const options = initOptions();

    expect(Sentry.replayIntegration).not.toHaveBeenCalled();
    expect(options?.integrations).not.toContainEqual({ name: 'Replay' });
  });

  it('stops Replay before a client navigation into an auth page', () => {
    initOptions();
    expect(Sentry.replayIntegration).toHaveBeenCalledOnce();

    for (const hook of sentryHooks.beforeAddBreadcrumb) {
      hook({ category: 'navigation', data: { from: '/', to: '/projects' } });
    }
    expect(sentryHooks.stopReplay).not.toHaveBeenCalled();

    for (const hook of sentryHooks.beforeAddBreadcrumb) {
      hook({ category: 'navigation', data: { from: '/', to: '/invitations/accept?token=x' } });
    }
    expect(sentryHooks.stopReplay).toHaveBeenCalledOnce();
  });

  it('does not initialize outside production', () => {
    vi.stubEnv('PROD', false);
    sentryBrowserConfig(app);
    expect(Sentry.init).not.toHaveBeenCalled();
  });

  it('does not initialize twice', () => {
    vi.stubEnv('PROD', true);
    vi.mocked(Sentry.isInitialized).mockReturnValueOnce(true);
    sentryBrowserConfig(app);
    expect(Sentry.init).not.toHaveBeenCalled();
  });

  it('enables tracing from env and leaves release to the vite plugin injection', () => {
    vi.stubEnv('PROD', true);
    vi.stubEnv('PUBLIC_ENV__SENTRY_TRACES_SAMPLE_RATE', '0.5');
    vi.stubEnv('PUBLIC_ENV__SENTRY_ENVIRONMENT', 'staging');
    sentryBrowserConfig(app);

    const options = vi.mocked(Sentry.init).mock.calls[0]?.[0];
    expect(options?.app).toBe(app);
    expect(options).toHaveProperty('attachErrorHandler', false);
    expect(Sentry.browserTracingIntegration).toHaveBeenCalledOnce();
    expect(options?.integrations).toContainEqual({ name: 'BrowserTracing' });
    expect(options?.tracesSampleRate).toBe(0.5);
    expect(options?.environment).toBe('staging');
    expect(options).not.toHaveProperty('release');
  });
});
