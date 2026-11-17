import { fetchClient } from '@/lib/api-vue-query';
import type { ProjectUuid, TenantUuid } from '@/lib/api-ids';

export const GET_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}' as const;

/** 詳細・カード・親タスク検索の共有取得。404 は null、他の失敗は HTTP status 付きに統一する。
 * 詳細と親タスク検索は signal を渡し、離れたタスクや打ち直した番号の取得を中断する。
 * カードは渡さず、閉じても取得を完了して共有キャッシュに残す。
 */
export async function fetchTask(
  params: { path: { tenant_id: TenantUuid; project_id: ProjectUuid; id: string } },
  signal?: AbortSignal,
) {
  const { data, error, response } = await fetchClient.GET(
    GET_TASK_PATH,
    signal ? { params, signal } : { params },
  );
  if (response.status === 404) return null;
  if (error) throw Object.assign(new Error(error.message), { status: response.status });
  return data;
}
