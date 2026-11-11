// service worker の登録。+client.ts（client 専用の entry）から一度だけ呼ぶ。

export const SERVICE_WORKER_PATH = '/sw.js';

type RegisterEnv = {
  /** SSR では undefined。 */
  navigator: Navigator | undefined;
  /** vike dev では登録しない（下を見よ）。 */
  dev: boolean;
};

let registration: Promise<ServiceWorkerRegistration | undefined> | undefined;

/**
 * `/sw.js` を scope `/` で登録する。二度目以降の呼び出しは最初の登録を返す。
 * 登録に失敗しても reject しない（頁は SW 無しで動く）。undefined で解け、次の
 * 呼び出しで改めて試す。
 *
 * dev では登録しない。SW は何も cache しないので害は無いが、localhost の同じ port で
 * 別の物を立てた時にも登録が残り、DevTools の Application に紛らわしく出るため。
 */
export function registerServiceWorker(
  env: RegisterEnv = {
    navigator: typeof navigator === 'undefined' ? undefined : navigator,
    dev: import.meta.env.DEV,
  },
): Promise<ServiceWorkerRegistration | undefined> | undefined {
  if (registration) return registration;
  if (env.dev || !env.navigator || !('serviceWorker' in env.navigator)) return undefined;
  registration = env.navigator.serviceWorker
    .register(SERVICE_WORKER_PATH, { scope: '/' })
    .catch(() => {
      registration = undefined;
      return undefined;
    });
  return registration;
}

/** 試験のための後始末。 */
export function resetServiceWorkerRegistrationForTest() {
  registration = undefined;
}
