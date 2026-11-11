import { nextTick, watch, type Ref, type WatchSource } from 'vue';

type FindTarget = (list: HTMLElement) => HTMLElement | null | undefined;

const bySeqKey =
  (seqKey: string): FindTarget =>
  (list) =>
    [...list.querySelectorAll<HTMLElement>('[data-task-seq-key]')].find(
      (el) => el.dataset.taskSeqKey === seqKey,
    );

const byId =
  (id: string): FindTarget =>
  (list) => {
    const el = document.getElementById(id);
    return el && list.contains(el) ? el : null;
  };

/**
 * 詳細ペインの開閉で、一覧は SplitPane の内と外を行き来する。そのたびに一覧の DOM が
 * 作り直されてフォーカスとスクロール位置が失われるので、切り替わる直前に控えて描き直した後に戻す。
 *
 * - フォーカスが一覧の中にあれば同じ要素へ戻す（タイトルリンクは seq key、ほかは id で探す）
 * - 詳細ペインや境界にあったまま閉じたら、閉じたタスクのタイトルリンクへ戻す
 * - 戻す先が見つからなければ一覧の DataList（`tabindex="-1"` の region）へ当てる
 * - 一覧の外（ダイアログなど）にあったフォーカスは動かさない
 *
 * @param openSeqKey 詳細を開いているタスクの seq key。閉じていれば null
 * @param splitView 一覧と詳細ペインを含む要素
 */
export function keepTaskListFocusAcrossSplit(
  openSeqKey: WatchSource<string | null>,
  splitView: Ref<HTMLElement | null>,
  list: Ref<HTMLElement | null>,
  scroller: Ref<HTMLElement | null>,
) {
  // 既定の flush: 'pre' で、描き直す前の DOM から控える
  watch(openSeqKey, async (seqKey, prevSeqKey) => {
    // 開いたまま別のタスクへ移っただけなら、一覧は作り直されない
    if ((seqKey === null) === (prevSeqKey === null)) return;
    const scrollTop = scroller.value?.scrollTop ?? 0;
    const find = focusTarget(splitView.value, list.value, prevSeqKey);
    await nextTick();
    if (scroller.value) scroller.value.scrollTop = scrollTop;
    const newList = list.value;
    if (!find || !newList) return;
    const target = find(newList) ?? newList.querySelector<HTMLElement>('section[tabindex="-1"]');
    target?.focus({ preventScroll: true });
  });
}

function focusTarget(
  splitView: HTMLElement | null,
  list: HTMLElement | null,
  closedSeqKey: string | null,
): FindTarget | null {
  const active = document.activeElement;
  if (!(active instanceof HTMLElement)) return null;
  if (list?.contains(active)) {
    const { taskSeqKey } = active.dataset;
    if (taskSeqKey) return bySeqKey(taskSeqKey);
    return active.id ? byId(active.id) : () => null;
  }
  if (closedSeqKey && splitView?.contains(active)) return bySeqKey(closedSeqKey);
  return null;
}
