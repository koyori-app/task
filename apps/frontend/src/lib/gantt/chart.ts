import type { GanttDep, GanttRow, GanttTask, PaletteOverrides } from '@koyori-app/arc-vue';

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
  // 群れの帯と土日の帯は、surface の段のうち互いに一番離れる二つにする。どの組でも明度の
  // 対比は小さいので、群れの帯を一段強く（暗い表示で明るく、明るい表示で暗く）して紛れぬようにする
  GroupRow: 'var(--koyori-color-surface-active)',
  Weekend: 'var(--koyori-color-surface-disabled)',
  Grid: 'var(--koyori-color-border)',
  GridLabel: 'var(--koyori-color-text-muted)',
  // 日の段の日の数字。渡さないと arc は字に色を書かず、暗い theme の見出しで黒く潰れる
  DayNumber: 'var(--koyori-color-text)',
  // 未着手の棒の地。図の地とも土日の帯とも非文字の対比 3:1 を越える段にする（accent-subtle は
  // 暗い表示で図の地に溶けた）。進み（accent）との対比も 4.5:1 を越える
  BarBg: 'var(--koyori-color-border-strong)',
  TierLow: 'var(--koyori-color-accent)',
  TierMid: 'var(--koyori-color-accent)',
  TierHigh: 'var(--koyori-color-accent)',
  // 済みの棒は地を淡い緑で埋め、縁と進みを濃い緑で引く（案 A の完了の行）。進みを地と同じ色に
  // すると、暗い表示で進みの濃い所が見えぬ
  TierDone: 'var(--koyori-color-success)',
  DoneBg: 'var(--koyori-color-success-subtle)',
  DoneStroke: 'var(--koyori-color-success)',
  WarnBg: 'var(--koyori-color-danger-subtle)',
  WarnStroke: 'var(--koyori-color-danger)',
  HardDeadline: 'var(--koyori-color-danger)',
  // 添えの字と範囲の外の印の字。帯（図の地）の上で、明暗とも 4.5:1 を大きく越える段にする
  Note: 'var(--koyori-color-text-secondary)',
  // 添えの字の下の帯。今日の線と格子を字の所だけ隠すので、図の地と同じ色にする
  NoteBg: 'var(--koyori-color-surface)',
  // 今日の線は稲妻線（warning の実線）と色相で分ける。稲妻線は今日の線の上の層に描かれ、ずれの無い
  // 行では今日の線に重なるので、字の色（白・黒）では重なった所が橙の点線に見えた
  Today: 'var(--koyori-color-info)',
  ProgressLine: 'var(--koyori-color-warning)',
  // 依存の矢印（cmd_426 の板）。普通は info、待ちで遅れは danger の破線（破線は arc が描く）、
  // 済みは text-disabled で退かせる
  Dep: 'var(--koyori-color-info)',
  DepDelayed: 'var(--koyori-color-danger)',
  DepDone: 'var(--koyori-color-text-disabled)',
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
  /** 依存の辺（`lib/gantt/dependencies.ts`）。狭い画面では描かず、行の添えに印を出す */
  deps: GanttDep[];
  /** 頁を縦に巻く外の域（`scrollParentOf`）。まだ組まれておらぬ間は null */
  scrollContainer: HTMLElement | Window | null;
};

/**
 * arc-vue の `GanttChart` に渡す props。題の列・進みの字・凡例は隠し、左の一覧と上の凡例を
 * task の側で描く。稲妻線は点けた時だけ cmd_407 の決まり（`by_row`）で描く。
 *
 * 外のスクロール域（`scrollContainer`）を渡す。渡さぬと、arc-vue は低性能と判じた端末で図だけを
 * 高さ min(70vh, 600px) の内側のスクロール域に閉じ込め、左の一覧と狭い画面の重ね文字はその外に
 * あるので、図だけが縦に動いて棒が別の課題の名と並ぶ。渡せば図は行の高さのままで、一覧・重ね文字・
 * 図が頁の一つの縦のスクロールで一緒に動き、低性能の端末ではその域に見える行だけを描く。
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
  deps: GanttDep[];
  scrollContainer: HTMLElement | Window | null;
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
    // 狭い画面では矢印を描かず、行の添えに「依存 N」と「<キー> 待ち」を出す
    deps: input.compact ? [] : input.deps,
    scrollContainer: input.scrollContainer,
  };
}
