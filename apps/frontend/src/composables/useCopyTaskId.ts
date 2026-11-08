import { onBeforeUnmount, ref } from 'vue';

/** コピー結果を出しておく時間（ミリ秒） */
const COPY_RESET_MS = 2000;

export type CopyTaskIdState = 'idle' | 'copied' | 'error';

/** ボタンの読み上げ名。見た目はアイコンが変わるだけなので、結果はここで伝える */
export function copyTaskIdLabel(state: CopyTaskIdState, seqKey: string) {
  if (state === 'copied') return 'タスクIDをコピーしました';
  if (state === 'error') return 'タスクIDをコピーできませんでした';
  return `タスクID ${seqKey} をコピー`;
}

/** タスク ID のコピー。非 secure context では clipboard が無いので失敗も示す */
export function useCopyTaskId() {
  const copyState = ref<CopyTaskIdState>('idle');
  let copyResetTimer: ReturnType<typeof setTimeout> | undefined;
  /** コピー操作の世代。連続して押したとき、最後の操作だけが表示とタイマーを動かす */
  let copyRequest = 0;

  function clearCopyReset() {
    if (copyResetTimer) {
      clearTimeout(copyResetTimer);
      copyResetTimer = undefined;
    }
  }

  async function copyTaskId(seqKey: string) {
    const request = ++copyRequest;
    // 実行中は前の結果を出したままにする（新しい操作の結果が出る前に消さない）
    clearCopyReset();

    let result: CopyTaskIdState;
    try {
      await navigator.clipboard.writeText(seqKey);
      result = 'copied';
    } catch {
      result = 'error';
    }

    // 遅れて返ってきた古い操作は、新しい操作の表示もタイマーも触らない
    if (request !== copyRequest) return;
    clearCopyReset();
    copyState.value = result;
    copyResetTimer = setTimeout(() => (copyState.value = 'idle'), COPY_RESET_MS);
  }

  onBeforeUnmount(clearCopyReset);

  return { copyState, copyTaskId };
}
