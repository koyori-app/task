import { setSort, qualifierValues } from '@/lib/task-query/edit';
import { computed, watch, type Ref } from 'vue';

/**
 * タスク一覧の「今どこを見ているか」を URL クエリで持つ。
 *
 * ページ番号・検索語・ラベル絞り込み・並び替えはコンポーネントの ref だけに載っていて、
 * リロードや詳細ページからの戻りで初期値に戻っていた。3 ページ目まで送ってタスクを開き、
 * 戻ると 1 ページ目から探し直しになる。
 *
 * 置き場所を URL にすると、リロードとブラウザの戻るの両方が同じ経路で復元される
 * （選択タスクの `?selected=` が既にこの形）。localStorage だと URL を共有したときに
 * 相手と違う画面になり、タブごとに別の状態を持てない。
 */
/** 一覧の表示形式。既定はステータスごとに並べる List。 */
export type TaskListView = 'list' | 'table';

export type TaskListUrlState = {
  view: TaskListView;
  /** 1 始まり。URL に出るので人が読める形にする（内部の pageIndex は 0 始まり） */
  page: number;
  /** 確定済みの検索語（デバウンス後に投げているもの） */
  q: string;
};

export const DEFAULT_TASK_LIST_URL_STATE: TaskListUrlState = {
  view: 'list',
  page: 1,
  q: '',
};

function parsePage(raw: string | undefined): number {
  if (!raw) return DEFAULT_TASK_LIST_URL_STATE.page;
  const parsed = Number(raw);
  // 小数・0 以下・数値でないものは 1 ページ目に倒す（offset が負になる要求を作らない）
  if (!Number.isInteger(parsed) || parsed < 1) return DEFAULT_TASK_LIST_URL_STATE.page;
  return parsed;
}

export function parseTaskListUrlState(
  search: Record<string, string | undefined> | undefined,
): TaskListUrlState & { legacyLabelId: string | null } {
  return {
    // 未知の値は既定の List へ倒す（表示形式が消えるより安全）
    view: search?.view === 'table' ? 'table' : DEFAULT_TASK_LIST_URL_STATE.view,
    page: parsePage(search?.page),
    q: legacyQuery(search),
    legacyLabelId: search?.label || null,
  };
}

/**
 * 現在の状態を URL へ書き戻す。既定値のキーは落として URL を短く保つ。
 *
 * `selected` など、この関数が扱わないクエリは触らない（選択タスクは別の関心）。
 */
export function applyTaskListUrlState(url: URL, state: TaskListUrlState): URL {
  const next = new URL(url.href);
  const entries: [string, string][] = [
    ['view', state.view === DEFAULT_TASK_LIST_URL_STATE.view ? '' : state.view],
    ['page', state.page > 1 ? String(state.page) : ''],
    ['q', state.q],
    ['label', ''],
    ['sort', ''],
  ];
  for (const [key, value] of entries) {
    if (value) next.searchParams.set(key, value);
    else next.searchParams.delete(key);
  }
  return next;
}

/**
 * 総件数が分かった時点で、範囲外のページを最後のページへ丸める。
 *
 * URL に古い `page=9` が残ったまま件数が減っていると、空のページが出て
 * 「タスクが消えた」ように見える。0 件のときは 1 ページ目のまま（丸め先が無い）。
 */
export function clampPage(page: number, total: number, pageSize: number): number {
  if (total <= 0 || pageSize <= 0) return DEFAULT_TASK_LIST_URL_STATE.page;
  const lastPage = Math.ceil(total / pageSize);
  return Math.min(Math.max(page, 1), lastPage);
}

/**
 * 一覧の状態を URL へ同期し、件数が分かったら範囲外のページを丸める。
 *
 * ページ側に watch を直接置くと、状態が増えたときに書き漏らしても気づけない
 * （型検査も lint も通る）。同期の配線ごとここへ寄せてテストで固定する。
 */
export function useTaskListUrlSync(params: {
  selectedTaskId: Ref<string | null>;
  view: Ref<TaskListView>;
  pagination: Ref<{ pageIndex: number; pageSize: number }>;
  query: Ref<string>;
  /**
   * 取得済みの総件数。**未取得は `null`**。
   *
   * 未取得と「0 件だった」を同じ 0 で表すと、丸めが走らない経路ができる。
   * watcher は即時実行されないので、空の一覧を取得しても 0 → 0 で発火せず、
   * キャッシュから同期的に得られたときも初期値のまま検査されない。
   * `?page=9` が 1 ページ目へ戻らず、空のページを出したままになる。
   */
  taskTotal: Readonly<Ref<number | null>>;
  /**
   * ページャが一覧を駆動しているか（Table 表示かつ検索していない）。
   *
   * List 表示と検索中は Table 用の一覧を取らない = 件数を持たないので、丸めを
   * 走らせると URL の `page` を 1 に潰してしまう（Table へ戻したときに失われる）。
   */
  isPagerActive: Readonly<Ref<boolean>>;
}) {
  const { selectedTaskId, view, pagination, query } = params;

  const listState = computed<TaskListUrlState>(() => ({
    view: view.value,
    page: pagination.value.pageIndex + 1,
    q: query.value,
  }));

  watch([selectedTaskId, listState], ([selected, state]) => {
    if (typeof window === 'undefined') return;
    let url = new URL(window.location.href);
    if (selected) url.searchParams.set('selected', selected);
    else url.searchParams.delete('selected');
    url = applyTaskListUrlState(url, state);
    // history を汚さない: 戻るで一覧より前へ抜けられなくなるのを避ける
    window.history.replaceState(window.history.state, '', url);
  });

  // ページャを使っていない表示（List・検索）は丸めの対象外。
  // immediate にするのは、キャッシュ由来の件数や 0 件の一覧でも初回に検査するため
  watch(
    [params.taskTotal, params.isPagerActive],
    ([total, pagerActive]) => {
      if (!pagerActive) return;
      // 未取得のあいだは待つ（0 件と区別する）
      if (total === null) return;
      const clamped = clampPage(pagination.value.pageIndex + 1, total, pagination.value.pageSize);
      if (clamped - 1 !== pagination.value.pageIndex) {
        pagination.value = { ...pagination.value, pageIndex: clamped - 1 };
      }
    },
    { immediate: true },
  );

  return { listState };
}

function legacyQuery(search: Record<string, string | undefined> | undefined): string {
  const q = search?.q?.trim() ?? '';
  if (qualifierValues(q, 'sort').length) return q;
  const entry = search?.sort?.split(',')[0];
  if (!entry) return q;
  const [id, direction] = entry.split(':');
  if (
    !['key', 'status', 'title', 'priority', 'assignee', 'due_date'].includes(id ?? '') ||
    !['asc', 'desc'].includes(direction ?? '')
  )
    return q;
  return setSort(q, [{ id: id!, desc: direction === 'desc' }]);
}
