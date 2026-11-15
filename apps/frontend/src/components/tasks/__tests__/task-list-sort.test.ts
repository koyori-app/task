import { it, expect } from 'vitest';
import { activeTaskListSort } from '../task-list-sort';
it('validates sortable columns', () => {
  expect(activeTaskListSort([{ id: 'title', desc: false }])).toEqual({ id: 'title', desc: false });
  expect(activeTaskListSort([{ id: 'unknown', desc: false }])).toBeNull();
});
