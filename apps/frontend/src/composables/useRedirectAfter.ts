import { usePageContext } from 'vike-vue/usePageContext';

/** サインイン後の戻り先を運ぶクエリ名。 */
export const REDIRECT_AFTER_PARAM = 'redirect_after';

/**
 * 戻り先として使ってよい値だけを返す。同一オリジンの相対パス（`/` 始まり）に限り、
 * `//host`・`/\host`・スキーム付き・制御文字入りは `/` にする（オープンリダイレクト対策）。
 *
 * OAuth 開始 API が受け付ける形にも揃える。backend の `sanitize_redirect_path`
 * （auth-core `crates/auth-core/src/state.rs`）は `:`・`@`・`..`・`\`・`//` 始まりを 400 で拒否するので、
 * ここで先に `/` へ落とす。パスワード経路と OAuth 経路で同じ値になり、OAuth だけ失敗することがない。
 */
export function safeRedirectPath(value: string | null | undefined): string {
  if (typeof value !== 'string' || value === '') return '/';
  if (!value.startsWith('/') || value.startsWith('//') || value.startsWith('/\\')) return '/';
  // ブラウザは URL からタブ・改行を取り除いてから解釈するので、`/\t/evil` が `//evil` になる
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f\\]/.test(value)) return '/';
  if (value.includes(':') || value.includes('@') || value.includes('..')) return '/';
  return value;
}

/** サインイン画面の URL。戻り先が `/` のときはクエリを付けない。 */
export function signInUrl(returnTo?: string | null): string {
  const target = safeRedirectPath(returnTo);
  return target === '/'
    ? '/signin'
    : `/signin?${REDIRECT_AFTER_PARAM}=${encodeURIComponent(target)}`;
}

/** 今開いているページ（path + search + hash）。クライアント専用。 */
export function currentLocationPath(): string {
  const { pathname, search, hash } = window.location;
  return `${pathname}${search}${hash}`;
}

/** 認証ガードがサインインへ送るときの URL。サインイン系の画面自身は戻り先にしない。 */
export function signInUrlForCurrentPage(): string {
  const { pathname } = window.location;
  if (pathname === '/signin' || pathname === '/signup') return '/signin';
  return signInUrl(currentLocationPath());
}

/**
 * 現在の URL の `redirect_after` を検証して返す。SSR とハイドレーションで値が食い違わないよう、
 * vike の pageContext を優先し、無いとき（テストなど）だけ window.location を読む。
 */
export function useRedirectAfter(): string {
  let raw: string | undefined;
  try {
    const ctx = usePageContext() as { urlParsed?: { search?: Record<string, string> } } | undefined;
    raw = ctx?.urlParsed?.search?.[REDIRECT_AFTER_PARAM];
  } catch {
    // pageContext の無い環境
  }
  if (raw === undefined && typeof window !== 'undefined') {
    raw = new URLSearchParams(window.location.search).get(REDIRECT_AFTER_PARAM) ?? undefined;
  }
  return safeRedirectPath(raw);
}

/** サインイン ↔ 登録の行き来で戻り先を持ち越すリンク。 */
export function withRedirectAfter(path: string, returnTo: string): string {
  return returnTo === '/'
    ? path
    : `${path}?${REDIRECT_AFTER_PARAM}=${encodeURIComponent(returnTo)}`;
}
