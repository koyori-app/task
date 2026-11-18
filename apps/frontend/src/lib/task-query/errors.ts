import type { components } from '@/generated/api';
export type TaskQueryError = components['schemas']['TaskQueryError'];
export function taskQueryErrorsOf(error: unknown): TaskQueryError[] | null {
  if (
    error &&
    typeof error === 'object' &&
    'message' in error &&
    error.message === 'invalid-task-query' &&
    'errors' in error &&
    Array.isArray(error.errors)
  )
    return error.errors;
  return null;
}
export function describeTaskQueryError(e: TaskQueryError): string {
  switch (e.code) {
    case 'unknown-value':
      return ['assignee', 'author'].includes(e.key ?? '')
        ? `「${e.value}」という利用者は見つかりません`
        : `「${e.value}」という${e.key === 'status' ? 'ステータス' : 'ラベル'}はありません`;
    case 'empty-value':
      return `${e.key}: の値がありません`;
    case 'not-negatable':
      return `${e.key}: は除外（-）にできません`;
    case 'duplicate-sort':
    case 'conflicting-sort':
      return 'sort: は 1 つだけ指定できます';
    case 'too-long':
      return '条件が長すぎます';
    default:
      return `${e.key}: に「${e.value}」は指定できません`;
  }
}
