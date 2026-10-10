export type EditableField =
  | 'title'
  | 'description'
  | 'progress_pct'
  | 'soft_deadline'
  | 'hard_deadline';

/** useTaskDetail が 1 リクエストで更新する単位。保存通知の対象もこれ */
export type MutatingField = EditableField | 'status_id' | 'labels' | 'priority';
