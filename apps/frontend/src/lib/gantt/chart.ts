import type { GanttRow, GanttTask, PaletteOverrides } from '@koyori-app/arc-vue';

import type { GanttLayout } from '@/lib/gantt/layout';
import type { GanttScale } from '@/lib/gantt/view';

/** 日付の見出しの高さ。月の札、日の数字、曜日の三段が収まる */
export const HEADER_H = 48;
/** 群れの見出しの行の高さ */
export const GROUP_H = 40;

/**
 * 課題の行の高さ。狭い画面は題と添えの一行を上に置き、棒を行の下へ寄せるので高くする
 * （棒の上に字の二行が収まる高さ）。
 */
export function rowHeight(compact: boolean): number {
  return compact ? 72 : 40;
}

export const WEEKDAY_LABELS = ['月', '火', '水', '木', '金', '土', '日'];
/** 土曜と日曜（arc の数え方で 0 が月曜） */
export const WEEKEND_DAYS = [5, 6];

/** 色は library の token だけで当てる。canvas は var() を図の要素の computed style から読む */
export const GANTT_PALETTE: PaletteOverrides = {
  HeaderBg: 'var(--koyori-color-surface)',
  GroupRow: 'var(--koyori-color-surface-disabled)',
  // 群れの見出しの行の上でも土日が途切れて見えぬよう、群れの帯より一段濃い色にする
  Weekend: 'var(--koyori-color-surface-hover)',
  Grid: 'var(--koyori-color-border)',
  GridLabel: 'var(--koyori-color-text-muted)',
  BarBg: 'var(--koyori-color-accent-subtle)',
  TierLow: 'var(--koyori-color-accent)',
  TierMid: 'var(--koyori-color-accent)',
  TierHigh: 'var(--koyori-color-accent)',
  // 済みの棒は地を淡い緑で埋め、縁を濃い緑で引く（案 A の完了の行）
  TierDone: 'var(--koyori-color-success-subtle)',
  DoneBg: 'var(--koyori-color-success-subtle)',
  DoneStroke: 'var(--koyori-color-success)',
  WarnBg: 'var(--koyori-color-danger-subtle)',
  WarnStroke: 'var(--koyori-color-danger)',
  HardDeadline: 'var(--koyori-color-danger)',
  Note: 'var(--koyori-color-text-muted)',
  // 添えの字の下の帯。今日の線と格子を字の所だけ隠すので、図の地と同じ色にする
  NoteBg: 'var(--koyori-color-surface)',
  Today: 'var(--koyori-color-text)',
  ProgressLine: 'var(--koyori-color-warning)',
};

/** 標準の行の高さ。arc は棒を下へ寄せる時、この高さの行と同じ隙を棒の下に残す */
export const STANDARD_ROW_H = 40;

/**
 * arc の行。群れの見出しの行に高さを付け、狭い画面は課題の行を高くする。課題の行だけを
 * 標準より高くするのは、棒を下へ寄せた時に棒の上へ題の二行が収まる隙を作るため。
 */
export function chartRows(layout: GanttLayout, compact: boolean): GanttRow[] {
  return layout.rows.map((row) =>
    row.kind === 'group'
      ? { ...row, height: GROUP_H }
      : compact
        ? { ...row, height: rowHeight(true) }
        : row,
  );
}

/** 図の高さ（凡例は隠すので、見出しと行の高さの和） */
export function chartHeight(rows: readonly GanttRow[], compact: boolean): number {
  return (
    HEADER_H +
    rows.reduce((sum, row) => sum + (row.kind === 'group' ? GROUP_H : rowHeight(compact)), 0)
  );
}

export type ChartInput = {
  layout: GanttLayout;
  today: string;
  from: string;
  to: string;
  scale: GanttScale;
  pxPerDay: number;
  compact: boolean;
  progressLine: boolean;
  backend: 'svg' | 'canvas';
};

/**
 * arc-vue の `GanttChart` に渡す props。題の列・進みの字・凡例は隠し、左の一覧と上の凡例を
 * task の側で描く。稲妻線は点けた時だけ cmd_407 の決まり（`by_row`）で描く。
 */
export function chartProps(input: ChartInput): {
  tasks: GanttTask[];
  rows: GanttRow[];
  today: string;
  view: { from: string; to: string };
  scale: GanttScale;
  pxPerDay: number;
  weekendDays: number[];
  weekdayLabels: string[];
  rowHeight: number;
  headerHeight: number;
  barHeight: number;
  barAlign: 'middle' | 'bottom';
  rowLabels: 'hidden';
  percentLabels: 'hidden';
  legend: 'hidden';
  palette: PaletteOverrides;
  progressLine: 'by_row' | 'hidden';
  backend: 'svg' | 'canvas';
} {
  return {
    tasks: input.layout.tasks,
    rows: chartRows(input.layout, input.compact),
    today: input.today,
    view: { from: input.from, to: input.to },
    scale: input.scale,
    pxPerDay: input.pxPerDay,
    weekendDays: WEEKEND_DAYS,
    weekdayLabels: WEEKDAY_LABELS,
    rowHeight: STANDARD_ROW_H,
    headerHeight: HEADER_H,
    barHeight: input.compact ? 12 : 16,
    barAlign: input.compact ? 'bottom' : 'middle',
    rowLabels: 'hidden',
    percentLabels: 'hidden',
    legend: 'hidden',
    palette: GANTT_PALETTE,
    progressLine: input.progressLine ? 'by_row' : 'hidden',
    backend: input.backend,
  };
}
