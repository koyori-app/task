/**
 * ガントの組み（layoutGantt と chartProps）を本物の arc の wasm に通し、図に出る物を確かめる。
 * 稲妻線の決まり（cmd_407）は arc が描くが、どの行を済みとするか、線を点けるかは task の側が決めるので、
 * 組みから図までを一つに通して縛る。
 */
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { beforeAll, describe, expect, it } from 'vitest';
import init, { render_canvas_commands, render_svg } from '@koyori-app/arc';

import { chartProps, GROUP_H, HEADER_H, rowHeight, type ChartInput } from '../chart';
import { layoutGantt, type GanttSource } from '../layout';
import { viewTo } from '../view';

const TZ = 'Asia/Tokyo';
const FROM = '2026-11-09';
const TODAY = '2026-11-11';
/** 題の列を消した arc の図の左の余白（棒の高さ 16px の半分）。日の x はここから数える */
const LEFT = 8;
/** canvas の色の番号（arc の `canvas.rs`） */
const PROGRESS_LINE_COLOR = 23;

function task(over: Partial<GanttSource> & { id: string; seq_key: string }): GanttSource {
  return {
    title: over.id,
    progress_pct: 0,
    assignees: [],
    project: { id: 'p-rel', key: 'REL', name: 'Release' },
    status: { is_done_state: false },
    ...over,
  };
}

// 今日（11/11）は範囲の 3 日目。行は棒の始まり、同じならキーの順に並ぶ
const TASKS: GanttSource[] = [
  // 遅れ: 11/9〜11/20 の 10%。進みは 1.2 日目で今日より左
  task({
    id: 'late',
    seq_key: 'REL-1',
    planned_start_on: '2026-11-09',
    soft_deadline: '2026-11-20T00:00:00Z',
    progress_pct: 10,
  }),
  // 完了: 進みに関わらず今日の線のまま
  task({
    id: 'done',
    seq_key: 'REL-3',
    planned_start_on: '2026-11-09',
    soft_deadline: '2026-11-10T00:00:00Z',
    progress_pct: 50,
    status: { is_done_state: true },
  }),
  // 先行: 11/10〜11/14 の 80%。進みは 5 日目で今日より右
  task({
    id: 'ahead',
    seq_key: 'REL-2',
    planned_start_on: '2026-11-10',
    soft_deadline: '2026-11-14T00:00:00Z',
    progress_pct: 80,
  }),
  // 0% で予定開始（11/10）を過ぎた: 予定開始へ尖る
  task({
    id: 'stalled',
    seq_key: 'REL-5',
    planned_start_on: '2026-11-10',
    soft_deadline: '2026-11-20T00:00:00Z',
  }),
  // まだ始まらぬ 0%: 今日の線のまま
  task({
    id: 'future',
    seq_key: 'REL-4',
    planned_start_on: '2026-11-16',
    soft_deadline: '2026-11-18T00:00:00Z',
  }),
];

function input(over: Partial<ChartInput> = {}): ChartInput {
  return {
    layout: layoutGantt(TASKS, {
      groupBy: 'project',
      collapsed: new Set(),
      memberIds: [],
      members: [],
      timeZone: TZ,
      today: TODAY,
    }),
    today: TODAY,
    from: FROM,
    to: viewTo(FROM, 'day', false),
    scale: 'day',
    pxPerDay: 32,
    compact: false,
    progressLine: true,
    backend: 'svg',
    ...over,
  };
}

/** arc-vue の GanttChart が options_json に渡すのと同じ鍵に写す */
function render(over: Partial<ChartInput> = {}) {
  const p = chartProps(input(over));
  const options = JSON.stringify({
    today: p.today,
    view: p.view,
    scale: p.scale,
    px_per_day: p.pxPerDay,
    weekend_days: p.weekendDays,
    weekday_labels: p.weekdayLabels,
    row_height_px: p.rowHeight,
    header_height_px: p.headerHeight,
    bar_height_px: p.barHeight,
    bar_align: p.barAlign,
    row_labels: p.rowLabels,
    percent_labels: p.percentLabels,
    legend: p.legend,
    palette: p.palette,
    progress_line: p.progressLine,
  });
  const args = [JSON.stringify(p.tasks), '[]', options, JSON.stringify(p.rows)] as const;
  const buffer = JSON.parse(render_canvas_commands(...args)) as {
    error?: string;
    ops: unknown[];
  };
  return { svg: render_svg(...args), buffer };
}

/** 稲妻線の折れ線の点。無ければ null */
function progressLine(buffer: { ops: unknown[] }): [number, number][] | null {
  for (const op of buffer.ops) {
    const line = (op as { StrokePolyline?: { points: [number, number][]; color_id: number } })
      .StrokePolyline;
    if (line?.color_id === PROGRESS_LINE_COLOR) return line.points;
  }
  return null;
}

/** 課題の行 `i`（群れの見出しの下から 0 始まり）の真ん中の高さ */
function rowMid(i: number): number {
  return HEADER_H + GROUP_H + i * rowHeight(false) + rowHeight(false) / 2;
}

beforeAll(async () => {
  const pkgDir = dirname(createRequire(import.meta.url).resolve('@koyori-app/arc/package.json'));
  await init({ module_or_path: readFileSync(join(pkgDir, 'koyori_arc_core_bg.wasm')) });
});

describe('ガントの図（本物の wasm）', () => {
  it('組みが arc に断られない', () => {
    const { buffer } = render();
    expect(buffer.error).toBeUndefined();
  });

  it('稲妻線: 遅れは左、先行は右、完了とまだ始まらぬ行は今日の線、0% で開始を過ぎた行は開始へ', () => {
    const points = progressLine(render().buffer)!;
    expect(points).not.toBeNull();
    const todayX = points[0]![0];
    // 今日は範囲の 3 日目の頭
    expect(todayX).toBe(LEFT + 2 * 32);
    const bendAt = (row: number) => points.find(([, y]) => y === rowMid(row))?.[0] ?? null;
    // 行の順は late、done、ahead、stalled、future
    expect(bendAt(0)).toBeLessThan(todayX);
    expect(bendAt(1)).toBeNull();
    expect(bendAt(2)).toBeGreaterThan(todayX);
    expect(bendAt(3)).toBe(LEFT + 1 * 32);
    expect(bendAt(4)).toBeNull();
  });

  it('稲妻線は既定で切れ。切れなら描かない', () => {
    expect(progressLine(render({ progressLine: false }).buffer)).toBeNull();
  });

  it('今日が範囲の外なら稲妻線を描かない', () => {
    expect(progressLine(render({ today: '2026-12-30' }).buffer)).toBeNull();
    expect(progressLine(render({ today: '2026-11-01' }).buffer)).toBeNull();
  });

  it('目盛りは日ごとの数字と曜日、土日の地と今日の線を描く', () => {
    const { svg, buffer } = render();
    expect(svg).toContain('>水</text>');
    expect(svg).toContain('>2026/11</text>');
    // 範囲の 3 週に土日が 6 日
    expect(svg.match(/class="weekend"/g)).toHaveLength(6);
    // 今日の線は 4,3 の破線で、範囲の 3 日目の頭に立つ
    const today = buffer.ops
      .map(
        (op) =>
          (op as { StrokePolyline?: { points: [number, number][]; dash: string | null } })
            .StrokePolyline,
      )
      .find((line) => line?.dash === '4,3');
    expect(today?.points[0]![0]).toBe(LEFT + 2 * 32);
  });

  it('棒と厳守の菱形と、目安を過ぎた行の赤い縁と字を描く', () => {
    const warn = task({
      id: 'warn',
      seq_key: 'REL-9',
      planned_start_on: '2026-11-09',
      // 今日（11/11）は目安の期限の翌日
      soft_deadline: '2026-11-10T00:00:00Z',
      hard_deadline: '2026-11-12T00:00:00Z',
    });
    const layout = layoutGantt([warn], {
      groupBy: 'project',
      collapsed: new Set(),
      memberIds: [],
      members: [],
      timeZone: TZ,
      today: TODAY,
    });
    const { svg } = render({ layout });
    expect(svg).toContain('class="hard-deadline"');
    expect(svg).toContain('>目安 11/10 を過ぎた</text>');
    // 字の下に地の色の帯を敷き、今日の線の後に描く
    expect(svg).toContain('class="note-bg"');
    expect(svg.indexOf('class="note-bg"')).toBeGreaterThan(svg.indexOf('stroke-dasharray="4,3"'));
    // 赤い縁（warn の縁の色の番号は 19）
    const { buffer } = render({ layout });
    expect(
      buffer.ops.some(
        (op) => (op as { StrokePath?: { color_id: number } }).StrokePath?.color_id === 19,
      ),
    ).toBe(true);
  });

  it('範囲の前に終わった、目安を過ぎた行も、印の右に字を描き、範囲の内の厳守の菱形を描く', () => {
    // 11/2〜11/6 の課題。範囲は 11/9 から。厳守の期限 11/12 は範囲の内
    const before = task({
      id: 'before',
      seq_key: 'REL-8',
      planned_start_on: '2026-11-02',
      soft_deadline: '2026-11-06T00:00:00Z',
      hard_deadline: '2026-11-12T00:00:00Z',
      progress_pct: 40,
    });
    const layout = layoutGantt([before], {
      groupBy: 'project',
      collapsed: new Set(),
      memberIds: [],
      members: [],
      timeZone: TZ,
      today: TODAY,
    });
    const { svg } = render({ layout });
    expect(svg).toContain('>← 11/6</text>');
    expect(svg).toContain('>目安 11/6 を過ぎた</text>');
    expect(svg).toContain('class="hard-deadline"');
  });
});
