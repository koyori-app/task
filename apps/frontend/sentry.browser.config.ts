import * as Sentry from '@sentry/vue';
import type { App } from 'vue';
import { isAuthPagePath } from '@/lib/auth-pages';

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

/**
 * vike-vue はクライアント遷移のたびに `app.config.errorHandler` を代入し直すので、
 * SDK が init で付けたハンドラーは初回の遷移で失われる。代入のたびに SDK のラッパーで
 * 包み直し、遷移中の描画エラーも含めて常に捕捉する。init の `attachErrorHandler` は切ること
 * （残すと SDK の代入がここを通って二重に包まれる）。
 */
export const keepSentryErrorHandler = (app: App) => {
  const wrap = (errorHandler: App['config']['errorHandler']) => {
    const target = { config: { errorHandler }, mixin: () => {} };
    Sentry.attachErrorHandler(target);
    return target.config.errorHandler;
  };

  let wrapped = wrap(app.config.errorHandler);
  Object.defineProperty(app.config, 'errorHandler', {
    configurable: true,
    enumerable: true,
    get: () => wrapped,
    set: (errorHandler: App['config']['errorHandler']) => {
      wrapped = wrap(errorHandler);
    },
  });
};

// 絶対 URL と、先頭か区切りの直後に来る相対パスの、クエリとフラグメントを落とす。
// `/auth/reset-password?token=…` のように、URL に載った秘密を監視基盤へ渡さないため。
const ABSOLUTE_URL_SUFFIX = /\b(https?:\/\/[^\s"'<>?#]*)[?#][^\s"'<>]*/g;
const RELATIVE_URL_SUFFIX = /(^|[\s("'=,])(\/[^\s"'<>?#]*)[?#][^\s"'<>]*/g;
// クエリとフラグメントだけを持つ属性（SDK の span 属性・HttpContext の request）
const QUERY_KEYS = new Set([
  'query_string',
  'http.query',
  'url.query',
  'http.fragment',
  'url.fragment',
]);
// スタックのファイル名は sourcemap の照合に使うので触らない（本番のバンドルはクエリを持たない）
const SKIPPED_KEYS = new Set(['stacktrace', 'debug_meta']);

export const stripUrlQuery = (value: string): string =>
  value.replace(ABSOLUTE_URL_SUFFIX, '$1').replace(RELATIVE_URL_SUFFIX, '$1$2');

const scrubInPlace = (value: unknown, seen: WeakSet<object>): unknown => {
  if (typeof value === 'string') return stripUrlQuery(value);
  if (value === null || typeof value !== 'object' || seen.has(value)) return value;
  seen.add(value);
  if (Array.isArray(value)) {
    value.forEach((item, index) => {
      value[index] = scrubInPlace(item, seen);
    });
    return value;
  }
  const record = value as Record<string, unknown>;
  for (const key of Object.keys(record)) {
    if (QUERY_KEYS.has(key)) delete record[key];
    else if (!SKIPPED_KEYS.has(key)) record[key] = scrubInPlace(record[key], seen);
  }
  return record;
};

/**
 * 送信物の中の URL からクエリとフラグメントを落とす（エラー・トランザクション・breadcrumb・
 * Replay のイベント）。HttpContext は現在の完全な URL を `request.url` に入れ、
 * 遷移や fetch の breadcrumb・span も URL をそのまま持つため、文字列はすべて見る。
 */
export const scrubUrls = <T>(payload: T): T => scrubInPlace(payload, new WeakSet()) as T;

/**
 * 送信前の処理。`beforeSend` はエラーにしか、`beforeSendTransaction` はトランザクションにしか
 * 掛からず、Replay のイベント（`urls`）には掛からないので、全種のイベントが通る
 * `processEvent` でも落とす。
 */
const scrubUrlsIntegration = () => ({
  name: 'ScrubUrls',
  processEvent: <T>(event: T) => scrubUrls(event),
});

/**
 * Replay の記録はサインイン前のページでは止める。記録の先頭に入るスナップショットは
 * `location.href` を丸ごと持ち、`beforeAddRecordingEvent` では書き換えられない。
 * 全面読み込みは init で、クライアント遷移は遷移の breadcrumb（履歴の書き換えの前に同期で出る）で止める。
 */
const stopReplayOnAuthPages = () => {
  Sentry.getClient()?.on('beforeAddBreadcrumb', (breadcrumb) => {
    const to = breadcrumb.category === 'navigation' ? breadcrumb.data?.to : undefined;
    if (typeof to !== 'string') return;
    if (isAuthPagePath(new URL(to, window.location.origin).pathname)) {
      void Sentry.getReplay()?.stop();
    }
  });
};

export const sentryBrowserConfig = (app: App) => {
  // Replay は多重に起動すると SDK が例外を投げるので、init は 1 度だけにする
  if (import.meta.env.PROD && !Sentry.isInitialized()) {
    // `release` は渡さない。sentryVitePlugin が自身の release.name を SENTRY_RELEASE として
    // バンドルへ注入し、SDK がそれを既定値に使うので、sourcemap の release と必ず一致する。
    keepSentryErrorHandler(app);
    Sentry.init({
      app,
      attachErrorHandler: false,
      dsn: import.meta.env.PUBLIC_ENV__SENTRY_DSN,
      environment: import.meta.env.PUBLIC_ENV__SENTRY_ENVIRONMENT || import.meta.env.MODE,
      // browserTracingIntegration が無いと fetch / XHR に trace ヘッダが付かない
      integrations: [
        Sentry.browserTracingIntegration(),
        ...(isAuthPagePath(window.location.pathname)
          ? []
          : [
              Sentry.replayIntegration({
                // 遷移・fetch の span と breadcrumb は記録に入る前にここを通る
                beforeAddRecordingEvent: (event) => scrubUrls(event),
              }),
            ]),
        scrubUrlsIntegration(),
      ],
      beforeSend: (event) => scrubUrls(event),
      beforeSendTransaction: (event) => scrubUrls(event),
      tracesSampleRate: parseTracesSampleRate(
        import.meta.env.PUBLIC_ENV__SENTRY_TRACES_SAMPLE_RATE,
      ),
      tracePropagationTargets: buildTracePropagationTargets(import.meta.env.VITE_API_BASE),
      replaysSessionSampleRate: 0.1,
      replaysOnErrorSampleRate: 1.0,
    });
    stopReplayOnAuthPages();
  }
};
