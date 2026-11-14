import { afterEach, describe, expect, it } from 'vitest';

import { formatShortDate, toIsoDate } from '../task-date';

const originalTimezone = process.env.TZ;

afterEach(() => {
  if (originalTimezone === undefined) delete process.env.TZ;
  else process.env.TZ = originalTimezone;
});

describe('toIsoDate', () => {
  it('JSTでも入力したカレンダー日を前日にずらさない', () => {
    process.env.TZ = 'Asia/Tokyo';

    // This demonstrates the regression condition in the previous local-time
    // conversion, then verifies the timezone-independent result.
    expect(new Date('2026-07-15T00:00:00').toISOString()).toBe('2026-07-14T15:00:00.000Z');
    expect(toIsoDate('2026-07-15')).toBe('2026-07-15T00:00:00.000Z');
  });

  it('空の入力はAPI payloadから省略できる', () => {
    expect(toIsoDate('')).toBeUndefined();
  });
});

describe('formatShortDate', () => {
  const today = new Date(2026, 9, 15);

  it('今年は月/日、他の年は年/月/日で出す', () => {
    expect(formatShortDate('2026-11-18', today)).toBe('11/18');
    expect(formatShortDate('2027-01-05', today)).toBe('2027/1/5');
    expect(formatShortDate('2025-12-31', today)).toBe('2025/12/31');
  });

  it('月日の先頭の 0 を落とす', () => {
    expect(formatShortDate('2026-01-05', today)).toBe('1/5');
  });

  it('タイムゾーンに依らず入力した日のまま出す', () => {
    const original = process.env.TZ;
    process.env.TZ = 'Asia/Tokyo';
    try {
      expect(formatShortDate('2026-07-15', today)).toBe('7/15');
    } finally {
      if (original === undefined) delete process.env.TZ;
      else process.env.TZ = original;
    }
  });
});
