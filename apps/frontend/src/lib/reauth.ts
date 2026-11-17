import type { components } from '@/generated/api';
import { isKnownProvider, providerLabel } from '@/lib/oauth-providers';

export type ReauthStatus = components['schemas']['ReauthStatusResponse'];
export type ReauthOAuthMethod = components['schemas']['ReauthOAuthMethod'];

export const REAUTH_PATH = '/v1/auth/reauth';

/**
 * 切れる間際の印は、切れたとみなす。問うてから操作を撃つまでの間に切れると、連携の開始は
 * 全画面の遷移なので 403 の JSON の頁で止まる。
 */
export const REAUTH_MARGIN_MS = 15_000;

/** 再認証を求めずに操作を撃てるか。 */
export function isReauthFresh(status: ReauthStatus, nowMs: number): boolean {
  if (!status.fresh || !status.expires_at) return false;
  return Date.parse(status.expires_at) - nowMs > REAUTH_MARGIN_MS;
}

/**
 * 本人確認の後に続ける操作。連携で確かめると全画面で移るので、戻った画面で続けるために
 * sessionStorage に置く。パスワードなどの秘密は置かない。
 */
export type ReauthResume =
  | { kind: 'link'; provider: string; instanceUrl?: string }
  | { kind: 'set-password' };

/** 連携で確かめに移る前に置く印。続ける操作と、確かめに使った連携（失敗の知らせに使う）。 */
export type ReauthMarker = { resume: ReauthResume; via: string | null };

export function parseReauthMarker(raw: string | null): ReauthMarker | null {
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as {
      kind?: unknown;
      provider?: unknown;
      instanceUrl?: unknown;
      via?: unknown;
    };
    const via = typeof parsed.via === 'string' && isKnownProvider(parsed.via) ? parsed.via : null;
    if (parsed.kind === 'set-password') return { resume: { kind: 'set-password' }, via };
    if (parsed.kind === 'link' && typeof parsed.provider === 'string') {
      if (!isKnownProvider(parsed.provider)) return null;
      const resume: ReauthResume =
        typeof parsed.instanceUrl === 'string' && parsed.instanceUrl
          ? { kind: 'link', provider: parsed.provider, instanceUrl: parsed.instanceUrl }
          : { kind: 'link', provider: parsed.provider };
      return { resume, via };
    }
    return null;
  } catch {
    return null;
  }
}

/** dialog の説明に入れる操作の名（「〜前に、本人であることを確かめます」）。 */
export function reauthOperationLabel(resume: ReauthResume): string {
  if (resume.kind === 'set-password') return 'パスワードを設定する';
  return `${providerLabel(resume.provider)} の連携を追加する`;
}

/** 有効期間の長さの字（300 秒なら「5 分間」）。 */
export function reauthWindowLabel(windowSecs: number): string {
  if (windowSecs >= 60 && windowSecs % 60 === 0) return `${windowSecs / 60} 分間`;
  return `${windowSecs} 秒間`;
}

/** 連携の手段の表示名。self-hosted は instance も添えて見分ける。 */
export function reauthOAuthLabel(method: ReauthOAuthMethod): string {
  const label = providerLabel(method.provider);
  return method.instance_url ? `${label}（${method.instance_url}）` : label;
}
