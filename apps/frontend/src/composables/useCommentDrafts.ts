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

function clearCommentDraft(
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

/*
 * 宛先ごとの送信中の状態。下書きと同じ置き場に置くので、ペイン（composable）を作り直しても効き、
 * 送った下書きを送り直したり書き足したりできない。値は送信ごとの一意な token で、古い送信の終わりが
 * 新しい送信の状態を解かないようにする。メモリだけに置く（リロードで消える）
 */
const sending = ref<Record<string, symbol>>({});

export function isCommentDraftSending(
  userId: string | null,
  taskId: string | undefined,
  threadId: string | null,
): boolean {
  return keyOf(userId, taskId, threadId) in sending.value;
}

/**
 * 送信を始める。送る本文を覚えて送信中にし、「終える」関数を返す。
 * 終える関数は、送信が通って下書きが送った本文のままなら消すのと、送信中の解除を、同じ同期の呼び出しで行う
 * （下書きを先に消すので、送信中が解けて送った本文が残ったままの状態は途中でも見えない）。
 * 宛先と送った本文は開始時に固定する。送信中に一覧へ戻ったりスレッドを開き直したりしても、
 * 消すのは送った宛先の下書きだけ。下書きが送った本文と違えば、送信済みの本文ではないので残す。
 * ログアウトのリセットの後に同じ宛先で新しい送信が始まっていたら、古い送信の終わりは何もしない
 * （新しい送信の下書きを消したり、送信中を解いたりしない）
 */
export function beginCommentDraftSend(
  userId: string | null,
  taskId: string | undefined,
  threadId: string | null,
): (posted: boolean) => void {
  const key = keyOf(userId, taskId, threadId);
  const sent = getCommentDraft(userId, taskId, threadId);
  const token = Symbol(key);
  sending.value = { ...sending.value, [key]: token };
  return (posted) => {
    if (sending.value[key] !== token) return;
    if (posted && getCommentDraft(userId, taskId, threadId) === sent) {
      clearCommentDraft(userId, taskId, threadId);
    }
    const rest = { ...sending.value };
    delete rest[key];
    sending.value = rest;
  };
}

/**
 * 明示的なログアウト（useAuthSession の logout）とテストで使う。メモリと sessionStorage の下書きと、
 * 送信中の状態をすべて消す
 */
export function resetCommentDrafts() {
  memory.value = {};
  sending.value = {};
  try {
    const s = storage();
    if (!s) return;
    for (const key of Object.keys(s).filter((k) => k.startsWith(PREFIX))) s.removeItem(key);
  } catch {
    // 同上
  }
}
