import { computed, toValue, type MaybeRefOrGetter } from 'vue';
import { useQuery } from '@tanstack/vue-query';

import { apiClient } from '@/lib/api-vue-query';

export const SIMILAR_TASKS_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/similar' as const;

const SIMILAR_TASKS_LIMIT = 5;
/** 1 文字では似ているかの判定にならないので問い合わせない */
const MIN_TITLE_LENGTH = 2;

export type UseSimilarTasksParams = {
  /** API 用の UUID（表示用 ID ではない） */
  tenantId: MaybeRefOrGetter<string | null | undefined>;
  projectId: MaybeRefOrGetter<string | null | undefined>;
  title: MaybeRefOrGetter<string>;
  /** 候補から除くタスク（詳細で自分自身を除く） */
  excludeTaskId?: MaybeRefOrGetter<string | null | undefined>;
};

/**
 * 同じプロジェクトでタイトルが似ているタスク（最大 5 件）。
 *
 * 4 つの入力はすべて query key に入る。`enabled: false` にしてもキャッシュ済みの
 * `data` は返るため、key を変えることで旧タイトルの応答を持ち越さない。
 */
export function useSimilarTasks(params: UseSimilarTasksParams) {
  return useQuery(
    computed(() => {
      const tenantId = toValue(params.tenantId) ?? '';
      const projectId = toValue(params.projectId) ?? '';
      const title = toValue(params.title).trim();
      return {
        ...apiClient.queryOptions(
          'get',
          SIMILAR_TASKS_PATH,
          {
            params: {
              path: { tenant_id: tenantId, project_id: projectId },
              query: {
                title,
                exclude_task_id: toValue(params.excludeTaskId) ?? undefined,
                limit: SIMILAR_TASKS_LIMIT,
              },
            },
          },
          { retry: false },
        ),
        enabled: !!tenantId && !!projectId && title.length >= MIN_TITLE_LENGTH,
      };
    }),
  );
}
