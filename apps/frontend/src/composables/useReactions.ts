import { useQueryClient } from '@tanstack/vue-query';
import { computed, ref, toValue, type MaybeRefOrGetter } from 'vue';

import { GET_TASK_PATH } from '@/composables/useTaskDetail';
import {
  apiErrorMessage,
  LIST_COMMENTS_PATH,
  type CommentThread,
} from '@/composables/useTaskComments';
import type { components } from '@/generated/api';
import { apiClient, fetchClient } from '@/lib/api-vue-query';
import { applyReaction, type ReactionSummary, type ReactionUser } from '@/lib/reactions';

const TASK_REACTION_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/reactions/{emoji}' as const;
const TASK_REACTION_USERS_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/reactions/{emoji}/users' as const;
const COMMENT_REACTION_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/comments/{cid}/reactions/{emoji}' as const;
const COMMENT_REACTION_USERS_PATH =
  '/v1/tenants/{tenant_id}/projects/{project_id}/tasks/{id}/comments/{cid}/reactions/{emoji}/users' as const;

type TaskDetail = components['schemas']['TaskDetailResponse'];
type CommentList = { comments: CommentThread[] };

function reactionErrorMessage(error: unknown): string {
  const summary = 'リアクションを送れませんでした';
  const message =
    error && typeof error === 'object' && 'error' in error
      ? (error as { error?: { message?: unknown } }).error?.message
      : undefined;
  if (message === 'reaction-limit-exceeded') return 'これ以上種類を増やせません';
  // 不正な絵文字と削除済みコメントはどちらも bad-request で、内部の語を見せても伝わらない
  if (message === 'bad-request') return summary;
  return apiErrorMessage(error, summary);
}

function withCommentReactions(
  list: CommentList,
  commentId: string,
  update: (reactions: ReactionSummary[]) => ReactionSummary[],
): CommentList {
  return {
    ...list,
    comments: list.comments.map((thread) => ({
      ...thread,
      ...(thread.id === commentId && { reactions: update(thread.reactions) }),
      replies: thread.replies.map((reply) =>
        reply.id === commentId ? { ...reply, reactions: update(reply.reactions) } : reply,
      ),
    })),
  };
}

function findCommentReactions(list: CommentList | undefined, commentId: string) {
  for (const thread of list?.comments ?? []) {
    if (thread.id === commentId) return thread.reactions;
    const reply = thread.replies.find((item) => item.id === commentId);
    if (reply) return reply.reactions;
  }
  return [];
}

export interface UseReactionsParams {
  tenantId: MaybeRefOrGetter<string | null | undefined>;
  projectId: MaybeRefOrGetter<string | null | undefined>;
  /** タスク識別子（URL と同じ seq key 形式） */
  taskId: MaybeRefOrGetter<string>;
  /** 押した時点の表示に本人を載せるため */
  currentUser: MaybeRefOrGetter<ReactionUser | null>;
}

/**
 * タスク本体とコメントのリアクション。`commentId` が null ならタスク本体を指す。
 * 押した時点でキャッシュを書き換え（楽観更新）、失敗したら戻して文言を出す。
 * 同じ対象の次の操作は再取得が終わるまで受けない（古い応答が新しい操作を上書きしないように）。
 */
export function useReactions(params: UseReactionsParams) {
  const tenantId = computed(() => toValue(params.tenantId) ?? null);
  const projectId = computed(() => toValue(params.projectId) ?? null);
  const taskId = computed(() => String(toValue(params.taskId) ?? ''));

  const queryClient = useQueryClient();

  const busyTargets = ref(new Set<string>());
  const errors = ref(new Map<string, string>());

  const putTask = apiClient.useMutation('put', TASK_REACTION_PATH);
  const deleteTask = apiClient.useMutation('delete', TASK_REACTION_PATH);
  const putComment = apiClient.useMutation('put', COMMENT_REACTION_PATH);
  const deleteComment = apiClient.useMutation('delete', COMMENT_REACTION_PATH);

  function taskPath() {
    return { tenant_id: tenantId.value!, project_id: projectId.value!, id: taskId.value };
  }

  /**
   * 送信中と失敗の鍵。ペインは作り直されずにタスクを移るので、タスクまで含めて分ける
   * （前のタスクの送信中に次のタスクが押せなくなる、前のタスクの失敗が次のタスクに出る）。
   */
  function targetKey(path: ReturnType<typeof taskPath>, commentId: string | null) {
    return JSON.stringify([path.tenant_id, path.project_id, path.id, commentId]);
  }

  /**
   * openapi-fetch がパスの値を encodeURIComponent するので、絵文字はそのまま渡す。
   * ここで掛けると二重になり、backend に `%F0...` の字のまま届く。
   */
  function send(
    path: ReturnType<typeof taskPath>,
    commentId: string | null,
    emoji: string,
    reacted: boolean,
  ) {
    if (commentId === null) {
      const mutation = reacted ? putTask : deleteTask;
      return mutation.mutateAsync({ params: { path: { ...path, emoji } } });
    }
    const mutation = reacted ? putComment : deleteComment;
    return mutation.mutateAsync({ params: { path: { ...path, cid: commentId, emoji } } });
  }

  async function toggle(commentId: string | null, emoji: string): Promise<void> {
    if (!tenantId.value || !projectId.value || !taskId.value) return;
    // ペインの切替で unmount しても完走した結果を対象のキャッシュへ書けるよう、開始時点で固定する
    const path = taskPath();
    const key = targetKey(path, commentId);
    if (busyTargets.value.has(key)) return;

    busyTargets.value.add(key);
    errors.value.delete(key);
    const me = toValue(params.currentUser);
    const commentsKey = ['get', LIST_COMMENTS_PATH, { params: { path } }] as const;
    const taskKey = ['get', GET_TASK_PATH, { params: { path } }] as const;

    const read = () =>
      commentId === null
        ? (queryClient.getQueryData<TaskDetail | null>(taskKey)?.reactions ?? [])
        : findCommentReactions(queryClient.getQueryData<CommentList>(commentsKey), commentId);
    const write = (update: (reactions: ReactionSummary[]) => ReactionSummary[]) => {
      if (commentId === null) {
        queryClient.setQueryData<TaskDetail | null>(taskKey, (task) =>
          task ? { ...task, reactions: update(task.reactions) } : task,
        );
      } else {
        queryClient.setQueryData<CommentList>(commentsKey, (list) =>
          list ? withCommentReactions(list, commentId, update) : list,
        );
      }
    };

    try {
      // 走っている取得の古い応答で楽観更新を消されないよう止める
      await queryClient.cancelQueries({
        queryKey: commentId === null ? taskKey : commentsKey,
        exact: true,
      });
      const before = read();
      const reacted = !before.find((reaction) => reaction.emoji === emoji)?.reacted;
      write((reactions) => applyReaction(reactions, emoji, reacted, me));
      try {
        await send(path, commentId, emoji, reacted);
      } catch (error) {
        write(() => before);
        errors.value.set(key, reactionErrorMessage(error));
        return;
      }
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: commentsKey, exact: true }),
        queryClient.invalidateQueries({ queryKey: taskKey, exact: true }),
      ]);
    } finally {
      busyTargets.value.delete(key);
    }
  }

  async function fetchUsers(commentId: string | null, emoji: string): Promise<ReactionUser[]> {
    const { data, error } =
      commentId === null
        ? await fetchClient.GET(TASK_REACTION_USERS_PATH, {
            params: { path: { ...taskPath(), emoji } },
          })
        : await fetchClient.GET(COMMENT_REACTION_USERS_PATH, {
            params: { path: { ...taskPath(), cid: commentId, emoji } },
          });
    if (error) throw error;
    return data.users;
  }

  return {
    isBusy: (commentId: string | null) => busyTargets.value.has(targetKey(taskPath(), commentId)),
    errorFor: (commentId: string | null) =>
      errors.value.get(targetKey(taskPath(), commentId)) ?? null,
    toggle,
    fetchUsers,
  };
}

export type ReactionController = ReturnType<typeof useReactions>;
