import { ref } from 'vue';

/*
 * コメントの下書き置き場。キーはタスク ID + スレッド ID（一覧は threadId = null）。
 * コンポーネントの外（sessionStorage）にも書くので、別のタスクへ移って戻る・オーバーレイを
 * 閉じて開き直しても復元できる。sessionStorage が使えない環境（SSR・例外）ではメモリだけで動く。
 */

const PREFIX = 'task:comment-draft:';

// 読み書きの両方がここを通る。reactive なので、書いた下書きを読む computed が追従する
const memory = ref<Record<string, string>>({});

function keyOf(taskId: string | undefined, threadId: string | null) {
  return `${PREFIX}${taskId ?? ''}:${threadId ?? ''}`;
}

function storage(): Storage | null {
  try {
    return typeof window === 'undefined' ? null : window.sessionStorage;
  } catch {
    return null;
  }
}

export function getCommentDraft(taskId: string | undefined, threadId: string | null): string {
  const key = keyOf(taskId, threadId);
  if (key in memory.value) return memory.value[key] ?? '';
  try {
    return storage()?.getItem(key) ?? '';
  } catch {
    return '';
  }
}

export function setCommentDraft(
  taskId: string | undefined,
  threadId: string | null,
  value: string,
) {
  const key = keyOf(taskId, threadId);
  memory.value = { ...memory.value, [key]: value };
  try {
    if (value) storage()?.setItem(key, value);
    else storage()?.removeItem(key);
  } catch {
    // 容量超過・無効化。メモリだけで続ける
  }
}

export function clearCommentDraft(taskId: string | undefined, threadId: string | null) {
  const key = keyOf(taskId, threadId);
  const rest = { ...memory.value };
  delete rest[key];
  memory.value = rest;
  try {
    storage()?.removeItem(key);
  } catch {
    // 同上
  }
}

/** テスト用。メモリと sessionStorage の下書きをすべて消す */
export function resetCommentDrafts() {
  memory.value = {};
  try {
    const s = storage();
    if (!s) return;
    for (const key of Object.keys(s).filter((k) => k.startsWith(PREFIX))) s.removeItem(key);
  } catch {
    // 同上
  }
}
