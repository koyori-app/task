import { fetchClient } from '@/lib/api-vue-query';
import type { ProjectUuid, TenantUuid } from '@/lib/api-ids';

export const GET_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}' as const;

/** 詳細とカードの共有取得。404 は null、他の失敗は HTTP status 付きに統一する。
 * AbortSignal は渡さず、カードを閉じても取得を完了して共有キャッシュに残す。
 */
export async function fetchTask(params: {
  path: { tenant_id: TenantUuid; project_id: ProjectUuid; id: string };
}) {
  const { data, error, response } = await fetchClient.GET(GET_TASK_PATH, { params });
  if (response.status === 404) return null;
  if (error) throw Object.assign(new Error(error.message), { status: response.status });
  return data;
}
