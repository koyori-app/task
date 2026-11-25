import { isoToLocalDateInput } from '@/lib/task-display';

const WEEKDAYS = ['日', '月', '火', '水', '木', '金', '土'];
const DAY_MS = 86_400_000;

interface CalendarDate {
  year: number;
  month: number;
  day: number;
  /** 暦の日を UTC の 0 時に置いた時刻。日数の差と曜日だけに使う */
  time: number;
}

/**
 * 値の暦の日。期限は「暦の日の UTC 0 時」で保存していて、編集欄（isoToLocalDateInput）も
 * 見る人の時間帯で動かさずに読む。表示も同じ日にそろえる。
 */
function calendarDate(value: string | null | undefined): CalendarDate | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(isoToLocalDateInput(value));
  if (!match) return null;
  const [year, month, day] = [Number(match[1]), Number(match[2]), Number(match[3])];
  return { year, month, day, time: Date.UTC(year, month - 1, day) };
}

/** 見る人の今日（その人の時間帯の暦の日） */
function localToday(today: Date): CalendarDate {
  const [year, month, day] = [today.getFullYear(), today.getMonth() + 1, today.getDate()];
  return { year, month, day, time: Date.UTC(year, month - 1, day) };
}

/** 値の日が今日から何日後か（過ぎていれば負）。値が無ければ null */
export function daysFromToday(value: string | null | undefined, today: Date): number | null {
  const date = calendarDate(value);
  return date ? Math.round((date.time - localToday(today).time) / DAY_MS) : null;
}

/** `11/16（月）`。今年でない日付だけ年を付けて `2027/1/10（日）`。未設定は `—` */
export function formatDetailDate(value: string | null | undefined, today: Date): string {
  const date = calendarDate(value);
  if (!date) return '—';
  const weekday = WEEKDAYS[new Date(date.time).getUTCDay()];
  const year = date.year === today.getFullYear() ? '' : `${date.year}/`;
  return `${year}${date.month}/${date.day}（${weekday}）`;
}

/** `11/16` の短い形（注記の文に使う） */
export function formatMonthDay(value: string | null | undefined): string {
  const date = calendarDate(value);
  return date ? `${date.month}/${date.day}` : '—';
}

/**
 * 残りの日数。30 日以内なら「あと N 日」、当日は「今日」、過ぎていれば赤で「N 日過ぎ」。
 * 完了したタスクには出さない（終わった仕事を赤くすると、まだ手当てが要ると読まれる）。
 */
export function remainingLabel(
  value: string | null | undefined,
  today: Date,
  done: boolean,
): { text: string; overdue: boolean } | null {
  const days = daysFromToday(value, today);
  if (days === null || done) return null;
  if (days < 0) return { text: `${-days} 日過ぎ`, overdue: true };
  if (days === 0) return { text: '今日', overdue: false };
  if (days <= 30) return { text: `あと ${days} 日`, overdue: false };
  return null;
}
