import { useQuery, useQueryClient } from '@tanstack/vue-query';
import { computed, ref, toValue, type MaybeRefOrGetter } from 'vue';

import { ACTIVITIES_PATH } from '@/composables/useTaskActivities';
import type { components } from '@/generated/api';
import { apiClient, fetchClient } from '@/lib/api-vue-query';

const FORGE_PATH = '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge' as const;
const UNLINK_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge/pull-requests/{pull_request_id}' as const;
const ROLE_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/forge/pull-requests/{pull_request_id}/role' as const;

export type TaskForgePullRequest = components['schemas']['TaskForgePullRequest'];
export type PullRequestRole = 'implementation' | 'mention';

type TaskForgeParams = {
  tenantId: MaybeRefOrGetter<string | null | undefined>;
  projectId: MaybeRefOrGetter<string | null | undefined>;
  /** UUID または PROJECT-42 */
  taskId: MaybeRefOrGetter<string>;
};

/**
 * タスクに結ばれた PR・コミット・ブランチ。題の上の実装 PR のピルとリポジトリ欄が同じ
 * クエリキーを使い、1 回の取得を分け合う。
 */
export function useTaskForge(params: TaskForgeParams) {
  const tenantId = computed(() => toValue(params.tenantId) ?? null);
  const projectId = computed(() => toValue(params.projectId) ?? null);
  const taskId = computed(() => toValue(params.taskId));
  return useQuery({
    queryKey: computed(() => [
      'get',
      FORGE_PATH,
      { tenantId: tenantId.value, projectId: projectId.value, taskId: taskId.value },
    ]),
    queryFn: async ({ signal }) => {
      const { data, error } = await fetchClient.GET(FORGE_PATH, {
        params: {
          path: { tenant_id: tenantId.value!, project_id: projectId.value!, id: taskId.value },
        },
        signal,
      });
      if (error) throw error;
      return data;
    },
    enabled: computed(() => !!tenantId.value && !!projectId.value && !!taskId.value),
    retry: false,
  });
}

/**
 * PR の役割の入れ替えと、リンクを外す操作。ピルとリポジトリ欄が同じ書き込みと同じ後始末を使う。
 * 書き込みの後は forge とアクティビティを取り直す（楽観更新はしない）。
 */
export function useTaskForgePullRequestActions(params: TaskForgeParams) {
  const queryClient = useQueryClient();
  const roleMutation = apiClient.useMutation('put', ROLE_PATH);
  const unlinkMutation = apiClient.useMutation('delete', UNLINK_PATH);

  function path(pullRequest: TaskForgePullRequest) {
    const tenantId = toValue(params.tenantId);
    const projectId = toValue(params.projectId);
    if (!tenantId || !projectId) return null;
    return {
      tenant_id: tenantId,
      project_id: projectId,
      id: toValue(params.taskId),
      pull_request_id: pullRequest.id,
    };
  }

  function invalidate() {
    return Promise.all([
      queryClient.invalidateQueries({ queryKey: ['get', FORGE_PATH] }),
      queryClient.invalidateQueries({ queryKey: ['get', ACTIVITIES_PATH] }),
    ]);
  }

  const roleError = ref<string | null>(null);
  /** 役割を変える。応答が返るまで表示は変えない */
  async function setRole(pullRequest: TaskForgePullRequest, role: PullRequestRole) {
    const target = path(pullRequest);
    if (!target || roleMutation.isPending.value) return;
    roleError.value = null;
    try {
      await roleMutation.mutateAsync({ params: { path: target }, body: { role } });
      await invalidate();
    } catch {
      roleError.value = 'PR の役割を変えられませんでした';
    }
  }

  // リンクを外すのは確認してから。確認待ちの PR を持つ
  const unlinkTarget = ref<TaskForgePullRequest | null>(null);
  const unlinkError = ref<string | null>(null);
  function requestUnlink(pullRequest: TaskForgePullRequest) {
    unlinkError.value = null;
    unlinkTarget.value = pullRequest;
  }
  function cancelUnlink() {
    // 送信中に閉じると結果を見逃す
    if (unlinkMutation.isPending.value) return;
    unlinkTarget.value = null;
  }
  async function confirmUnlink() {
    const pullRequest = unlinkTarget.value;
    const target = pullRequest && path(pullRequest);
    if (!target) return;
    unlinkError.value = null;
    try {
      await unlinkMutation.mutateAsync({ params: { path: target } });
      await invalidate();
      unlinkTarget.value = null;
    } catch {
      unlinkError.value = 'リンクを外せませんでした';
    }
  }

  return {
    setRole,
    rolePending: roleMutation.isPending,
    roleError,
    unlinkTarget,
    unlinkError,
    unlinkPending: unlinkMutation.isPending,
    requestUnlink,
    cancelUnlink,
    confirmUnlink,
  };
}

/** マージ済みなのに自動完了を行っていない PR か（確認の操作を待っている） */
export function mergeEffectsPending(pullRequest: TaskForgePullRequest): boolean {
  return pullRequest.state === 'merged' && !pullRequest.merge_effects_processed_at;
}
