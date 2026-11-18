/**
 * ホーム（`/{tenant}`）のタスクの絞り込みとページを URL クエリで持つ（My Tasks の
 * `my-tasks-url-state` と同じ流儀）。タスクを開いて戻っても同じタブとページで開き直す。
 * 既定の値は URL に書かない。
 */
export const HOME_DASHBOARD_TABS = ['today', 'week', 'overdue', 'all'] as const;
export type HomeDashboardTab = (typeof HOME_DASHBOARD_TABS)[number];

export type HomeDashboardUrlState = {
  tab: HomeDashboardTab;
  /** 1 始まりのページ番号 */
  page: number;
};

export const DEFAULT_HOME_DASHBOARD_URL_STATE: HomeDashboardUrlState = { tab: 'today', page: 1 };

/** `search` は Vike の `urlParsed.search` */
export function parseHomeDashboardUrlState(
  search: Record<string, string | undefined> | undefined,
): HomeDashboardUrlState {
  const rawTab = search?.tab;
  const rawPage = search?.page;
  const page = rawPage !== undefined && /^\d+$/.test(rawPage) ? Number(rawPage) : 0;
  return {
    tab: HOME_DASHBOARD_TABS.includes(rawTab as HomeDashboardTab)
      ? (rawTab as HomeDashboardTab)
      : DEFAULT_HOME_DASHBOARD_URL_STATE.tab,
    page: Number.isSafeInteger(page) && page >= 1 ? page : DEFAULT_HOME_DASHBOARD_URL_STATE.page,
  };
}

/** 状態を URL へ書き戻す。今日のタブと 1 ページ目は書かない。扱わないキーは触らない */
export function applyHomeDashboardUrlState(url: URL, state: HomeDashboardUrlState): URL {
  const next = new URL(url.href);
  if (state.tab === DEFAULT_HOME_DASHBOARD_URL_STATE.tab) next.searchParams.delete('tab');
  else next.searchParams.set('tab', state.tab);
  if (state.page <= 1) next.searchParams.delete('page');
  else next.searchParams.set('page', String(state.page));
  return next;
}
