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
