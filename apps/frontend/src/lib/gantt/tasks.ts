import { taskSpan, type SpanSource } from '@/lib/calendar/timeline';

/** ガントに渡す棒。`@koyori-app/arc-vue` の `GanttTask` と同じ形（日付は `YYYY-MM-DD` のまま渡す） */
export type GanttBar = {
  id: string;
  title: string;
  progress_pct: number;
  start: string;
  end: string;
};

export type GanttSource = SpanSource & {
  id: string;
  title: string;
  progress_pct: number;
};

/**
 * 予定の課題をガントの棒に写す。
 *
 * 棒の始まりと終わりはカレンダーと同じ `taskSpan` から取る。同じ課題がカレンダーとガントで
 * 違う日に終わらないようにするためで、ここで `hard_deadline` と `soft_deadline` の優先を
 * 組み直さない。`taskSpan` は締切をタイムゾーンの暦の日に落とすので、日付の字をそのまま渡せば
 * ガント側で時差を持たずに済む。期間を持たない課題（締切も開始予定日も無い）は棒にできず、
 * 除いた数を `skipped` で返す。
 */
export function toGanttBars(
  tasks: readonly GanttSource[],
  timeZone: string,
): { bars: GanttBar[]; skipped: number } {
  const bars: GanttBar[] = [];
  for (const task of tasks) {
    const span = taskSpan(task, timeZone);
    if (!span) continue;
    bars.push({
      id: task.id,
      title: task.title,
      progress_pct: task.progress_pct,
      start: span.start,
      end: span.end,
    });
  }
  return { bars, skipped: tasks.length - bars.length };
}
