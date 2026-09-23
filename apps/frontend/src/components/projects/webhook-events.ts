/** 購読できるイベント（backend `service::webhooks::EVENTS` と同じ集合。仕様 §3 の実装済み） */
export const WEBHOOK_EVENTS = [
  { value: 'task.created', label: 'タスクの作成' },
  { value: 'comment.created', label: 'コメントの投稿' },
  { value: 'review.round_created', label: 'レビューラウンドの起票' },
  { value: 'review.finding_changed', label: 'レビュー指摘の状態変更' },
] as const;

export const WEBHOOK_FORMATS = [
  { value: 'json', label: 'JSON' },
  { value: 'discord', label: 'Discord' },
] as const;

export type WebhookFormat = (typeof WEBHOOK_FORMATS)[number]['value'];

export function webhookEventLabel(event: string): string {
  return WEBHOOK_EVENTS.find((e) => e.value === event)?.label ?? event;
}

export function webhookFormatLabel(format: string): string {
  return WEBHOOK_FORMATS.find((f) => f.value === format)?.label ?? format;
}

/**
 * 変更系 API の失敗を文言にする。403 は権限不足、400 は API の `message`（URL の拒否理由など）を
 * そのまま出し、それ以外は呼び出し側の既定文言。
 */
export function webhookErrorMessage(error: unknown, fallback: string): string {
  const e = error as { response?: { status?: number }; error?: { message?: string } };
  const status = e.response?.status;
  if (status === 403) return 'この操作にはプロジェクトの管理者権限が必要です';
  if (status === 400 && e.error?.message) return e.error.message;
  return fallback;
}
