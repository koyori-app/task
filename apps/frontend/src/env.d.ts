declare module '*.vue' {
  import type { DefineComponent } from 'vue';
  const component: DefineComponent<object, object, any>;
  export default component;
}

declare module '*.css';

interface ImportMetaEnv {
  /** 0〜1。未設定なら 0.1。vike build でバンドルへ埋め込まれる */
  readonly PUBLIC_ENV__SENTRY_TRACES_SAMPLE_RATE?: string;
  /** 未設定ならビルドの mode（本番ビルドは production）。vike build でバンドルへ埋め込まれる */
  readonly PUBLIC_ENV__SENTRY_ENVIRONMENT?: string;
}
