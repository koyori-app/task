/** backend の validate_project_key と同じ制約（空は自動生成に委ねる） */
export const PROJECT_KEY_PATTERN = /^$|^[A-Z][A-Z0-9]{1,9}$/;

/** 名前からキー候補を作る（backend の generate_project_key 相当の簡易版） */
export function suggestKey(name: string): string {
  const upper = name
    .normalize('NFKD')
    .toUpperCase()
    .replace(/[^A-Z0-9]+/g, '')
    .replace(/^[0-9]+/, '')
    .slice(0, 10);
  return upper.length >= 2 ? upper : '';
}

/** create_project（#372）が seed する既定ステータスセット。作成ページのプレビュー表示用 */
export const DEFAULT_STATUS_PREVIEW = [
  { name: 'Backlog', color: '#94a3b8', isDefault: false, isDone: false },
  { name: 'Todo', color: '#3b82f6', isDefault: true, isDone: false },
  { name: 'In Progress', color: '#f59e0b', isDefault: false, isDone: false },
  { name: 'Done', color: '#22c55e', isDefault: false, isDone: true },
] as const;
