import { afterEach, describe, expect, it } from 'vitest';

import { toIsoDate } from '../task-date';

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
