import { addDays } from '@/lib/calendar/timeline';

/** 目盛りの段。日は日ごとの数字と曜日、週は週の頭、月は月の頭に札を描く */
export type GanttScale = 'day' | 'week' | 'month';
export const GANTT_SCALES: readonly GanttScale[] = ['day', 'week', 'month'];

/** 行を何で群れにするか。既定はプロジェクト */
export type GanttGroupBy = 'project' | 'assignee';

/** 予定の API が一度に引ける日数（`to - from` は 92 日まで） */
export const SCHEDULE_MAX_DAYS = 93;

/**
 * 一画面に描く日数。どれも 7 の倍数にして、送っても範囲の頭が月曜のままになるようにする。
 * 狭い画面（`compact`）は、日の段で一日の幅が読める広さを保つために日数を減らす。
 * 月の段も予定の API の上限に収め、13 週（およそ 3 か月）にする。
 */
export function viewDays(scale: GanttScale, compact: boolean): number {
  switch (scale) {
    case 'day':
      return compact ? 7 : 21;
    case 'week':
      return compact ? 28 : 56;
    case 'month':
      return compact ? 63 : 91;
  }
}

/** 一日の幅の下限（px）。日の段の 32px は、日の数字と曜日が二字まで収まる幅 */
export function pxPerDay(scale: GanttScale): number {
  switch (scale) {
    case 'day':
      return 32;
    case 'week':
      return 12;
    case 'month':
      return 8;
  }
}

/**
 * 図の箱の幅に範囲の日を割った一日の幅。広い画面は下限を割らず、余れば広げて右の空きを無くす。
 * 狭い画面は箱に収めるのを先にする。20 は arc が図の右に残す余白。
 */
export function fitDayWidth(boxWidth: number, scale: GanttScale, compact: boolean): number {
  if (!boxWidth) return pxPerDay(scale);
  const fit = Math.floor((boxWidth - 20) / viewDays(scale, compact));
  return compact ? Math.max(2, fit) : Math.max(pxPerDay(scale), fit);
}

/** `from`（月曜）から始まる範囲の最後の日（含む） */
export function viewTo(from: string, scale: GanttScale, compact: boolean): string {
  return addDays(from, viewDays(scale, compact) - 1);
}

/** 前後の一画面へ動かす（`direction` は -1 / 1） */
export function shiftView(
  from: string,
  scale: GanttScale,
  compact: boolean,
  direction: number,
): string {
  return addDays(from, direction * viewDays(scale, compact));
}

/** 狭い画面で、canvas に替える行の数の目安。これを越える行を SVG の要素にすると重い */
export const COMPACT_SVG_MAX_ROWS = 40;

/**
 * 図を SVG で描くか canvas で描くか。広い画面は SVG（字を選べ、要素が読み上げに出る）。
 * 狭い画面は、行が多いか、図の高さが画面の 3 つ分を越える時だけ canvas にする。
 */
export function chartBackend(options: {
  compact: boolean;
  rowCount: number;
  chartHeight: number;
  screenHeight: number;
}): 'svg' | 'canvas' {
  if (!options.compact) return 'svg';
  const tall = options.screenHeight > 0 && options.chartHeight > options.screenHeight * 3;
  return options.rowCount > COMPACT_SVG_MAX_ROWS || tall ? 'canvas' : 'svg';
}
