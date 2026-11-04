import { ref } from 'vue';

/*
 * コメントの下書き置き場。キーは利用者 ID + タスクのキー（テナント/プロジェクト/表示用 ID）+ スレッド ID（一覧は threadId = null）。
 * 利用者 ID を含めるのは、同じタブで別の利用者がログインしても前の人の下書きを出さないため。
 * 利用者 ID が未取得（null）の間は sessionStorage へ書かず、メモリだけに置く。
 * 明示的なログアウト（useAuthSession の logout）で resetCommentDrafts が全部消す。
 * clearUser は /me の一時障害でも呼ばれるため、そこでは消さない（利用者の切り替えはキーの利用者 ID で防ぐ）。
 * コンポーネントの外（sessionStorage）にも書くので、別のタスクへ移って戻る・オーバーレイを
 * 閉じて開き直しても復元できる。sessionStorage が使えない環境（SSR・例外）ではメモリだけで動く。
 */

const PREFIX = 'task:comment-draft:';

// 読み書きの両方がここを通る。reactive なので、書いた下書きを読む computed が追従する
const memory = ref<Record<string, string>>({});

function keyOf(userId: string | null, taskId: string | undefined, threadId: string | null) {
  return `${PREFIX}${userId ?? ''}:${taskId ?? ''}:${threadId ?? ''}`;
}

function storage(): Storage | null {
  try {
    return typeof window === 'undefined' ? null : window.sessionStorage;
  } catch {
    return null;
  }
}

export function getCommentDraft(
  userId: string | null,
  taskId: string | undefined,
  threadId: string | null,
): string {
  const key = keyOf(userId, taskId, threadId);
  if (key in memory.value) return memory.value[key] ?? '';
  if (!userId) return '';
  try {
    return storage()?.getItem(key) ?? '';
  } catch {
    return '';
  }
}

export function setCommentDraft(
  userId: string | null,
  taskId: string | undefined,
  threadId: string | null,
  value: string,
) {
  const key = keyOf(userId, taskId, threadId);
  memory.value = { ...memory.value, [key]: value };
  if (!userId) return;
  try {
    if (value) storage()?.setItem(key, value);
    else storage()?.removeItem(key);
  } catch {
    // 容量超過・無効化。メモリだけで続ける
  }
}

export function clearCommentDraft(
  userId: string | null,
  taskId: string | undefined,
  threadId: string | null,
) {
  const key = keyOf(userId, taskId, threadId);
  const rest = { ...memory.value };
  delete rest[key];
  memory.value = rest;
  try {
    storage()?.removeItem(key);
  } catch {
    // 同上
  }
}

/** 明示的なログアウト（useAuthSession の logout）とテストで使う。メモリと sessionStorage の下書きをすべて消す */
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
