import * as Sentry from '@sentry/vue';
import { getCurrentInstance } from 'vue';

const DEFAULT_TRACES_SAMPLE_RATE = 0.1;

/** 未設定・空文字（Docker の空 ARG）・0〜1 の範囲外は既定値に倒す。`0` は無効化として通す。 */
export const parseTracesSampleRate = (raw: string | undefined): number => {
  if (!raw?.trim()) return DEFAULT_TRACES_SAMPLE_RATE;
  const rate = Number(raw);
  return rate >= 0 && rate <= 1 ? rate : DEFAULT_TRACES_SAMPLE_RATE;
};

/**
 * `sentry-trace` / `baggage` を付ける宛先。バックエンド以外へ trace を漏らさないよう先頭一致に絞る。
 * SDK は同一オリジンの URL だけ pathname でも照合するので、`/api/` は同一オリジンのプロキシにしか当たらない。
 * `apiBase` は API クライアントと同じ `VITE_API_BASE`（別オリジンの API を直接指す場合がある）。
 */
export const buildTracePropagationTargets = (apiBase: string | undefined): RegExp[] => {
  const targets = [/^\/api\//];
  const base = apiBase?.trim().replace(/\/+$/, '');
  if (base && base !== '/api') {
    const escaped = base.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&');
    targets.push(new RegExp(`^${escaped}(?:/|$)`));
  }
  return targets;
};

export const sentryBrowserConfig = () => {
  if (import.meta.env.PROD) {
    // `release` は渡さない。sentryVitePlugin が自身の release.name を SENTRY_RELEASE として
    // バンドルへ注入し、SDK がそれを既定値に使うので、sourcemap の release と必ず一致する。
    Sentry.init({
      app: getCurrentInstance()?.appContext.app,
      dsn: import.meta.env.PUBLIC_ENV__SENTRY_DSN,
      environment: import.meta.env.PUBLIC_ENV__SENTRY_ENVIRONMENT || import.meta.env.MODE,
      // browserTracingIntegration が無いと fetch / XHR に trace ヘッダが付かない
      integrations: [Sentry.browserTracingIntegration(), Sentry.replayIntegration()],
      tracesSampleRate: parseTracesSampleRate(
        import.meta.env.PUBLIC_ENV__SENTRY_TRACES_SAMPLE_RATE,
      ),
      tracePropagationTargets: buildTracePropagationTargets(import.meta.env.VITE_API_BASE),
      replaysSessionSampleRate: 0.1,
      replaysOnErrorSampleRate: 1.0,
    });
  }
};
