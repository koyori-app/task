import { addDays, isDateString, mondayOf } from '@/lib/calendar/timeline';

/** 1 画面の日数（2 週間） */
export const CALENDAR_DAYS = 14;

/**
 * カレンダーの「今どこを見ているか」を URL クエリで持つ（タスク一覧の
 * `task-list-url-state` と同じ流儀。リロード・共有・戻るで同じ画面に戻る）。
 */
export type CalendarUrlState = {
  /** 表示範囲の起点（月曜） */
  from: string;
  memberIds: string[];
  projectIds: string[];
  /** オーバーレイで開いているタスク（seq key。例: "ENG-42"）。タスク一覧の `?selected=` と同じ */
  selected: string | null;
};

/** `searchAll` は Vike の `urlParsed.searchAll`（同じキーの複数値を持つ） */
export function parseCalendarUrlState(
  searchAll: Record<string, string[]> | undefined,
  today: string,
): CalendarUrlState {
  const rawFrom = searchAll?.from?.at(-1) ?? '';
  return {
    // 不正な日付は今週へ倒し、途中の曜日は週の月曜へ揃える（月曜始まりで固定）
    from: mondayOf(isDateString(rawFrom) ? rawFrom : today),
    memberIds: [...new Set((searchAll?.member ?? []).filter(Boolean))],
    projectIds: [...new Set((searchAll?.project ?? []).filter(Boolean))],
    selected: searchAll?.selected?.at(-1) || null,
  };
}

/** 状態を URL へ書き戻す。今週の `from` は落として URL を短く保つ。扱わないキーは触らない */
export function applyCalendarUrlState(url: URL, state: CalendarUrlState, today: string): URL {
  const next = new URL(url.href);
  if (state.from === mondayOf(today)) next.searchParams.delete('from');
  else next.searchParams.set('from', state.from);
  next.searchParams.delete('member');
  for (const id of state.memberIds) next.searchParams.append('member', id);
  next.searchParams.delete('project');
  for (const id of state.projectIds) next.searchParams.append('project', id);
  if (state.selected) next.searchParams.set('selected', state.selected);
  else next.searchParams.delete('selected');
  return next;
}

/** 前後の 2 週間へ動かす（`direction` は -1 / 1） */
export function shiftFrom(from: string, direction: number): string {
  return addDays(from, direction * CALENDAR_DAYS);
}
