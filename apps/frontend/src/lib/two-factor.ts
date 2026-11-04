/**
 * 2FA 画面（`/auth/2fa`）のパスと戻り先の規則。
 * 規則は docs/features/auth-2fa.md §5。
 */

export const TWO_FACTOR_PATH = '/auth/2fa';

/**
 * `redirect_after` が同一オリジンの相対パスであるときだけ返し、それ以外は `/` にする。
 * `/` 始まりで `//` 始まりでないこと。ブラウザは `\` を `/` と同じに扱うので
 * `/\evil.example` も `//evil.example` と同じ外部 URL になる。制御文字も受け付けない。
 * 2FA 画面そのものへ戻すと入力画面が繰り返されるので、それも `/` にする。
 */
export function safeRedirectAfter(raw: string | null | undefined): string {
  if (typeof raw !== 'string') return '/';
  if (!raw.startsWith('/') || raw.startsWith('//')) return '/';
  // eslint-disable-next-line no-control-regex
  if (/[\\\u0000-\u001f\u007f]/.test(raw)) return '/';
  const path = raw.split(/[?#]/, 1)[0];
  if (path === TWO_FACTOR_PATH || path.startsWith(`${TWO_FACTOR_PATH}/`)) return '/';
  return raw;
}

/** 2FA 画面への遷移先。戻り先は検証してから載せる。 */
export function twoFactorPath(redirectAfter?: string | null): string {
  const target = safeRedirectAfter(redirectAfter);
  return target === '/'
    ? TWO_FACTOR_PATH
    : `${TWO_FACTOR_PATH}?redirect_after=${encodeURIComponent(target)}`;
}

/** パスワードログインの 200 応答が 2FA の続きを要求しているか（204 は undefined）。 */
export function requiresTwoFactor(result: unknown): boolean {
  if (!result || typeof result !== 'object') return false;
  const r = result as { requires_2fa?: unknown; requires_2fa_setup?: unknown };
  return r.requires_2fa === true || r.requires_2fa_setup === true;
}
