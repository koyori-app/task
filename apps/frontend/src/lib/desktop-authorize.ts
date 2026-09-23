import { codePointLength } from '@/lib/code-points';

/**
 * Koyori Desktop の承認画面（`/desktop/authorize`）のクエリ検証。
 * 規則は apps/backend/docs/personal-access-tokens-authz.md の「Desktop 認証」。
 */

export type DesktopAuthorizeRequest = {
  port: number;
  codeChallenge: string;
  state: string;
  name: string;
};

/** backend の `PKCE_CHALLENGE_REGEX` と同じ（S256 の base64url、43〜128 文字）。 */
const CODE_CHALLENGE = /^[A-Za-z0-9_-]{43,128}$/;
/** backend の端末名の上限。 */
export const DEVICE_NAME_MAX = 100;

/**
 * クエリを検証する。不正なら null。
 *
 * `port` は 1024〜65535 の整数に限る。承認後の遷移先は常に
 * `http://127.0.0.1:{port}/callback` で、ホストや経路をクエリから受け取らないので
 * 外部サイトへは飛ばせない。
 */
export function parseDesktopAuthorizeQuery(
  search: URLSearchParams,
): DesktopAuthorizeRequest | null {
  const rawPort = search.get('port') ?? '';
  if (!/^\d{4,5}$/.test(rawPort)) return null;
  const port = Number(rawPort);
  if (port < 1024 || port > 65535) return null;

  const codeChallenge = search.get('code_challenge') ?? '';
  if (!CODE_CHALLENGE.test(codeChallenge)) return null;

  const state = search.get('state') ?? '';
  if (state === '') return null;

  const name = (search.get('name') ?? '').trim() || 'Koyori Desktop';
  if (codePointLength(name) > DEVICE_NAME_MAX) return null;

  return { port, codeChallenge, state, name };
}

/** 承認後に Desktop へ戻す loopback の URL。 */
export function desktopCallbackUrl(port: number, code: string, state: string): string {
  const url = new URL(`http://127.0.0.1:${port}/callback`);
  url.searchParams.set('code', code);
  url.searchParams.set('state', state);
  return url.toString();
}

/** サインイン後にこの画面へ戻すための印（`lib/one-time-notice.ts`）。値は戻り先のパス。 */
export const DESKTOP_AUTHORIZE_RETURN = 'task:desktop-authorize-return';

/** 印の値が承認画面のパスであるときだけ戻り先として使う。 */
export function desktopAuthorizeReturnPath(value: string | null): string | null {
  return value?.startsWith('/desktop/authorize?') ? value : null;
}
