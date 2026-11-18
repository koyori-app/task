import { reactive, readonly } from 'vue';
import { fetchClient } from '@/lib/api-vue-query';
import { isReauthFresh, REAUTH_PATH, type ReauthResume, type ReauthStatus } from '@/lib/reauth';

/**
 * 直近の本人確認（再認証）を要る操作の前に置く関所。
 *
 * 操作の前に backend へ印の有無を問い、要れば本人確認の dialog（ReauthDialog）を開いて、
 * 確かめ終えるか取り消すまで待つ。dialog は一つの画面に一つ置き、状態はここで共にする
 * （パスワードの行と連携の行のように、離れた部品から同じ dialog を開くため）。
 */
type GateState = {
  open: boolean;
  /** 確かめた後に続ける操作。連携で確かめる時に、戻った画面で続けるために置く。 */
  resume: ReauthResume | null;
  status: ReauthStatus | null;
  /** 開いた時に出す知らせ（連携で確かめられずに戻った時など）。 */
  notice: string | null;
  /** 問いの番号。開いたままの dialog に次の問いが来ても、中身を戻せるようにする。 */
  seq: number;
};

const state = reactive<GateState>({
  open: false,
  resume: null,
  status: null,
  notice: null,
  seq: 0,
});
let settle: ((ok: boolean) => void) | null = null;

/** 印の有無を問えなかった。操作は撃たない（撃てば 403 で止まる）。 */
export class ReauthStatusError extends Error {
  constructor() {
    super('reauth status unavailable');
    this.name = 'ReauthStatusError';
  }
}

/**
 * 再認証を要る操作の前に呼ぶ。要らなければ true。要れば dialog を開き、確かめれば true、
 * 取り消せば false。問えなければ ReauthStatusError を投げる。
 */
export async function requireRecentAuth(
  resume: ReauthResume,
  options: { notice?: string } = {},
): Promise<boolean> {
  let result;
  try {
    result = await fetchClient.GET(REAUTH_PATH);
  } catch {
    // 通信が切れた。呼び手は問えなかった時と同じに知らせる（例外のまま抜けると、何も出ずに止まる）
    throw new ReauthStatusError();
  }
  const { data, error } = result;
  if (error || !data) throw new ReauthStatusError();
  if (isReauthFresh(data)) return true;
  // 前の問いが開いたままなら、取り消したことにして閉じる
  settle?.(false);
  state.open = true;
  state.resume = resume;
  state.status = data;
  state.notice = options.notice ?? null;
  state.seq += 1;
  return new Promise((resolve) => {
    settle = resolve;
  });
}

export function useReauthGate() {
  /** dialog を閉じ、待っておる操作へ結果を返す。 */
  function finish(ok: boolean) {
    state.open = false;
    const resolve = settle;
    settle = null;
    resolve?.(ok);
  }
  return { state: readonly(state), finish };
}
