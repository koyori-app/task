import type { QueryClient } from '@tanstack/vue-query';

import { TASK_SEARCH_PATH } from '@/lib/api-vue-query';

const TASKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;
const TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}' as const;
const SCHEDULE_PATH = '/v1/tenants/{tenant_id}/schedule' as const;

/**
 * 種別の名前・色の変更や削除のあと、タスクに埋め込まれた種別が古くなるキャッシュを取り直す。
 * 一覧・詳細・検索・カレンダーが同じタスクを別々のキーで持つ。
 */
export function invalidateTaskCachesAfterTaskTypeChange(queryClient: QueryClient) {
  return Promise.all(
    [TASKS_PATH, TASK_PATH, TASK_SEARCH_PATH, SCHEDULE_PATH].map((path) =>
      queryClient.invalidateQueries({ queryKey: ['get', path] }),
    ),
  );
}
