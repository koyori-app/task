/**
 * My Tasks の「今どこを見ているか」を URL クエリで持つ（カレンダーの `url-state` と同じ流儀）。
 * タスクを開いて戻っても、同じタブと読み込んだ件数で開き直す。既定の値は URL に書かない。
 */
export const MY_TASKS_TABS = ['today', 'week', 'no_due_date', 'overdue', 'all'] as const;
export type MyTasksTab = (typeof MY_TASKS_TABS)[number];

/** 1 回に取る件数（「もっと見る」1 回分） */
export const MY_TASKS_PAGE_LIMIT = 50;
/** URL から復元する件数の上限。マイタスク API の `limit` の上限と同じ */
export const MY_TASKS_MAX_SHOWN = 200;

export type MyTasksUrlState = {
  tab: MyTasksTab;
  /** 読み込んで表示している件数。`MY_TASKS_PAGE_LIMIT` の倍数で、上限は `MY_TASKS_MAX_SHOWN` */
  shown: number;
};

const DEFAULT_MY_TASKS_URL_STATE: MyTasksUrlState = {
  tab: 'today',
  shown: MY_TASKS_PAGE_LIMIT,
};

/** `search` は Vike の `urlParsed.search` */
export function parseMyTasksUrlState(
  search: Record<string, string | undefined> | undefined,
): MyTasksUrlState {
  const rawTab = search?.tab;
  const tab = MY_TASKS_TABS.includes(rawTab as MyTasksTab)
    ? (rawTab as MyTasksTab)
    : DEFAULT_MY_TASKS_URL_STATE.tab;
  return { tab, shown: normalizeShown(search?.shown) };
}

function normalizeShown(raw: string | undefined): number {
  if (raw === undefined || !/^\d+$/.test(raw)) return MY_TASKS_PAGE_LIMIT;
  const pages = Math.ceil(Number(raw) / MY_TASKS_PAGE_LIMIT);
  return Math.min(Math.max(pages, 1) * MY_TASKS_PAGE_LIMIT, MY_TASKS_MAX_SHOWN);
}

/** 状態を URL へ書き戻す。今日のタブと 1 回分の件数は書かない。扱わないキーは触らない */
export function applyMyTasksUrlState(url: URL, state: MyTasksUrlState): URL {
  const next = new URL(url.href);
  if (state.tab === DEFAULT_MY_TASKS_URL_STATE.tab) next.searchParams.delete('tab');
  else next.searchParams.set('tab', state.tab);
  if (state.shown <= MY_TASKS_PAGE_LIMIT) next.searchParams.delete('shown');
  else next.searchParams.set('shown', String(Math.min(state.shown, MY_TASKS_MAX_SHOWN)));
  return next;
}

/** タブごとの空の文言。今日・今週・期限超過は完了済みを数えないので「未完了」と書く */
export function myTasksEmptyMessage(tab: MyTasksTab): string {
  switch (tab) {
    case 'today':
      return '今日が期限の未完了タスクはありません';
    case 'week':
      return '今週が期限の未完了タスクはありません';
    case 'overdue':
      return '期限を過ぎた未完了タスクはありません';
    case 'no_due_date':
      return '期限のないタスクはありません';
    case 'all':
      return '割り当てられたタスクは0件です';
  }
}
