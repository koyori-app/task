import { useQueryClient } from '@tanstack/vue-query';
import { computed, ref, toValue, type MaybeRefOrGetter } from 'vue';

import { fetchClient } from '@/lib/api-vue-query';
import type { components } from '@/generated/api';
import { ACTIVITIES_PATH } from '@/composables/useTaskActivities';
import { TASK_RELATIONS_PATH } from '@/composables/useTaskSubtasks';

const GET_TASK_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}' as const;
const LIST_TASKS_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks' as const;
const TASK_SEARCH_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/search' as const;
const SCHEDULE_PATH = '/v1/tenants/{tenant_id}/schedule' as const;
const COMPLETE_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/assignees/{user_id}/complete' as const;
const REOPEN_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/assignees/{user_id}/reopen' as const;

type TaskResponse = components['schemas']['TaskResponse'];

export type TaskCompletionParams = {
  tenantId: MaybeRefOrGetter<string | null | undefined>;
  projectId: MaybeRefOrGetter<string | null | undefined>;
};

/** API の失敗。`message` は backend のエラー本文、`status` は HTTP ステータス。 */
type CompletionFailure = { status: number; message?: string };

function failureText(failure: unknown): string {
  const { status, message } = (failure ?? {}) as Partial<CompletionFailure>;
  if (message === 'task-already-done') {
    return 'タスクが完了済みです。ステータスを戻してから取り消してください';
  }
  if (status === 400) return '完了へ移せません。必須のカスタムフィールドを確認してください';
  return '更新に失敗しました';
}

/**
 * 担当者ごとの完了（`require_all_assignees_done`）の操作。
 * 設定の切り替えと、自分の担当分の完了・取り消し。
 *
 * 完了にした結果タスクの状態が変わりうる（最後の 1 人なら完了へ移る）ので、
 * 成功後は一覧・検索・詳細・履歴などをまとめて取り直す。
 */
export function useTaskCompletion(params: TaskCompletionParams) {
  const queryClient = useQueryClient();
  const tenantId = computed(() => toValue(params.tenantId) ?? '');
  const projectId = computed(() => toValue(params.projectId) ?? '');

  /** 更新中のタスク ID。 */
  const pending = ref<Record<string, boolean | undefined>>({});
  /** タスクごとの直近の失敗。 */
  const errors = ref<Record<string, string | undefined>>({});

  function pathParams(taskId: string) {
    return { tenant_id: tenantId.value, project_id: projectId.value, id: taskId };
  }

  function invalidate() {
    return Promise.all([
      queryClient.invalidateQueries({ queryKey: ['get', LIST_TASKS_PATH] }),
      queryClient.invalidateQueries({ queryKey: ['get', TASK_SEARCH_PATH] }),
      queryClient.invalidateQueries({ queryKey: ['get', GET_TASK_PATH] }),
      queryClient.invalidateQueries({ queryKey: ['get', TASK_RELATIONS_PATH] }),
      queryClient.invalidateQueries({ queryKey: ['get', ACTIVITIES_PATH] }),
      queryClient.invalidateQueries({ queryKey: ['get', SCHEDULE_PATH] }),
    ]);
  }

  async function run(
    taskId: string,
    request: () => Promise<{ error?: unknown; response: Response }>,
  ) {
    if (!tenantId.value || !projectId.value) return;
    if (pending.value[taskId]) return;
    pending.value = { ...pending.value, [taskId]: true };
    errors.value = { ...errors.value, [taskId]: undefined };
    try {
      const { error, response } = await request();
      if (error) {
        const message = (error as { message?: string } | null)?.message;
        throw { status: response.status, message } satisfies CompletionFailure;
      }
      await invalidate();
    } catch (failure) {
      errors.value = { ...errors.value, [taskId]: failureText(failure) };
      // 404（途中で担当者から外された）と 409（すでに完了済み・設定が off）は、画面が
      // 古い担当者やボタンを見ているのが原因なので、詳細を取り直して最新にする
      const { status } = failure as Partial<CompletionFailure>;
      if (status === 404 || status === 409) await invalidate();
    } finally {
      const next = { ...pending.value };
      delete next[taskId];
      pending.value = next;
    }
  }

  /** 「担当者全員の完了が必要」の切り替え。off にすると全員の完了が消える。 */
  function setRequireAll(task: TaskResponse, value: boolean) {
    if (!!task.require_all_assignees_done === value) return Promise.resolve();
    return run(task.id, () =>
      fetchClient.PUT(GET_TASK_PATH, {
        params: { path: pathParams(task.id) },
        body: { require_all_assignees_done: value },
      }),
    );
  }

  /** 自分の担当分を完了にする。最後の 1 人ならタスクが完了へ移る。 */
  function completeMine(task: TaskResponse, userId: string) {
    return run(task.id, () =>
      fetchClient.POST(COMPLETE_PATH, {
        params: { path: { ...pathParams(task.id), user_id: userId } },
      }),
    );
  }

  /** 自分の担当分の完了を取り消す。タスクが完了済みの間は 409。 */
  function reopenMine(task: TaskResponse, userId: string) {
    return run(task.id, () =>
      fetchClient.POST(REOPEN_PATH, {
        params: { path: { ...pathParams(task.id), user_id: userId } },
      }),
    );
  }

  return { pending, errors, setRequireAll, completeMine, reopenMine };
}
