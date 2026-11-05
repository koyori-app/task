/**
 * サインイン前（または半認証のセッション）で開くページ。
 * `+Layout.vue` はここで認証ガードを外し、Sentry の Replay はここでは記録しない
 * （再設定・招待・メール確認のトークンや、デスクトップ連携の state が URL に載るため）。
 */
export const AUTH_PAGE_PATHS: readonly string[] = [
  '/signin',
  '/signup',
  '/auth/reset-password',
  // 半認証のセッションでも開けなければならない（ここで 2FA を終えるため）
  '/auth/2fa',
  '/verify-email',
  '/desktop/authorize',
  '/invitations/accept',
];

/** 末尾のスラッシュの有無は区別しない。 */
export const isAuthPagePath = (pathname: string): boolean =>
  AUTH_PAGE_PATHS.includes(pathname.replace(/\/+$/, '') || '/');
