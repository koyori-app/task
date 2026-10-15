/**
 * タイムラインの日付計算。日付は `YYYY-MM-DD` の文字列のまま扱う。
 *
 * `Date` をローカル時刻で通すと実行環境のタイムゾーンで日付がずれるので、
 * 日付の足し引きと曜日は UTC の 0 時として計算し、締切（時刻つき）だけを
 * 指定のタイムゾーンで日付に落とす。
 */

const DAY_MS = 86_400_000;

function utc(date: string): Date {
  return new Date(`${date}T00:00:00Z`);
}

export function addDays(date: string, days: number): string {
  return new Date(utc(date).getTime() + days * DAY_MS).toISOString().slice(0, 10);
}

/** `to - from` の日数 */
export function diffDays(from: string, to: string): number {
  return Math.round((utc(to).getTime() - utc(from).getTime()) / DAY_MS);
}

export function isWeekend(date: string): boolean {
  const day = utc(date).getUTCDay();
  return day === 0 || day === 6;
}

/** その日を含む週の月曜（月曜始まり） */
export function mondayOf(date: string): string {
  return addDays(date, -((utc(date).getUTCDay() + 6) % 7));
}

export function isDateString(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const date = utc(value);
  // 2026-02-30 のような繰り上がる日付も弾く
  return !Number.isNaN(date.getTime()) && date.toISOString().startsWith(value);
}

/** 時刻をタイムゾーンでの日付に落とす。`en-CA` は `YYYY-MM-DD` で出る */
export function dateInZone(instant: string | Date, timeZone: string): string {
  return new Intl.DateTimeFormat('en-CA', {
    timeZone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).format(typeof instant === 'string' ? new Date(instant) : instant);
}

export function rangeDates(from: string, days: number): string[] {
  return Array.from({ length: days }, (_, i) => addDays(from, i));
}

export type Span = { start: string; end: string };

export type SpanSource = {
  planned_start_on?: string | null;
  soft_deadline?: string | null;
  hard_deadline?: string | null;
};

/**
 * タスクの始点・終点（schedule API と同じ決め方）。
 * 終点 = Soft 締切 → Hard 締切 → 開始予定日、始点 = 開始予定日 → 終点。
 */
export function taskSpan(task: SpanSource, timeZone: string): Span | null {
  const deadline = task.soft_deadline ?? task.hard_deadline;
  const end = deadline ? dateInZone(deadline, timeZone) : task.planned_start_on;
  if (!end) return null;
  const start = task.planned_start_on ?? end;
  // 開始予定日が締切より後に入っていても（サーバーは検証しない）逆向きのバーにしない
  return start <= end ? { start, end } : { start: end, end: start };
}

export type ClippedSpan = {
  /** 範囲内の列（0 始まり、両端含む） */
  startCol: number;
  endCol: number;
  clippedStart: boolean;
  clippedEnd: boolean;
};

/** 範囲 `from`〜`from + days - 1` に切り詰める。完全に外なら null */
export function clipSpan(span: Span, from: string, days: number): ClippedSpan | null {
  const start = diffDays(from, span.start);
  const end = diffDays(from, span.end);
  if (end < 0 || start > days - 1) return null;
  return {
    startCol: Math.max(start, 0),
    endCol: Math.min(end, days - 1),
    clippedStart: start < 0,
    clippedEnd: end > days - 1,
  };
}

/**
 * 重なる帯を段に分ける。始点順に並べ、最初に収まる（最後の帯の終点 < 始点）段へ詰める。
 * `start` / `end` は列番号でも日付文字列でもよい（比較できれば足りる）。
 */
export function pack<T extends { start: string | number; end: string | number }>(
  items: readonly T[],
): T[][] {
  const lanes: T[][] = [];
  const sorted = [...items].sort((a, b) =>
    a.start < b.start ? -1 : a.start > b.start ? 1 : a.end < b.end ? -1 : a.end > b.end ? 1 : 0,
  );
  for (const item of sorted) {
    const lane = lanes.find((l) => l[l.length - 1]!.end < item.start);
    if (lane) lane.push(item);
    else lanes.push([item]);
  }
  return lanes;
}
