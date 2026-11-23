/**
 * Converts a date-input value to the API's ISO datetime representation.
 *
 * A date input is a calendar date without a timezone. Treating it as local
 * midnight before converting to UTC can change the date (for example, JST
 * midnight becomes the previous day in UTC), so preserve the entered date.
 */
export function toIsoDate(value: string): string | undefined {
  return value ? `${value}T00:00:00.000Z` : undefined;
}

/**
 * チップに出す短い日付。今年は「11/18」、他の年は「2027/1/5」。値は YYYY-MM-DD。
 *
 * `new Date(value)` を通すと UTC の 0 時として読まれ、タイムゾーンによって日がずれるので、文字列を分解して組む。
 */
export function formatShortDate(value: string, today: Date = new Date()): string {
  const [year, month, day] = value.split('-').map(Number);
  if (!year || !month || !day) return value;
  return year === today.getFullYear() ? `${month}/${day}` : `${year}/${month}/${day}`;
}

/** 期限が最終期限と同じか後のときに出す文言。画面の呼び名（期限・最終期限）に合わせる。 */
export const DEADLINE_ORDER_MESSAGE = '期限は最終期限より前の日付にしてください';

/**
 * 期限が最終期限と同じ時刻か後（サーバーが 400 で弾く関係）。値は ISO の日時。
 * どちらかが無い（null / undefined / 空）なら false。Date.parse で比べる。
 *
 * 日付の文字列ではなく時刻で比べる。保存済みの期限は CLI や MCP から時刻付きで入りうるので、
 * 同じ日でもサーバーが通す組（期限が 0 時・最終期限が 12 時など）を止めないため。
 */
export function softNotBeforeHard(softIso?: string | null, hardIso?: string | null): boolean {
  if (!softIso || !hardIso) return false;
  return Date.parse(softIso) >= Date.parse(hardIso);
}
