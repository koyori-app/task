import { addDays, mondayOf } from '@/lib/calendar/timeline';

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

/**
 * 一日の幅の下限（px）。日の段の 30px は、日の数字と曜日が二字まで収まる幅で、幅 1280 の画面で
 * 左の一覧（320px）の右に 3 週が横に転がさずに収まる幅
 */
export function pxPerDay(scale: GanttScale): number {
  switch (scale) {
    case 'day':
      return 30;
    case 'week':
      return 12;
    case 'month':
      return 8;
  }
}

/**
 * arc が日の左右に残す余白（px）。題の列を消した図の左の余白（棒の高さの半分。広い画面の
 * 棒 16px で 8、狭い画面の棒 12px で 6 だが、大きい方で数える）と、右の余白 20 の和。
 */
export const CHART_CHROME_X = 8 + 20;

/**
 * 図の箱の幅に範囲の日を割った一日の幅。広い画面は下限を割らず、余れば広げて右の空きを無くす。
 * 狭い画面は箱に収めるのを先にする。`CHART_CHROME_X` は arc が図の左右に残す余白。
 */
export function fitDayWidth(boxWidth: number, scale: GanttScale, compact: boolean): number {
  if (!boxWidth) return pxPerDay(scale);
  const fit = Math.floor((boxWidth - CHART_CHROME_X) / viewDays(scale, compact));
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

/**
 * 今日より前に残す日数。今日の線を範囲の左の端に貼り付けず、過ぎた事（遅れた棒、過ぎた期限）も
 * 見えるようにする。案 A の板は 11/2〜11/22 に今日 11/8 を置き、今日の前に一週を残していた。
 * 狭い画面の日の段は一週しか描かぬので、今日の週を描く（案 A の 375 の板と同じ）。
 */
export function leadDays(scale: GanttScale, compact: boolean): number {
  switch (scale) {
    case 'day':
      return compact ? 0 : 7;
    case 'week':
      return compact ? 7 : 14;
    case 'month':
      return compact ? 14 : 28;
  }
}

/** 範囲を選んでおらぬ時（「今日」の button と既定）の範囲の頭。今日の週の月曜から前へ残す */
export function defaultFrom(today: string, scale: GanttScale, compact: boolean): string {
  return addDays(mondayOf(today), -leadDays(scale, compact));
}
