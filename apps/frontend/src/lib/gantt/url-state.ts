import {
  applyCalendarUrlState,
  parseCalendarUrlState,
  type CalendarUrlState,
} from '@/lib/calendar/url-state';
import { isDateString, mondayOf } from '@/lib/calendar/timeline';
import { GANTT_SCALES, type GanttGroupBy, type GanttScale } from '@/lib/gantt/view';

/**
 * ガントの「今どこを見ているか」を URL クエリで持つ。範囲の起点と担当者はカレンダーと同じ
 * `from`・`member` の名で持ち、ガントだけの切り替えを足す。既定の値は URL に書かず、短く保つ。
 */
export type GanttUrlState = Pick<CalendarUrlState, 'memberIds'> & {
  /**
   * 範囲の頭（月曜）。選んでおらねば `null` で、今日に従う（目盛りと画面の幅で決まる
   * `defaultFrom`）。今日に従う範囲は URL に書かぬので、日が替われば今日の範囲になる。
   */
  from: string | null;
  scale: GanttScale;
  groupBy: GanttGroupBy;
  /**
   * 稲妻線。URL に `line` があればそれに従い、無ければ `null`（利用者ごとに覚えた値を使う）。
   * 共有した URL を開いた者にも、開いた時の線の有無が伝わるようにするため。
   */
  progressLine: boolean | null;
};

export const DEFAULT_SCALE: GanttScale = 'day';
export const DEFAULT_GROUP_BY: GanttGroupBy = 'project';

/** `searchAll` は Vike の `urlParsed.searchAll`（同じキーの複数値を持つ） */
export function parseGanttUrlState(
  searchAll: Record<string, string[]> | undefined,
  today: string,
): GanttUrlState {
  const { memberIds } = parseCalendarUrlState(searchAll, today);
  const rawFrom = searchAll?.from?.at(-1) ?? '';
  const from = isDateString(rawFrom) ? mondayOf(rawFrom) : null;
  const rawScale = searchAll?.scale?.at(-1);
  const rawGroup = searchAll?.group?.at(-1);
  const rawLine = searchAll?.line?.at(-1);
  return {
    from,
    memberIds,
    scale: GANTT_SCALES.includes(rawScale as GanttScale) ? (rawScale as GanttScale) : DEFAULT_SCALE,
    groupBy: rawGroup === 'assignee' ? 'assignee' : DEFAULT_GROUP_BY,
    progressLine: rawLine === 'on' ? true : rawLine === 'off' ? false : null,
  };
}

/** 状態を URL へ書き戻す。扱わないキーは触らない。稲妻線は覚えた値と同じでも明に書く */
export function applyGanttUrlState(url: URL, state: GanttUrlState, today: string): URL {
  const next = applyCalendarUrlState(
    url,
    { from: mondayOf(today), memberIds: state.memberIds, projectIds: [], selected: null },
    today,
  );
  setOrDelete(next, 'from', state.from);
  setOrDelete(next, 'scale', state.scale === DEFAULT_SCALE ? null : state.scale);
  setOrDelete(next, 'group', state.groupBy === DEFAULT_GROUP_BY ? null : state.groupBy);
  setOrDelete(next, 'line', state.progressLine === null ? null : state.progressLine ? 'on' : 'off');
  return next;
}

function setOrDelete(url: URL, key: string, value: string | null) {
  if (value === null) url.searchParams.delete(key);
  else url.searchParams.set(key, value);
}
