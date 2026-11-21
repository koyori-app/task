import type { components } from '@/generated/api';

export type AuthorizationParams = components['schemas']['AuthorizationParams'];
export type AuthorizationPreview = components['schemas']['AuthorizationPreviewResponse'];
export type OAuthGrant = components['schemas']['OAuthGrantResponse'];

/** 同意画面のパス。 */
const AUTHORIZE_PATH = '/oauth/authorize';
/** 保存した要求を指す query の名前。 */
export const RESUME_PARAM = 'resume';

const STASH_PREFIX = 'oauth-authorize:';
/**
 * 保存のキーに使う文字。`:`・`@`・`.` を含めない。サインインの戻り先の検査
 * （`safeRedirectPath` と backend の `sanitize_redirect_path`）に捨てられないため。
 */
const KEY_PATTERN = /^[A-Za-z0-9_-]+$/;

/**
 * 認可の要求（クライアントが付けてきた query）を読む。`client_id` か `redirect_uri` が無ければ null。
 * ほかの項目の誤りは、同意の API が理由を付けて断る。
 */
export function parseAuthorizeQuery(search: URLSearchParams): AuthorizationParams | null {
  const clientId = search.get('client_id');
  const redirectUri = search.get('redirect_uri');
  if (!clientId || !redirectUri) return null;
  return {
    client_id: clientId,
    redirect_uri: redirectUri,
    response_type: search.get('response_type') ?? '',
    code_challenge: search.get('code_challenge') ?? '',
    code_challenge_method: search.get('code_challenge_method') ?? '',
    state: search.get('state'),
    scope: search.get('scope'),
    resource: search.get('resource'),
  };
}

function randomKey(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return btoa(String.fromCharCode(...bytes))
    .replaceAll('+', '-')
    .replaceAll('/', '_')
    .replaceAll('=', '');
}

/**
 * 認可の要求の query を `sessionStorage` に置き、キーを返す。
 *
 * 要求の URL は `redirect_uri=https://…` や `scope=read:task` を含むので、そのままでは
 * サインインの戻り先にできない（`:` を含む値は捨てられる）。戻り先には
 * `/oauth/authorize?resume={キー}` を使い、中身はここから読む。
 */
export function stashAuthorizeQuery(search: string): string {
  const key = randomKey();
  try {
    window.sessionStorage.setItem(`${STASH_PREFIX}${key}`, search);
  } catch {
    // 保存できない環境では、読み出しが null になり、画面がやり直しを案内する
  }
  return key;
}

/**
 * 保存した query を読む。消さない。同じキーを、最初の表示、サインインから戻ったあと、
 * 再読み込み、失敗した承認のやり直しで、何度でも読めなければならない。
 */
export function readAuthorizeQuery(key: string): string | null {
  if (!KEY_PATTERN.test(key)) return null;
  try {
    return window.sessionStorage.getItem(`${STASH_PREFIX}${key}`);
  } catch {
    return null;
  }
}

/** 保存した query を消す。承認か拒否が成功して、クライアントへ遷移する直前にだけ呼ぶ。 */
export function removeAuthorizeQuery(key: string): void {
  if (!KEY_PATTERN.test(key)) return;
  try {
    window.sessionStorage.removeItem(`${STASH_PREFIX}${key}`);
  } catch {
    // 消せなくても、タブを閉じれば消える
  }
}

/** 保存した要求で同意画面を開き直す URL。サインインの戻り先に使える形である。 */
export function resumeUrl(key: string): string {
  return `${AUTHORIZE_PATH}?${RESUME_PARAM}=${key}`;
}

const ERROR_MESSAGES: Record<string, string> = {
  'oauth-client-not-allowed': 'このサーバーでは許可されていないクライアントです。',
  'oauth-invalid-client': 'クライアントの情報が正しくありません。',
  'oauth-client-metadata-unavailable':
    'クライアントの情報を取得できませんでした。時間をおいて、クライアントからやり直してください。',
  'oauth-invalid-redirect-uri': '戻り先の URL が、クライアントの登録と一致しません。',
  'oauth-unsupported-response-type': 'クライアントからの要求の形式に対応していません。',
  'oauth-invalid-code-challenge': 'クライアントからの要求に PKCE の値がありません。',
  'oauth-invalid-target': 'クライアントからの要求の宛先が、このサーバーではありません。',
  'oauth-invalid-scope': 'クライアントが求めている権限は、このサーバーでは渡せません。',
};

/** 同意の API の 400 の `message` を、画面に出す文にする。 */
export function authorizeErrorMessage(message: unknown): string {
  if (typeof message === 'string' && Object.hasOwn(ERROR_MESSAGES, message)) {
    return ERROR_MESSAGES[message]!;
  }
  return '接続の要求を確かめられませんでした。クライアントからやり直してください。';
}
