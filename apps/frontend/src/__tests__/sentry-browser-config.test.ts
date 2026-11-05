import * as Sentry from '@sentry/vue';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { App } from 'vue';
import {
  buildTracePropagationTargets,
  parseTracesSampleRate,
  sentryBrowserConfig,
} from '../../sentry.browser.config';

vi.mock('@sentry/vue', () => ({
  init: vi.fn(),
  attachErrorHandler: vi.fn(),
  isInitialized: vi.fn(() => false),
  browserTracingIntegration: vi.fn(() => ({ name: 'BrowserTracing' })),
  replayIntegration: vi.fn(() => ({ name: 'Replay' })),
}));

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

describe('sentryBrowserConfig', () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.clearAllMocks();
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
