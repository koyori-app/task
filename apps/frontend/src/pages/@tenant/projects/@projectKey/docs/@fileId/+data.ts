// ドキュメント詳細の server data hook。タスク詳細の tasks/@taskId/+data.ts と同じ形で、
// 本文の KFM 描画はここ (サーバ) で一度だけ行い、描画元テキストと対で返す。
// 表示側は描画元が最新の本文と厳密一致するときだけ v-html する (stale 照合)。
import type { PageContextServer } from 'vike/types';

import { API_BASE } from '#/api-base';
import type { components } from '@/generated/api';
import { renderDescription } from '@/lib/markup-renderer';
import { documentTitle, isDocumentFile } from '@/lib/documents/name';

type TenantResponse = components['schemas']['TenantResponse'];
type DriveFile = components['schemas']['DriveFileResponse'];

export type Data = {
  /** 本文の KFM 描画。取得失敗・空・上限超過は null（表示側はプレーン表示か再描画へ倒す） */
  descriptionHtml: string | null;
  /** renderDescription へ渡した入力そのもの（stale 照合キー） */
  descriptionSource: string | null;
  /** ページタイトル用。取れなければ null */
  title: string | null;
};

const EMPTY: Data = { descriptionHtml: null, descriptionSource: null, title: null };

/** tasks/@taskId/+data.ts と同じ上限・同じ理由（CPU とキャッシュのメモリが本文長に比例） */
const MAX_DESCRIPTION_LENGTH = 65_536;
/** backend の取得待ちの共有予算（tasks/@taskId/+data.ts と同じ） */
const SSR_FETCH_BUDGET_MS = 3000;
/** scope に入れるので UUID だけを受ける（scope の字種制約を常に満たす） */
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

async function fetchOk(url: string, cookie: string | undefined, signal: AbortSignal) {
  const response = await fetch(url, { headers: cookie ? { cookie } : undefined, signal });
  return response.ok ? response : null;
}

function toTenantList(data: unknown): TenantResponse[] {
  if (Array.isArray(data)) return data as TenantResponse[];
  const tenants = (data as { tenants?: TenantResponse[] } | null)?.tenants;
  return Array.isArray(tenants) ? tenants : [];
}

export async function data(pageContext: PageContextServer): Promise<Data> {
  const tenantDisplayId = String(pageContext.routeParams?.tenant ?? '');
  const fileId = String(pageContext.routeParams?.fileId ?? '');
  const cookie = pageContext.headers?.cookie;
  if (!tenantDisplayId || !UUID_RE.test(fileId)) return EMPTY;

  // 失敗は throw せず EMPTY へ倒す。KFM 表示は付加価値で、ページ本体はクライアントの取得で出る
  const signal = AbortSignal.timeout(SSR_FETCH_BUDGET_MS);
  try {
    const tenantsResponse = await fetchOk(`${API_BASE}/v1/tenants`, cookie, signal);
    const tenantId = toTenantList(await tenantsResponse?.json()).find(
      (tenant) => tenant.display_id === tenantDisplayId,
    )?.id;
    if (!tenantId) return EMPTY;

    const [metaResponse, contentResponse] = await Promise.all([
      fetchOk(`${API_BASE}/v1/tenants/${tenantId}/drive/files/${fileId}`, cookie, signal),
      fetchOk(`${API_BASE}/v1/drive/files/${fileId}/content`, cookie, signal),
    ]);
    const meta = (await metaResponse?.json()) as DriveFile | undefined;
    // 表示側（DocumentDetail）と同じ条件。ルート直下でも .md 以外はドキュメントとして描画しない
    if (!meta || !isDocumentFile(meta)) return EMPTY;
    const title = documentTitle(meta.name);
    const body = contentResponse ? await contentResponse.text() : '';
    if (!body || body.length > MAX_DESCRIPTION_LENGTH) return { ...EMPTY, title };

    return {
      descriptionHtml: await renderDescription(body, {
        scope: `doc-${fileId}`,
        taskRef: { tenant: tenantDisplayId },
      }),
      descriptionSource: body,
      title,
    };
  } catch (error) {
    console.error('[document-data] SSR data failed', error);
    return EMPTY;
  }
}
