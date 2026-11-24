import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { tokenizeTaskQuery, serializeTaskQuery, dropEmptyValues } from '../tokenize';
import { hasFilter, setQualifierValues, qualifierValues, setSort, sortOf } from '../edit';
const cases = JSON.parse(
  readFileSync('../backend/crates/service/src/task_query/fixtures/tokens.json', 'utf8'),
);
describe('shared task query grammar', () => {
  for (const c of cases)
    it(c.input, () =>
      expect(tokenizeTaskQuery(c.input).map(({ start: _s, end: _e, ...t }) => t)).toEqual(c.tokens),
    );
  it('round trips random text', () => {
    let seed = 500;
    const chars = Array.from('"\\-:, \t\n　aか🙂');
    for (let i = 0; i < 2000; i++) {
      let input = '';
      for (let j = 0; j < 32; j++) {
        seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
        input += chars[seed % chars.length];
      }
      const serialized = serializeTaskQuery(tokenizeTaskQuery(input));
      const contents = (value: string) =>
        tokenizeTaskQuery(value).map(({ start: _s, end: _e, ...t }) => t);
      expect(contents(serialized)).toEqual(contents(input));
    }
  });
  it('edits conditions and sort without other state', () => {
    let expr = setQualifierValues('通知', 'status', ['In Progress', 'Done']);
    expect(qualifierValues(expr, 'status')).toEqual(['In Progress', 'Done']);
    expr = setSort(expr, [{ id: 'priority', desc: true }]);
    expect(sortOf('sort:Title')).toEqual([{ id: 'title', desc: false }]);
    expect(sortOf(expr)).toEqual([{ id: 'priority', desc: true }]);
    expect(hasFilter(expr)).toBe(true);
    expect(hasFilter('sort:title')).toBe(false);
    expect(setQualifierValues(expr, 'status', [])).toBe('sort:priority-desc 通知');
  });
  it.each([
    ['label:bug,', 'label:bug'],
    ['label:,bug,', 'label:bug'],
    ['label:', 'label:""'],
    ['通知 label:bug, -is:', 'label:bug -is:"" 通知'],
  ])('drops empty values of %s', (input, expected) => {
    expect(serializeTaskQuery(dropEmptyValues(tokenizeTaskQuery(input)))).toBe(expected);
  });
});
