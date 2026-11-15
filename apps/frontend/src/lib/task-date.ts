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
